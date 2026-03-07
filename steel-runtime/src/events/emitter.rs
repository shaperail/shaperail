use std::sync::Arc;

use serde::{Deserialize, Serialize};
use steel_core::{EventSubscriber, EventTarget, EventsConfig, SteelError};

use crate::jobs::{JobPriority, JobQueue};

/// An emitted event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    /// Event name (e.g., "users.created").
    pub event: String,
    /// Resource name.
    pub resource: String,
    /// Action that triggered the event.
    pub action: String,
    /// The record data associated with the event.
    pub data: serde_json::Value,
    /// Timestamp of emission.
    pub timestamp: String,
    /// Unique event ID.
    pub event_id: String,
}

/// Non-blocking event emitter that dispatches events via the job queue.
///
/// Events never block the HTTP response — they are always enqueued as jobs.
#[derive(Clone)]
pub struct EventEmitter {
    job_queue: JobQueue,
    subscribers: Arc<Vec<EventSubscriber>>,
}

impl EventEmitter {
    /// Creates a new event emitter.
    pub fn new(job_queue: JobQueue, config: Option<&EventsConfig>) -> Self {
        let subscribers = config.map(|c| c.subscribers.clone()).unwrap_or_default();
        Self {
            job_queue,
            subscribers: Arc::new(subscribers),
        }
    }

    /// Emits an event non-blockingly via the job queue.
    ///
    /// This enqueues processing jobs for:
    /// 1. The event log (always)
    /// 2. Each matching subscriber target
    pub async fn emit(
        &self,
        event_name: &str,
        resource: &str,
        action: &str,
        data: serde_json::Value,
    ) -> Result<String, SteelError> {
        let event_id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        let payload = EventPayload {
            event: event_name.to_string(),
            resource: resource.to_string(),
            action: action.to_string(),
            data,
            timestamp,
            event_id: event_id.clone(),
        };

        let payload_json = serde_json::to_value(&payload)
            .map_err(|e| SteelError::Internal(format!("Failed to serialize event: {e}")))?;

        // Always log the event
        self.job_queue
            .enqueue("steel:event_log", payload_json.clone(), JobPriority::Normal)
            .await?;

        // Dispatch to matching subscribers
        let matching = self.find_matching_subscribers(event_name);
        for subscriber in matching {
            for target in &subscriber.targets {
                self.dispatch_to_target(target, &payload_json).await?;
            }
        }

        tracing::info!(
            event = event_name,
            event_id = %event_id,
            resource = resource,
            action = action,
            "Event emitted"
        );

        Ok(event_id)
    }

    /// Finds subscribers whose event pattern matches the given event name.
    fn find_matching_subscribers(&self, event_name: &str) -> Vec<&EventSubscriber> {
        self.subscribers
            .iter()
            .filter(|s| event_matches(&s.event, event_name))
            .collect()
    }

    /// Dispatches an event payload to a single target via the job queue.
    async fn dispatch_to_target(
        &self,
        target: &EventTarget,
        payload: &serde_json::Value,
    ) -> Result<(), SteelError> {
        match target {
            EventTarget::Job { name } => {
                self.job_queue
                    .enqueue(name, payload.clone(), JobPriority::Normal)
                    .await?;
            }
            EventTarget::Webhook { url } => {
                let webhook_payload = serde_json::json!({
                    "url": url,
                    "payload": payload,
                });
                self.job_queue
                    .enqueue("steel:webhook_deliver", webhook_payload, JobPriority::High)
                    .await?;
            }
            EventTarget::Channel { name, room } => {
                let channel_payload = serde_json::json!({
                    "channel": name,
                    "room": room,
                    "payload": payload,
                });
                self.job_queue
                    .enqueue(
                        "steel:channel_broadcast",
                        channel_payload,
                        JobPriority::High,
                    )
                    .await?;
            }
            EventTarget::Hook { name } => {
                let hook_payload = serde_json::json!({
                    "hook": name,
                    "payload": payload,
                });
                self.job_queue
                    .enqueue("steel:hook_execute", hook_payload, JobPriority::Normal)
                    .await?;
            }
        }
        Ok(())
    }
}

/// Matches an event name against a pattern.
///
/// Supports:
/// - Exact match: "users.created" matches "users.created"
/// - Wildcard prefix: "*.created" matches "users.created"
/// - Wildcard suffix: "users.*" matches "users.created"
/// - Full wildcard: "*" matches everything
fn event_matches(pattern: &str, event_name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern == event_name {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return event_name.ends_with(&format!(".{suffix}"));
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        return event_name.starts_with(&format!("{prefix}."));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        assert!(event_matches("users.created", "users.created"));
        assert!(!event_matches("users.created", "users.updated"));
    }

    #[test]
    fn wildcard_all() {
        assert!(event_matches("*", "users.created"));
        assert!(event_matches("*", "orders.deleted"));
    }

    #[test]
    fn wildcard_prefix() {
        assert!(event_matches("*.created", "users.created"));
        assert!(event_matches("*.created", "orders.created"));
        assert!(!event_matches("*.created", "users.deleted"));
    }

    #[test]
    fn wildcard_suffix() {
        assert!(event_matches("users.*", "users.created"));
        assert!(event_matches("users.*", "users.deleted"));
        assert!(!event_matches("users.*", "orders.created"));
    }

    #[test]
    fn no_partial_match() {
        assert!(!event_matches("user", "users.created"));
        assert!(!event_matches("users.create", "users.created"));
    }

    #[test]
    fn event_payload_serde_roundtrip() {
        let payload = EventPayload {
            event: "users.created".to_string(),
            resource: "users".to_string(),
            action: "created".to_string(),
            data: serde_json::json!({"id": "123", "name": "Alice"}),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            event_id: "evt-001".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: EventPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back.event, "users.created");
        assert_eq!(back.event_id, "evt-001");
    }
}
