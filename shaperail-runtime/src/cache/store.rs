use std::collections::HashMap;
use std::sync::Arc;

use redis::AsyncCommands;
use sha2::{Digest, Sha256};

/// Redis-backed response cache for GET endpoints.
///
/// Cache keys follow the pattern: `shaperail:<resource>:<endpoint>:<query_hash>:<user_role>`
/// Auto-invalidation deletes all keys for a resource when a write occurs.
#[derive(Clone)]
pub struct RedisCache {
    pool: Arc<deadpool_redis::Pool>,
}

impl RedisCache {
    /// Creates a new cache backed by the given Redis pool.
    pub fn new(pool: Arc<deadpool_redis::Pool>) -> Self {
        Self { pool }
    }

    /// Builds a cache key from components.
    ///
    /// Format: `shaperail:<resource>:<endpoint>:<query_hash>:<role>`
    pub fn build_key(
        resource: &str,
        endpoint: &str,
        query_params: &HashMap<String, String>,
        user_role: &str,
    ) -> String {
        let query_hash = Self::hash_query(query_params);
        format!("shaperail:{resource}:{endpoint}:{query_hash}:{user_role}")
    }

    /// Hashes query parameters into a deterministic short string.
    fn hash_query(params: &HashMap<String, String>) -> String {
        let mut sorted: Vec<(&String, &String)> = params.iter().collect();
        sorted.sort_by_key(|(k, _)| k.as_str());

        let mut hasher = Sha256::new();
        for (k, v) in sorted {
            hasher.update(k.as_bytes());
            hasher.update(b"=");
            hasher.update(v.as_bytes());
            hasher.update(b"&");
        }
        let result = hasher.finalize();
        // 16 hex chars from first 8 bytes — enough for cache keys
        result[..8]
            .iter()
            .fold(String::with_capacity(16), |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{b:02x}");
                s
            })
    }

    /// Attempts to retrieve a cached response.
    ///
    /// Returns `None` on cache miss or Redis errors (fail-open).
    pub async fn get(&self, key: &str) -> Option<String> {
        let _span = crate::observability::telemetry::cache_span("get", key).entered();
        let mut conn = self.pool.get().await.ok()?;
        let result: Option<String> = conn.get(key).await.ok()?;
        result
    }

    /// Stores a response in the cache with the given TTL.
    ///
    /// Silently ignores Redis errors (fail-open).
    pub async fn set(&self, key: &str, value: &str, ttl_secs: u64) {
        let _span = crate::observability::telemetry::cache_span("set", key).entered();
        let Ok(mut conn) = self.pool.get().await else {
            return;
        };
        let _: Result<(), _> = conn.set_ex(key, value, ttl_secs).await;
    }

    /// Invalidates all cache keys for a given resource.
    ///
    /// Uses SCAN to find matching keys and DEL to remove them.
    /// Silently ignores Redis errors (fail-open).
    pub async fn invalidate_resource(&self, resource: &str) {
        let Ok(mut conn) = self.pool.get().await else {
            return;
        };
        let pattern = format!("shaperail:{resource}:*");
        let keys: Vec<String> = match redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut *conn)
            .await
        {
            Ok(keys) => keys,
            Err(_) => return,
        };
        if keys.is_empty() {
            return;
        }
        let _: Result<(), _> = conn.del(keys).await;
    }

    /// Invalidates cache for a resource only if the action matches
    /// the endpoint's `invalidate_on` list.
    ///
    /// If `invalidate_on` is `None`, all writes invalidate.
    pub async fn invalidate_if_needed(
        &self,
        resource: &str,
        action: &str,
        invalidate_on: Option<&[String]>,
    ) {
        let should_invalidate = match invalidate_on {
            Some(actions) => actions.iter().any(|a| a == action),
            None => true, // No explicit list = invalidate on all writes
        };
        if should_invalidate {
            self.invalidate_resource(resource).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_key_format() {
        let mut params = HashMap::new();
        params.insert("filter[role]".to_string(), "admin".to_string());
        let key = RedisCache::build_key("users", "list", &params, "member");
        assert!(key.starts_with("shaperail:users:list:"));
        assert!(key.ends_with(":member"));
    }

    #[test]
    fn build_key_empty_params() {
        let params = HashMap::new();
        let key = RedisCache::build_key("users", "list", &params, "admin");
        assert!(key.starts_with("shaperail:users:list:"));
        assert!(key.ends_with(":admin"));
    }

    #[test]
    fn hash_query_deterministic() {
        let mut params1 = HashMap::new();
        params1.insert("a".to_string(), "1".to_string());
        params1.insert("b".to_string(), "2".to_string());

        let mut params2 = HashMap::new();
        params2.insert("b".to_string(), "2".to_string());
        params2.insert("a".to_string(), "1".to_string());

        // Same params in different insertion order should produce same hash
        assert_eq!(
            RedisCache::hash_query(&params1),
            RedisCache::hash_query(&params2)
        );
    }

    #[test]
    fn hash_query_different_params() {
        let mut params1 = HashMap::new();
        params1.insert("a".to_string(), "1".to_string());

        let mut params2 = HashMap::new();
        params2.insert("a".to_string(), "2".to_string());

        assert_ne!(
            RedisCache::hash_query(&params1),
            RedisCache::hash_query(&params2)
        );
    }
}
