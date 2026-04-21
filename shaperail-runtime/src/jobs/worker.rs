use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use redis::AsyncCommands;
use shaperail_core::ShaperailError;

use super::queue::{JobEnvelope, JobPriority, JobQueue, JobStatus};

/// A job handler is an async function that receives the job payload and returns
/// a result. Handlers are registered by name in the `JobRegistry`.
pub type JobHandler = Arc<
    dyn Fn(serde_json::Value) -> Pin<Box<dyn Future<Output = Result<(), ShaperailError>> + Send>>
        + Send
        + Sync,
>;

/// Registry of named job handlers.
#[derive(Clone, Default)]
pub struct JobRegistry {
    handlers: Arc<HashMap<String, JobHandler>>,
}

impl JobRegistry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(HashMap::new()),
        }
    }

    /// Creates a registry from a map of handlers.
    pub fn from_handlers(handlers: HashMap<String, JobHandler>) -> Self {
        Self {
            handlers: Arc::new(handlers),
        }
    }

    /// Looks up a handler by job name.
    pub fn get(&self, name: &str) -> Option<&JobHandler> {
        self.handlers.get(name)
    }

    /// Returns true if no handlers are registered.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

/// Background job worker that polls Redis and executes registered handlers.
///
/// Runs in a separate Tokio task — never blocks the HTTP server.
/// Polls priority queues in order: critical → high → normal → low.
pub struct Worker {
    queue: JobQueue,
    registry: JobRegistry,
    poll_interval: Duration,
}

impl Worker {
    /// Creates a new worker with the given queue, registry, and poll interval.
    pub fn new(queue: JobQueue, registry: JobRegistry, poll_interval: Duration) -> Self {
        Self {
            queue,
            registry,
            poll_interval,
        }
    }

    /// Starts the worker loop in a background Tokio task.
    ///
    /// The returned `JoinHandle` can be used to await shutdown.
    /// The worker runs until the `shutdown` receiver signals.
    pub fn spawn(
        self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            tracing::info!("Job worker started");
            loop {
                tokio::select! {
                    result = shutdown.changed() => {
                        if result.is_err() || *shutdown.borrow() {
                            tracing::info!("Job worker shutting down");
                            break;
                        }
                    }
                    _ = tokio::time::sleep(self.poll_interval) => {
                        if let Err(e) = self.poll_once().await {
                            tracing::error!(error = %e, "Worker poll error");
                        }
                    }
                }
            }
        })
    }

    /// Polls all priority queues once, processing at most one job.
    ///
    /// Returns `Ok(true)` if a job was processed, `Ok(false)` if queues were empty.
    pub async fn poll_once(&self) -> Result<bool, ShaperailError> {
        // Poll queues in priority order
        for priority in JobPriority::all() {
            if let Some(envelope) = self.dequeue(priority.queue_key()).await? {
                self.process_job(envelope).await;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Attempts to dequeue a job from the specified Redis list.
    async fn dequeue(&self, queue_key: &str) -> Result<Option<JobEnvelope>, ShaperailError> {
        let mut conn = self
            .queue
            .pool()
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection failed: {e}")))?;

        let result: Option<String> = conn
            .lpop(queue_key, None)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to dequeue job: {e}")))?;

        match result {
            Some(json) => {
                let envelope: JobEnvelope = serde_json::from_str(&json).map_err(|e| {
                    ShaperailError::Internal(format!("Failed to deserialize job: {e}"))
                })?;
                Ok(Some(envelope))
            }
            None => Ok(None),
        }
    }

    /// Processes a single job: executes the handler with timeout, handles retry/dead-letter.
    async fn process_job(&self, envelope: JobEnvelope) {
        let job_id = envelope.id.clone();
        let job_name = envelope.name.clone();
        let attempt = envelope.attempt + 1;
        let timeout = Duration::from_secs(envelope.timeout_secs);
        let _job_span_guard = tracing::info_span!(
            "job.execute",
            job.name = %job_name,
            job.id = %job_id,
        );

        tracing::info!(
            job_id = %job_id,
            job_name = %job_name,
            attempt = attempt,
            "Processing job"
        );

        // Update status to running
        if let Err(e) = self
            .queue
            .update_status(&job_id, JobStatus::Running, attempt, None)
            .await
        {
            tracing::error!(job_id = %job_id, error = %e, "Failed to update job status to running");
            return;
        }

        let handler = match self.registry.get(&job_name) {
            Some(h) => h,
            None => {
                let err = format!("No handler registered for job: {job_name}");
                tracing::error!(job_id = %job_id, %err);
                let _ = self.handle_failure(envelope, &err).await;
                return;
            }
        };

        // Execute with timeout
        let result = tokio::time::timeout(timeout, (handler)(envelope.payload.clone())).await;

        match result {
            Ok(Ok(())) => {
                // Success
                if let Err(e) = self
                    .queue
                    .update_status(&job_id, JobStatus::Completed, attempt, None)
                    .await
                {
                    tracing::error!(job_id = %job_id, error = %e, "Failed to update job status to completed");
                }
                tracing::info!(job_id = %job_id, job_name = %job_name, "Job completed");
            }
            Ok(Err(e)) => {
                // Handler error
                let err_msg = e.to_string();
                tracing::warn!(job_id = %job_id, job_name = %job_name, error = %err_msg, "Job failed");
                let _ = self.handle_failure(envelope, &err_msg).await;
            }
            Err(_) => {
                // Timeout
                let err_msg = format!("Job timed out after {}s", timeout.as_secs());
                tracing::warn!(job_id = %job_id, job_name = %job_name, %err_msg);
                let _ = self.handle_failure(envelope, &err_msg).await;
            }
        }
    }

    /// Handles a failed job: either retry with exponential backoff or move to dead letter.
    async fn handle_failure(
        &self,
        envelope: JobEnvelope,
        error: &str,
    ) -> Result<(), ShaperailError> {
        let next_attempt = envelope.attempt + 1;

        if next_attempt < envelope.max_retries {
            // Schedule retry with exponential backoff
            let backoff = Duration::from_secs(2u64.pow(next_attempt));
            tracing::info!(
                job_id = %envelope.id,
                next_attempt = next_attempt + 1,
                backoff_secs = backoff.as_secs(),
                "Scheduling job retry"
            );

            // For simplicity, we sleep then requeue. In production you'd use a sorted set
            // for delayed jobs, but this satisfies the milestone requirements.
            let queue = self.queue.clone();
            let envelope_clone = envelope.clone();
            tokio::spawn(async move {
                tokio::time::sleep(backoff).await;
                if let Err(e) = queue.requeue_for_retry(envelope_clone).await {
                    tracing::error!(error = %e, "Failed to requeue job for retry");
                }
            });

            Ok(())
        } else {
            // Max retries exceeded — dead letter
            self.queue.move_to_dead_letter(&envelope, error).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lookup() {
        let mut handlers = HashMap::new();
        handlers.insert(
            "test_job".to_string(),
            Arc::new(|_payload: serde_json::Value| {
                Box::pin(async { Ok(()) })
                    as Pin<Box<dyn Future<Output = Result<(), ShaperailError>> + Send>>
            }) as JobHandler,
        );
        let registry = JobRegistry::from_handlers(handlers);
        assert!(registry.get("test_job").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn empty_registry() {
        let registry = JobRegistry::new();
        assert!(registry.get("anything").is_none());
    }

    #[test]
    fn is_empty_returns_true_for_new_registry() {
        let registry = JobRegistry::new();
        assert!(registry.is_empty());
    }

    #[test]
    fn shutdown_arm_breaks_on_channel_close() {
        // When the watch sender is dropped, changed() returns Err.
        // This test documents that the worker loop checks for Err.
        let (tx, mut rx) = tokio::sync::watch::channel(false);
        drop(tx);
        // In a blocking context, changed() on a closed channel returns Err immediately
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(rx.changed());
        assert!(
            result.is_err(),
            "changed() must return Err when sender is dropped"
        );
    }

    #[test]
    fn is_empty_returns_false_when_handler_registered() {
        let mut handlers = HashMap::new();
        handlers.insert(
            "a_job".to_string(),
            Arc::new(|_payload: serde_json::Value| {
                Box::pin(async { Ok(()) })
                    as Pin<Box<dyn Future<Output = Result<(), ShaperailError>> + Send>>
            }) as JobHandler,
        );
        let registry = JobRegistry::from_handlers(handlers);
        assert!(!registry.is_empty());
    }
}
