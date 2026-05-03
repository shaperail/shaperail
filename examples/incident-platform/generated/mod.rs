#![allow(dead_code)]

pub mod alerts;
pub mod attachments;
pub mod incidents;
pub mod services;

#[path = "../resources/alerts.controller.rs"]
pub mod alerts_controller;
#[path = "../resources/incidents.controller.rs"]
pub mod incidents_controller;
#[path = "../resources/services.controller.rs"]
pub mod services_controller;

#[path = "../jobs/link_alert_to_incident.rs"]
mod job_link_alert_to_incident;
#[path = "../jobs/notify_on_call.rs"]
mod job_notify_on_call;
#[path = "../jobs/refresh_incident_cache.rs"]
mod job_refresh_incident_cache;
#[path = "../jobs/scan_attachment.rs"]
mod job_scan_attachment;

/// Re-exports of every controller module under one path so integration
/// tests can reach `pub` helpers via `crate::resources::<name>_controller::*`.
#[doc(hidden)]
#[allow(unused_imports)]
pub mod resources {
    pub use super::alerts_controller;
    pub use super::incidents_controller;
    pub use super::services_controller;
}

pub fn build_store_registry(pool: sqlx::PgPool) -> shaperail_runtime::db::StoreRegistry {
    let mut stores: std::collections::HashMap<
        String,
        std::sync::Arc<dyn shaperail_runtime::db::ResourceStore>,
    > = std::collections::HashMap::new();
    stores.insert(
        "alerts".to_string(),
        std::sync::Arc::new(alerts::AlertsStore::new(pool.clone())),
    );
    stores.insert(
        "attachments".to_string(),
        std::sync::Arc::new(attachments::AttachmentsStore::new(pool.clone())),
    );
    stores.insert(
        "incidents".to_string(),
        std::sync::Arc::new(incidents::IncidentsStore::new(pool.clone())),
    );
    stores.insert(
        "services".to_string(),
        std::sync::Arc::new(services::ServicesStore::new(pool.clone())),
    );
    std::sync::Arc::new(stores)
}

pub fn build_controller_map() -> shaperail_runtime::handlers::controller::ControllerMap {
    let mut map = shaperail_runtime::handlers::controller::ControllerMap::new();
    map.register("alerts", "ingest_alert", alerts_controller::ingest_alert);
    map.register(
        "alerts",
        "reconcile_alert_link",
        alerts_controller::reconcile_alert_link,
    );
    map.register(
        "incidents",
        "open_incident",
        incidents_controller::open_incident,
    );
    map.register(
        "incidents",
        "enforce_incident_update",
        incidents_controller::enforce_incident_update,
    );
    map.register(
        "incidents",
        "write_incident_audit",
        incidents_controller::write_incident_audit,
    );
    map.register(
        "services",
        "prepare_service",
        services_controller::prepare_service,
    );
    map
}

pub fn build_job_registry() -> shaperail_runtime::jobs::JobRegistry {
    let mut handlers: std::collections::HashMap<String, shaperail_runtime::jobs::JobHandler> =
        std::collections::HashMap::new();
    handlers.insert(
        "link_alert_to_incident".to_string(),
        std::sync::Arc::new(|payload: serde_json::Value| {
            Box::pin(job_link_alert_to_incident::handle(payload))
                as std::pin::Pin<
                    Box<
                        dyn std::future::Future<Output = Result<(), shaperail_core::ShaperailError>>
                            + Send,
                    >,
                >
        }) as shaperail_runtime::jobs::JobHandler,
    );
    handlers.insert(
        "notify_on_call".to_string(),
        std::sync::Arc::new(|payload: serde_json::Value| {
            Box::pin(job_notify_on_call::handle(payload))
                as std::pin::Pin<
                    Box<
                        dyn std::future::Future<Output = Result<(), shaperail_core::ShaperailError>>
                            + Send,
                    >,
                >
        }) as shaperail_runtime::jobs::JobHandler,
    );
    handlers.insert(
        "refresh_incident_cache".to_string(),
        std::sync::Arc::new(|payload: serde_json::Value| {
            Box::pin(job_refresh_incident_cache::handle(payload))
                as std::pin::Pin<
                    Box<
                        dyn std::future::Future<Output = Result<(), shaperail_core::ShaperailError>>
                            + Send,
                    >,
                >
        }) as shaperail_runtime::jobs::JobHandler,
    );
    handlers.insert(
        "scan_attachment".to_string(),
        std::sync::Arc::new(|payload: serde_json::Value| {
            Box::pin(job_scan_attachment::handle(payload))
                as std::pin::Pin<
                    Box<
                        dyn std::future::Future<Output = Result<(), shaperail_core::ShaperailError>>
                            + Send,
                    >,
                >
        }) as shaperail_runtime::jobs::JobHandler,
    );
    shaperail_runtime::jobs::JobRegistry::from_handlers(handlers)
}

pub fn build_handler_map() -> shaperail_runtime::handlers::custom::CustomHandlerMap {
    shaperail_runtime::handlers::custom::CustomHandlerMap::new()
}
