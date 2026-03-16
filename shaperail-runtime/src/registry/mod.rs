use std::sync::Arc;
use std::time::Duration;

use deadpool_redis::Pool;
use redis::AsyncCommands;
use shaperail_core::{ServiceRegistryEntry, ServiceStatus, ShaperailError};

/// Redis key prefix for service registry entries.
const REGISTRY_PREFIX: &str = "shaperail:services:";

/// Default heartbeat interval in seconds.
const HEARTBEAT_INTERVAL_SECS: u64 = 10;

/// Default TTL for registry entries in seconds.
/// If a service misses 3 heartbeats, it's considered unhealthy.
const REGISTRY_TTL_SECS: u64 = 35;

/// Redis-backed service registry for multi-service workspace discovery.
///
/// Services register on startup and send periodic heartbeats.
/// Other services discover peers by querying the registry.
#[derive(Clone)]
pub struct ServiceRegistry {
    pool: Arc<Pool>,
}

impl ServiceRegistry {
    /// Create a new service registry backed by the given Redis pool.
    pub fn new(pool: Arc<Pool>) -> Self {
        Self { pool }
    }

    /// Register a service in the registry. Sets a TTL so stale entries expire.
    pub async fn register(&self, entry: &ServiceRegistryEntry) -> Result<(), ShaperailError> {
        let key = format!("{REGISTRY_PREFIX}{}", entry.name);
        let value = serde_json::to_string(entry).map_err(|e| {
            ShaperailError::Internal(format!("Failed to serialize registry entry: {e}"))
        })?;

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection error: {e}")))?;

        redis::cmd("SET")
            .arg(&key)
            .arg(&value)
            .arg("EX")
            .arg(REGISTRY_TTL_SECS)
            .query_async::<()>(&mut *conn)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to register service: {e}")))?;

        Ok(())
    }

    /// Update heartbeat for a service (refreshes TTL and updates timestamp + status).
    pub async fn heartbeat(&self, name: &str) -> Result<(), ShaperailError> {
        let key = format!("{REGISTRY_PREFIX}{name}");

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection error: {e}")))?;

        let value: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to read registry: {e}")))?;

        let Some(value) = value else {
            return Err(ShaperailError::NotFound);
        };

        let mut entry: ServiceRegistryEntry = serde_json::from_str(&value).map_err(|e| {
            ShaperailError::Internal(format!("Failed to parse registry entry: {e}"))
        })?;

        entry.status = ServiceStatus::Healthy;
        entry.last_heartbeat = chrono::Utc::now().to_rfc3339();

        let updated = serde_json::to_string(&entry).map_err(|e| {
            ShaperailError::Internal(format!("Failed to serialize registry entry: {e}"))
        })?;

        redis::cmd("SET")
            .arg(&key)
            .arg(&updated)
            .arg("EX")
            .arg(REGISTRY_TTL_SECS)
            .query_async::<()>(&mut *conn)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to update heartbeat: {e}")))?;

        Ok(())
    }

    /// Deregister a service (mark as stopped and remove).
    pub async fn deregister(&self, name: &str) -> Result<(), ShaperailError> {
        let key = format!("{REGISTRY_PREFIX}{name}");

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection error: {e}")))?;

        conn.del::<_, ()>(&key)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to deregister service: {e}")))?;

        Ok(())
    }

    /// Look up a service by name.
    pub async fn lookup(&self, name: &str) -> Result<Option<ServiceRegistryEntry>, ShaperailError> {
        let key = format!("{REGISTRY_PREFIX}{name}");

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection error: {e}")))?;

        let value: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to read registry: {e}")))?;

        match value {
            Some(v) => {
                let entry: ServiceRegistryEntry = serde_json::from_str(&v).map_err(|e| {
                    ShaperailError::Internal(format!("Failed to parse registry entry: {e}"))
                })?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    /// List all registered services.
    pub async fn list_services(&self) -> Result<Vec<ServiceRegistryEntry>, ShaperailError> {
        let pattern = format!("{REGISTRY_PREFIX}*");

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection error: {e}")))?;

        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut *conn)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to list services: {e}")))?;

        let mut services = Vec::new();
        for key in &keys {
            let value: Option<String> = conn
                .get(key)
                .await
                .map_err(|e| ShaperailError::Internal(format!("Failed to read registry: {e}")))?;
            if let Some(v) = value {
                if let Ok(entry) = serde_json::from_str::<ServiceRegistryEntry>(&v) {
                    services.push(entry);
                }
            }
        }

        Ok(services)
    }

    /// Discover a service that exposes a specific resource.
    pub async fn discover_resource(
        &self,
        resource_name: &str,
    ) -> Result<Option<ServiceRegistryEntry>, ShaperailError> {
        let services = self.list_services().await?;
        Ok(services.into_iter().find(|s| {
            s.status == ServiceStatus::Healthy && s.resources.iter().any(|r| r == resource_name)
        }))
    }

    /// Start a background heartbeat task for the given service name.
    /// Returns a `tokio::task::JoinHandle` that can be aborted on shutdown.
    pub fn start_heartbeat(&self, name: String) -> tokio::task::JoinHandle<()> {
        let registry = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
            loop {
                interval.tick().await;
                if let Err(e) = registry.heartbeat(&name).await {
                    tracing::warn!("Service registry heartbeat failed for '{name}': {e}");
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_key_format() {
        let key = format!("{REGISTRY_PREFIX}users-api");
        assert_eq!(key, "shaperail:services:users-api");
    }

    #[test]
    fn registry_constants() {
        assert_eq!(HEARTBEAT_INTERVAL_SECS, 10);
        assert_eq!(REGISTRY_TTL_SECS, 35);
    }
}
