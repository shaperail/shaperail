use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use shaperail_core::{AuthRule, ChannelDefinition, ShaperailError, WsServerMessage};
use shaperail_runtime::auth::api_key::ApiKeyStore;
use shaperail_runtime::cache::RedisCache;
use shaperail_runtime::events::{
    EventLog, WebhookDeliveryLog, WebhookDeliveryRecord, WebhookDispatcher,
};
use shaperail_runtime::handlers::ControllerMap;
use shaperail_runtime::jobs::{JobHandler, JobRegistry};
use shaperail_runtime::ws::{PubSubMessage, RedisPubSub, RoomManager};

use crate::generated::{alerts_controller, incidents_controller, services_controller};

#[derive(Debug, Clone, serde::Deserialize)]
struct EventPayload {
    event: String,
    resource: String,
    action: String,
    data: serde_json::Value,
    timestamp: String,
    event_id: String,
}

fn boxed_job_handler<F, Fut>(f: F) -> JobHandler
where
    F: Fn(serde_json::Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), ShaperailError>> + Send + 'static,
{
    Arc::new(move |payload| {
        Box::pin(f(payload)) as Pin<Box<dyn Future<Output = Result<(), ShaperailError>> + Send>>
    })
}

pub fn build_api_key_store() -> ApiKeyStore {
    let mut store = ApiKeyStore::new();
    let primary_user_id = "00000000-0000-0000-0000-00000000a001".to_string();
    let backup_user_id = "00000000-0000-0000-0000-00000000a002".to_string();

    if let Ok(primary_key) = std::env::var("INCIDENT_INGEST_KEY") {
        store.insert(primary_key, primary_user_id, "ingest".to_string());
    }

    if let Ok(backup_key) = std::env::var("INCIDENT_INGEST_KEY_BACKUP") {
        store.insert(backup_key, backup_user_id, "ingest".to_string());
    }

    store
}

pub fn incident_channel_definition() -> ChannelDefinition {
    ChannelDefinition {
        channel: "incidents".to_string(),
        auth: Some(AuthRule::Roles(vec![
            "member".to_string(),
            "admin".to_string(),
        ])),
        rooms: true,
        hooks: None,
    }
}

pub fn build_controller_map() -> ControllerMap {
    let mut controllers = ControllerMap::new();

    controllers.register(
        "services",
        "prepare_service",
        services_controller::prepare_service,
    );
    controllers.register(
        "incidents",
        "open_incident",
        incidents_controller::open_incident,
    );
    controllers.register(
        "incidents",
        "enforce_incident_update",
        incidents_controller::enforce_incident_update,
    );
    controllers.register(
        "incidents",
        "write_incident_audit",
        incidents_controller::write_incident_audit,
    );
    controllers.register("alerts", "ingest_alert", alerts_controller::ingest_alert);
    controllers.register(
        "alerts",
        "reconcile_alert_link",
        alerts_controller::reconcile_alert_link,
    );

    controllers
}

pub fn build_job_registry(
    pool: sqlx::PgPool,
    cache: Option<RedisCache>,
    room_manager: Option<RoomManager>,
    pubsub: Option<RedisPubSub>,
    webhook_dispatcher: Option<WebhookDispatcher>,
) -> JobRegistry {
    let event_log = EventLog::new(pool.clone());
    let delivery_log = WebhookDeliveryLog::new(pool.clone());
    let mut handlers: HashMap<String, JobHandler> = HashMap::new();

    handlers.insert(
        "notify_on_call".to_string(),
        boxed_job_handler(|payload| async move {
            let title = payload
                .pointer("/data/title")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown incident");
            let severity = payload
                .pointer("/data/severity")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            tracing::info!(
                incident = title,
                severity = severity,
                "Queued on-call notification"
            );
            Ok(())
        }),
    );

    let cache_for_refresh = cache.clone();
    handlers.insert(
        "refresh_incident_cache".to_string(),
        boxed_job_handler(move |_payload| {
            let cache = cache_for_refresh.clone();
            async move {
                if let Some(cache) = cache {
                    cache.invalidate_resource("incidents").await;
                    cache.invalidate_resource("alerts").await;
                    cache.invalidate_resource("services").await;
                }
                Ok(())
            }
        }),
    );

    let pool_for_link = pool.clone();
    handlers.insert(
        "link_alert_to_incident".to_string(),
        boxed_job_handler(move |payload| {
            let pool = pool_for_link.clone();
            async move {
                let alert_id = match payload.pointer("/data/id").and_then(|value| value.as_str()) {
                    Some(value) => value,
                    None => return Ok(()),
                };
                let org_id = match payload
                    .pointer("/data/org_id")
                    .and_then(|value| value.as_str())
                {
                    Some(value) => value,
                    None => return Ok(()),
                };
                let service_id = match payload
                    .pointer("/data/service_id")
                    .and_then(|value| value.as_str())
                {
                    Some(value) => value,
                    None => return Ok(()),
                };

                let candidate: Option<String> = sqlx::query_scalar(
                    "SELECT id::text
                     FROM incidents
                     WHERE org_id = $1::uuid
                       AND service_id = $2::uuid
                       AND status IN ('open', 'acknowledged', 'mitigated')
                       AND deleted_at IS NULL
                     ORDER BY created_at DESC
                     LIMIT 1",
                )
                .bind(org_id)
                .bind(service_id)
                .fetch_optional(&pool)
                .await
                .map_err(|error| {
                    ShaperailError::Internal(format!(
                        "Failed to resolve incident for alert linking: {error}"
                    ))
                })?;

                if let Some(incident_id) = candidate {
                    sqlx::query(
                        "UPDATE alerts
                         SET incident_id = $1::uuid, status = 'linked', updated_at = NOW()
                         WHERE id = $2::uuid",
                    )
                    .bind(incident_id)
                    .bind(alert_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| {
                        ShaperailError::Internal(format!(
                            "Failed to update alert incident linkage: {error}"
                        ))
                    })?;
                }

                Ok(())
            }
        }),
    );

    handlers.insert(
        "scan_attachment".to_string(),
        boxed_job_handler(|payload| async move {
            let file_url = payload
                .pointer("/data/file_url")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            tracing::info!(file_url = file_url, "Queued attachment scan");
            Ok(())
        }),
    );

    let cache_for_projection = cache.clone();
    let pubsub_for_projection = pubsub.clone();
    let room_manager_for_projection = room_manager.clone();
    handlers.insert(
        "write_status_projection".to_string(),
        boxed_job_handler(move |payload| {
            let cache = cache_for_projection.clone();
            let pubsub = pubsub_for_projection.clone();
            let room_manager = room_manager_for_projection.clone();
            async move {
                let event: EventPayload = serde_json::from_value(payload).map_err(|error| {
                    ShaperailError::Internal(format!(
                        "Failed to decode event payload for status projection: {error}"
                    ))
                })?;

                if let Some(cache) = cache {
                    cache.invalidate_resource("incidents").await;
                    cache.invalidate_resource("services").await;
                }

                let room = event
                    .data
                    .get("room_key")
                    .and_then(|value| value.as_str())
                    .unwrap_or("all-incidents")
                    .to_string();

                if let Some(pubsub) = pubsub {
                    pubsub
                        .publish(&PubSubMessage {
                            channel: "incidents".to_string(),
                            room,
                            event: event.event,
                            data: event.data,
                        })
                        .await?;
                } else if let Some(room_manager) = room_manager {
                    let message = WsServerMessage::Broadcast {
                        room: room.clone(),
                        event: event.event,
                        data: event.data,
                    };
                    let text = serde_json::to_string(&message).map_err(|error| {
                        ShaperailError::Internal(format!(
                            "Failed to serialize local room broadcast: {error}"
                        ))
                    })?;
                    room_manager.broadcast_to_room(&room, &text).await;
                }

                Ok(())
            }
        }),
    );

    handlers.insert(
        "open_incident_from_webhook".to_string(),
        boxed_job_handler(|payload| async move {
            let event: EventPayload = serde_json::from_value(payload).map_err(|error| {
                ShaperailError::Internal(format!(
                    "Failed to decode inbound webhook event payload: {error}"
                ))
            })?;
            tracing::info!(event = event.event, "Inbound incident webhook received");
            Ok(())
        }),
    );

    let event_log_for_handler = event_log.clone();
    handlers.insert(
        "shaperail:event_log".to_string(),
        boxed_job_handler(move |payload| {
            let event_log = event_log_for_handler.clone();
            async move {
                let event: EventPayload = serde_json::from_value(payload).map_err(|error| {
                    ShaperailError::Internal(format!("Failed to decode event log payload: {error}"))
                })?;
                event_log
                    .append(&shaperail_runtime::events::EventRecord {
                        event_id: event.event_id,
                        event: event.event,
                        resource: event.resource,
                        action: event.action,
                        data: event.data,
                        timestamp: event.timestamp,
                    })
                    .await
            }
        }),
    );

    let delivery_log_for_handler = delivery_log.clone();
    handlers.insert(
        "shaperail:webhook_deliver".to_string(),
        boxed_job_handler(move |payload| {
            let dispatcher = webhook_dispatcher.clone();
            let delivery_log = delivery_log_for_handler.clone();
            async move {
                let url = payload
                    .get("url")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| {
                        ShaperailError::Internal(
                            "Webhook delivery job missing target url".to_string(),
                        )
                    })?;
                let inner_payload = payload.get("payload").cloned().ok_or_else(|| {
                    ShaperailError::Internal(
                        "Webhook delivery job missing event payload".to_string(),
                    )
                })?;

                if let Some(dispatcher) = dispatcher {
                    let request = dispatcher.build_delivery_request(url, &inner_payload)?;
                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(request.timeout_secs))
                        .build()
                        .map_err(|error| {
                            ShaperailError::Internal(format!(
                                "Failed to build webhook client: {error}"
                            ))
                        })?;
                    let target_url = request.url.clone();
                    let started_at = Instant::now();
                    let (status_code, success, error, latency_ms) = match client
                        .post(&target_url)
                        .header("Content-Type", "application/json")
                        .header("X-Shaperail-Signature", request.signature_header())
                        .body(request.body)
                        .send()
                        .await
                    {
                        Ok(response) => {
                            let status_code = response.status().as_u16();
                            let success = response.status().is_success();
                            (
                                status_code,
                                success,
                                if success {
                                    None
                                } else {
                                    Some(format!("HTTP {status_code}"))
                                },
                                started_at.elapsed().as_millis() as u64,
                            )
                        }
                        Err(error) => (
                            0,
                            false,
                            Some(error.to_string()),
                            started_at.elapsed().as_millis() as u64,
                        ),
                    };
                    let event_id = inner_payload
                        .get("event_id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("unknown");

                    delivery_log
                        .record(&WebhookDeliveryRecord {
                            delivery_id: uuid::Uuid::new_v4().to_string(),
                            event_id: event_id.to_string(),
                            url: target_url,
                            status_code: i32::from(status_code),
                            status: if success {
                                "success".to_string()
                            } else {
                                "failed".to_string()
                            },
                            latency_ms: latency_ms as i64,
                            error,
                            attempt: 1,
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        })
                        .await?;
                } else {
                    tracing::warn!(
                        url = url,
                        "Skipping webhook delivery: no dispatcher configured"
                    );
                }

                Ok(())
            }
        }),
    );

    let pubsub_for_channel = pubsub.clone();
    let room_manager_for_channel = room_manager.clone();
    handlers.insert(
        "shaperail:channel_broadcast".to_string(),
        boxed_job_handler(move |payload| {
            let pubsub = pubsub_for_channel.clone();
            let room_manager = room_manager_for_channel.clone();
            async move {
                let channel = payload
                    .get("channel")
                    .and_then(|value| value.as_str())
                    .unwrap_or("incidents");
                let room = payload
                    .get("room")
                    .and_then(|value| value.as_str())
                    .unwrap_or("all-incidents");
                let event = payload
                    .pointer("/payload/event")
                    .and_then(|value| value.as_str())
                    .unwrap_or("event")
                    .to_string();
                let data = payload
                    .get("payload")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));

                if let Some(pubsub) = pubsub {
                    pubsub
                        .publish(&PubSubMessage {
                            channel: channel.to_string(),
                            room: room.to_string(),
                            event,
                            data,
                        })
                        .await?;
                } else if let Some(room_manager) = room_manager {
                    let message = WsServerMessage::Broadcast {
                        room: room.to_string(),
                        event,
                        data,
                    };
                    let text = serde_json::to_string(&message).map_err(|error| {
                        ShaperailError::Internal(format!(
                            "Failed to serialize channel broadcast: {error}"
                        ))
                    })?;
                    room_manager.broadcast_to_room(room, &text).await;
                }

                Ok(())
            }
        }),
    );

    handlers.insert(
        "shaperail:hook_execute".to_string(),
        boxed_job_handler(|payload| async move {
            let hook_name = payload
                .get("hook")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            tracing::info!(hook = hook_name, "Hook execution is application-defined");
            Ok(())
        }),
    );

    JobRegistry::from_handlers(handlers)
}
