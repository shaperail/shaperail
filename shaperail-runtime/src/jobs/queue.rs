use std::sync::Arc;

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use shaperail_core::ShaperailError;
use uuid::Uuid;

/// Job priority levels, each backed by a separate Redis list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobPriority {
    Critical,
    High,
    Normal,
    Low,
}

impl JobPriority {
    /// Returns all priorities in polling order (highest first).
    pub fn all() -> &'static [JobPriority] {
        &[
            JobPriority::Critical,
            JobPriority::High,
            JobPriority::Normal,
            JobPriority::Low,
        ]
    }

    /// Redis list key for this priority level.
    pub fn queue_key(&self) -> &'static str {
        match self {
            JobPriority::Critical => "shaperail:jobs:queue:critical",
            JobPriority::High => "shaperail:jobs:queue:high",
            JobPriority::Normal => "shaperail:jobs:queue:normal",
            JobPriority::Low => "shaperail:jobs:queue:low",
        }
    }
}

/// Current status of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Pending => write!(f, "pending"),
            JobStatus::Running => write!(f, "running"),
            JobStatus::Completed => write!(f, "completed"),
            JobStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Serialized job envelope stored in Redis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobEnvelope {
    pub id: String,
    pub name: String,
    pub payload: serde_json::Value,
    pub priority: JobPriority,
    pub max_retries: u32,
    pub timeout_secs: u64,
    pub attempt: u32,
}

/// Metadata about a job stored in a Redis hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub id: String,
    pub name: String,
    pub status: JobStatus,
    pub attempt: u32,
    pub max_retries: u32,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Redis-backed job queue.
///
/// Enqueues jobs into priority-separated Redis lists.
/// Job metadata is stored in Redis hashes for status queries.
#[derive(Clone)]
pub struct JobQueue {
    pool: Arc<deadpool_redis::Pool>,
}

impl JobQueue {
    /// Creates a new job queue backed by the given Redis pool.
    pub fn new(pool: Arc<deadpool_redis::Pool>) -> Self {
        Self { pool }
    }

    /// Enqueues a job with the given name, payload, and priority.
    ///
    /// Returns the generated job ID.
    pub async fn enqueue(
        &self,
        name: &str,
        payload: serde_json::Value,
        priority: JobPriority,
    ) -> Result<String, ShaperailError> {
        self.enqueue_with_options(name, payload, priority, 3, 300)
            .await
    }

    /// Enqueues a job with full configuration.
    ///
    /// `max_retries` — how many times to retry on failure (default 3).
    /// `timeout_secs` — auto-fail after this duration (default 300s).
    pub async fn enqueue_with_options(
        &self,
        name: &str,
        payload: serde_json::Value,
        priority: JobPriority,
        max_retries: u32,
        timeout_secs: u64,
    ) -> Result<String, ShaperailError> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let envelope = JobEnvelope {
            id: id.clone(),
            name: name.to_string(),
            payload,
            priority,
            max_retries,
            timeout_secs,
            attempt: 0,
        };

        let envelope_json = serde_json::to_string(&envelope)
            .map_err(|e| ShaperailError::Internal(format!("Failed to serialize job: {e}")))?;

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection failed: {e}")))?;

        // Store job metadata
        let meta_key = format!("shaperail:jobs:meta:{id}");
        redis::cmd("HSET")
            .arg(&meta_key)
            .arg("id")
            .arg(&id)
            .arg("name")
            .arg(name)
            .arg("status")
            .arg(JobStatus::Pending.to_string())
            .arg("attempt")
            .arg("0")
            .arg("max_retries")
            .arg(max_retries.to_string())
            .arg("created_at")
            .arg(&now)
            .arg("updated_at")
            .arg(&now)
            .query_async::<()>(&mut *conn)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to store job metadata: {e}")))?;

        // Set TTL on metadata (7 days)
        let _: Result<(), _> = conn.expire(&meta_key, 604800).await;

        // Push to priority queue
        conn.rpush::<_, _, ()>(priority.queue_key(), &envelope_json)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to enqueue job: {e}")))?;

        tracing::info!(job_id = %id, job_name = name, priority = ?priority, "Job enqueued");
        Ok(id)
    }

    /// Retrieves the status/info of a job by ID.
    pub async fn get_status(&self, job_id: &str) -> Result<JobInfo, ShaperailError> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection failed: {e}")))?;

        let meta_key = format!("shaperail:jobs:meta:{job_id}");
        let values: Vec<String> = redis::cmd("HGETALL")
            .arg(&meta_key)
            .query_async(&mut *conn)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to get job status: {e}")))?;

        if values.is_empty() {
            return Err(ShaperailError::NotFound);
        }

        // Parse HGETALL flat key-value pairs
        let mut map = std::collections::HashMap::new();
        for chunk in values.chunks(2) {
            if chunk.len() == 2 {
                map.insert(chunk[0].clone(), chunk[1].clone());
            }
        }

        let status = match map.get("status").map(|s| s.as_str()) {
            Some("pending") => JobStatus::Pending,
            Some("running") => JobStatus::Running,
            Some("completed") => JobStatus::Completed,
            Some("failed") => JobStatus::Failed,
            _ => JobStatus::Pending,
        };

        Ok(JobInfo {
            id: map.get("id").cloned().unwrap_or_default(),
            name: map.get("name").cloned().unwrap_or_default(),
            status,
            attempt: map.get("attempt").and_then(|s| s.parse().ok()).unwrap_or(0),
            max_retries: map
                .get("max_retries")
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            error: map.get("error").cloned(),
            created_at: map.get("created_at").cloned().unwrap_or_default(),
            updated_at: map.get("updated_at").cloned().unwrap_or_default(),
        })
    }

    /// Updates a job's status in the metadata hash.
    pub(crate) async fn update_status(
        &self,
        job_id: &str,
        status: JobStatus,
        attempt: u32,
        error: Option<&str>,
    ) -> Result<(), ShaperailError> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection failed: {e}")))?;

        let meta_key = format!("shaperail:jobs:meta:{job_id}");
        let now = chrono::Utc::now().to_rfc3339();

        let mut cmd = redis::cmd("HSET");
        cmd.arg(&meta_key)
            .arg("status")
            .arg(status.to_string())
            .arg("attempt")
            .arg(attempt.to_string())
            .arg("updated_at")
            .arg(&now);

        if let Some(err_msg) = error {
            cmd.arg("error").arg(err_msg);
        }

        cmd.query_async::<()>(&mut *conn)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to update job status: {e}")))?;

        Ok(())
    }

    /// Moves a job to the dead letter queue.
    pub(crate) async fn move_to_dead_letter(
        &self,
        envelope: &JobEnvelope,
        error: &str,
    ) -> Result<(), ShaperailError> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection failed: {e}")))?;

        let dead_entry = serde_json::json!({
            "id": envelope.id,
            "name": envelope.name,
            "payload": envelope.payload,
            "error": error,
            "attempts": envelope.attempt,
            "failed_at": chrono::Utc::now().to_rfc3339(),
        });

        let dead_json = serde_json::to_string(&dead_entry).map_err(|e| {
            ShaperailError::Internal(format!("Failed to serialize dead letter: {e}"))
        })?;

        conn.rpush::<_, _, ()>("shaperail:jobs:dead", &dead_json)
            .await
            .map_err(|e| {
                ShaperailError::Internal(format!("Failed to push to dead letter queue: {e}"))
            })?;

        // Update status
        self.update_status(
            &envelope.id,
            JobStatus::Failed,
            envelope.attempt,
            Some(error),
        )
        .await?;

        tracing::warn!(
            job_id = %envelope.id,
            job_name = %envelope.name,
            attempts = envelope.attempt,
            "Job moved to dead letter queue"
        );

        Ok(())
    }

    /// Re-enqueues a job for retry with incremented attempt count.
    pub(crate) async fn requeue_for_retry(
        &self,
        mut envelope: JobEnvelope,
    ) -> Result<(), ShaperailError> {
        envelope.attempt += 1;

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection failed: {e}")))?;

        let envelope_json = serde_json::to_string(&envelope)
            .map_err(|e| ShaperailError::Internal(format!("Failed to serialize job: {e}")))?;

        // Push back to the same priority queue
        conn.rpush::<_, _, ()>(envelope.priority.queue_key(), &envelope_json)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to requeue job: {e}")))?;

        self.update_status(&envelope.id, JobStatus::Pending, envelope.attempt, None)
            .await?;

        Ok(())
    }

    /// Returns a reference to the underlying pool (used by Worker).
    pub(crate) fn pool(&self) -> &Arc<deadpool_redis::Pool> {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_queue_keys() {
        assert_eq!(
            JobPriority::Critical.queue_key(),
            "shaperail:jobs:queue:critical"
        );
        assert_eq!(JobPriority::High.queue_key(), "shaperail:jobs:queue:high");
        assert_eq!(
            JobPriority::Normal.queue_key(),
            "shaperail:jobs:queue:normal"
        );
        assert_eq!(JobPriority::Low.queue_key(), "shaperail:jobs:queue:low");
    }

    #[test]
    fn priority_all_order() {
        let all = JobPriority::all();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0], JobPriority::Critical);
        assert_eq!(all[1], JobPriority::High);
        assert_eq!(all[2], JobPriority::Normal);
        assert_eq!(all[3], JobPriority::Low);
    }

    #[test]
    fn job_status_display() {
        assert_eq!(JobStatus::Pending.to_string(), "pending");
        assert_eq!(JobStatus::Running.to_string(), "running");
        assert_eq!(JobStatus::Completed.to_string(), "completed");
        assert_eq!(JobStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn job_envelope_serde_roundtrip() {
        let envelope = JobEnvelope {
            id: "test-id".to_string(),
            name: "send_email".to_string(),
            payload: serde_json::json!({"user_id": "123"}),
            priority: JobPriority::Normal,
            max_retries: 3,
            timeout_secs: 300,
            attempt: 0,
        };
        let json = serde_json::to_string(&envelope).unwrap();
        let back: JobEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "test-id");
        assert_eq!(back.name, "send_email");
        assert_eq!(back.priority, JobPriority::Normal);
        assert_eq!(back.max_retries, 3);
        assert_eq!(back.timeout_secs, 300);
        assert_eq!(back.attempt, 0);
    }

    #[test]
    fn job_priority_serde() {
        let json = serde_json::to_string(&JobPriority::Critical).unwrap();
        assert_eq!(json, "\"critical\"");
        let back: JobPriority = serde_json::from_str(&json).unwrap();
        assert_eq!(back, JobPriority::Critical);
    }

    #[test]
    fn job_status_serde() {
        let json = serde_json::to_string(&JobStatus::Completed).unwrap();
        assert_eq!(json, "\"completed\"");
        let back: JobStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, JobStatus::Completed);
    }
}
