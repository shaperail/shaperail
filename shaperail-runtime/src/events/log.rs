use serde::{Deserialize, Serialize};
use shaperail_core::ShaperailError;
use sqlx::Row;

/// An append-only event log record for audit and replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    /// Unique event ID.
    pub event_id: String,
    /// Event name (e.g., "users.created").
    pub event: String,
    /// Resource name.
    pub resource: String,
    /// Action that triggered the event.
    pub action: String,
    /// Event data payload.
    pub data: serde_json::Value,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}

/// Append-only event log backed by the database.
///
/// Stores all emitted events for audit trails and replay.
/// Uses an append-only pattern — no UPDATE or DELETE operations.
#[derive(Clone)]
pub struct EventLog {
    pool: sqlx::PgPool,
}

impl EventLog {
    /// Creates a new event log.
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Appends an event record to the log.
    pub async fn append(&self, record: &EventRecord) -> Result<(), ShaperailError> {
        sqlx::query(
            r#"INSERT INTO shaperail_event_log (event_id, event, resource, action, data, timestamp)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(&record.event_id)
        .bind(&record.event)
        .bind(&record.resource)
        .bind(&record.action)
        .bind(&record.data)
        .bind(&record.timestamp)
        .execute(&self.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Failed to log event: {e}")))?;

        Ok(())
    }

    /// Retrieves recent events, ordered by timestamp descending.
    pub async fn recent(&self, limit: i64) -> Result<Vec<EventRecord>, ShaperailError> {
        let rows = sqlx::query(
            r#"SELECT event_id, event, resource, action, data, timestamp
               FROM shaperail_event_log
               ORDER BY timestamp DESC
               LIMIT $1"#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Failed to query event log: {e}")))?;

        rows.iter().map(row_to_event_record).collect()
    }

    /// Retrieves events for a specific resource.
    pub async fn for_resource(
        &self,
        resource: &str,
        limit: i64,
    ) -> Result<Vec<EventRecord>, ShaperailError> {
        let rows = sqlx::query(
            r#"SELECT event_id, event, resource, action, data, timestamp
               FROM shaperail_event_log
               WHERE resource = $1
               ORDER BY timestamp DESC
               LIMIT $2"#,
        )
        .bind(resource)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Failed to query event log: {e}")))?;

        rows.iter().map(row_to_event_record).collect()
    }
}

fn row_to_event_record(row: &sqlx::postgres::PgRow) -> Result<EventRecord, ShaperailError> {
    Ok(EventRecord {
        event_id: row
            .try_get("event_id")
            .map_err(|e| ShaperailError::Internal(format!("Missing event_id column: {e}")))?,
        event: row
            .try_get("event")
            .map_err(|e| ShaperailError::Internal(format!("Missing event column: {e}")))?,
        resource: row
            .try_get("resource")
            .map_err(|e| ShaperailError::Internal(format!("Missing resource column: {e}")))?,
        action: row
            .try_get("action")
            .map_err(|e| ShaperailError::Internal(format!("Missing action column: {e}")))?,
        data: row
            .try_get("data")
            .map_err(|e| ShaperailError::Internal(format!("Missing data column: {e}")))?,
        timestamp: row
            .try_get("timestamp")
            .map_err(|e| ShaperailError::Internal(format!("Missing timestamp column: {e}")))?,
    })
}

/// Webhook delivery status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDeliveryRecord {
    /// Unique delivery ID.
    pub delivery_id: String,
    /// The event ID that triggered this delivery.
    pub event_id: String,
    /// Target webhook URL.
    pub url: String,
    /// HTTP status code from the target (0 if connection failed).
    pub status_code: i32,
    /// Delivery status: "success", "failed", "pending".
    pub status: String,
    /// Response latency in milliseconds.
    pub latency_ms: i64,
    /// Error message if delivery failed.
    pub error: Option<String>,
    /// Number of attempts made.
    pub attempt: i32,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}

/// Webhook delivery log backed by the database.
#[derive(Clone)]
pub struct WebhookDeliveryLog {
    pool: sqlx::PgPool,
}

impl WebhookDeliveryLog {
    /// Creates a new delivery log.
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Records a webhook delivery attempt.
    pub async fn record(&self, record: &WebhookDeliveryRecord) -> Result<(), ShaperailError> {
        sqlx::query(
            r#"INSERT INTO shaperail_webhook_delivery_log
               (delivery_id, event_id, url, status_code, status, latency_ms, error, attempt, timestamp)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        )
        .bind(&record.delivery_id)
        .bind(&record.event_id)
        .bind(&record.url)
        .bind(record.status_code)
        .bind(&record.status)
        .bind(record.latency_ms)
        .bind(&record.error)
        .bind(record.attempt)
        .bind(&record.timestamp)
        .execute(&self.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Failed to log webhook delivery: {e}")))?;

        Ok(())
    }

    /// Retrieves delivery records for a specific event.
    pub async fn for_event(
        &self,
        event_id: &str,
    ) -> Result<Vec<WebhookDeliveryRecord>, ShaperailError> {
        let rows = sqlx::query(
            r#"SELECT delivery_id, event_id, url, status_code, status, latency_ms, error, attempt, timestamp
               FROM shaperail_webhook_delivery_log
               WHERE event_id = $1
               ORDER BY timestamp DESC"#,
        )
        .bind(event_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Failed to query delivery log: {e}")))?;

        rows.iter().map(row_to_delivery_record).collect()
    }

    /// Retrieves recent deliveries.
    pub async fn recent(&self, limit: i64) -> Result<Vec<WebhookDeliveryRecord>, ShaperailError> {
        let rows = sqlx::query(
            r#"SELECT delivery_id, event_id, url, status_code, status, latency_ms, error, attempt, timestamp
               FROM shaperail_webhook_delivery_log
               ORDER BY timestamp DESC
               LIMIT $1"#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Failed to query delivery log: {e}")))?;

        rows.iter().map(row_to_delivery_record).collect()
    }
}

fn row_to_delivery_record(
    row: &sqlx::postgres::PgRow,
) -> Result<WebhookDeliveryRecord, ShaperailError> {
    Ok(WebhookDeliveryRecord {
        delivery_id: row
            .try_get("delivery_id")
            .map_err(|e| ShaperailError::Internal(format!("Missing delivery_id column: {e}")))?,
        event_id: row
            .try_get("event_id")
            .map_err(|e| ShaperailError::Internal(format!("Missing event_id column: {e}")))?,
        url: row
            .try_get("url")
            .map_err(|e| ShaperailError::Internal(format!("Missing url column: {e}")))?,
        status_code: row
            .try_get("status_code")
            .map_err(|e| ShaperailError::Internal(format!("Missing status_code column: {e}")))?,
        status: row
            .try_get("status")
            .map_err(|e| ShaperailError::Internal(format!("Missing status column: {e}")))?,
        latency_ms: row
            .try_get("latency_ms")
            .map_err(|e| ShaperailError::Internal(format!("Missing latency_ms column: {e}")))?,
        error: row
            .try_get("error")
            .map_err(|e| ShaperailError::Internal(format!("Missing error column: {e}")))?,
        attempt: row
            .try_get("attempt")
            .map_err(|e| ShaperailError::Internal(format!("Missing attempt column: {e}")))?,
        timestamp: row
            .try_get("timestamp")
            .map_err(|e| ShaperailError::Internal(format!("Missing timestamp column: {e}")))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_record_serde() {
        let record = EventRecord {
            event_id: "evt-001".to_string(),
            event: "users.created".to_string(),
            resource: "users".to_string(),
            action: "created".to_string(),
            data: serde_json::json!({"id": "123"}),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let back: EventRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.event_id, "evt-001");
        assert_eq!(back.event, "users.created");
    }

    #[test]
    fn delivery_record_serde() {
        let record = WebhookDeliveryRecord {
            delivery_id: "del-001".to_string(),
            event_id: "evt-001".to_string(),
            url: "https://example.com/hook".to_string(),
            status_code: 200,
            status: "success".to_string(),
            latency_ms: 150,
            error: None,
            attempt: 1,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let back: WebhookDeliveryRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.delivery_id, "del-001");
        assert_eq!(back.status_code, 200);
        assert!(back.error.is_none());
    }

    #[test]
    fn delivery_record_with_error() {
        let record = WebhookDeliveryRecord {
            delivery_id: "del-002".to_string(),
            event_id: "evt-001".to_string(),
            url: "https://example.com/hook".to_string(),
            status_code: 500,
            status: "failed".to_string(),
            latency_ms: 3000,
            error: Some("Internal Server Error".to_string()),
            attempt: 3,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let back: WebhookDeliveryRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, "failed");
        assert_eq!(back.error.as_deref(), Some("Internal Server Error"));
    }
}
