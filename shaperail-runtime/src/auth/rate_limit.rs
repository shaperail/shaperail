use std::sync::Arc;

use shaperail_core::ShaperailError;

/// Configuration for the rate limiter.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window.
    pub max_requests: u64,
    /// Window size in seconds.
    pub window_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_secs: 60,
        }
    }
}

/// Redis-backed sliding window rate limiter.
///
/// Uses a sorted set per key with timestamps as scores.
/// Survives server restarts since all state is in Redis.
#[derive(Clone)]
pub struct RateLimiter {
    pool: Arc<deadpool_redis::Pool>,
    config: RateLimitConfig,
}

impl RateLimiter {
    /// Creates a new rate limiter backed by the given Redis pool.
    pub fn new(pool: Arc<deadpool_redis::Pool>, config: RateLimitConfig) -> Self {
        Self { pool, config }
    }

    /// Checks if the given key (IP or token) is within rate limits.
    ///
    /// Returns `Ok(remaining)` with the number of remaining requests,
    /// or `Err(ShaperailError::RateLimited)` if the limit is exceeded.
    pub async fn check(&self, key: &str) -> Result<u64, ShaperailError> {
        let redis_key = format!("shaperail:ratelimit:{key}");
        let now = chrono::Utc::now().timestamp_millis() as f64;
        let window_start = now - (self.config.window_secs as f64 * 1000.0);

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis connection failed: {e}")))?;

        // Lua script for atomic sliding window:
        // 1. Remove entries older than the window
        // 2. Add current timestamp
        // 3. Count entries in window
        // 4. Set TTL on the key
        let script = redis::Script::new(
            r#"
            redis.call('ZREMRANGEBYSCORE', KEYS[1], '-inf', ARGV[1])
            local seq = redis.call('INCR', KEYS[1] .. ':seq')
            redis.call('ZADD', KEYS[1], ARGV[2], ARGV[2] .. ':' .. seq)
            local count = redis.call('ZCARD', KEYS[1])
            redis.call('EXPIRE', KEYS[1], ARGV[3])
            return count
            "#,
        );

        let count: u64 = script
            .key(&redis_key)
            .arg(window_start)
            .arg(now)
            .arg(self.config.window_secs as i64 + 1)
            .invoke_async(&mut *conn)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Redis rate limit error: {e}")))?;

        if count > self.config.max_requests {
            return Err(ShaperailError::RateLimited);
        }

        Ok(self.config.max_requests - count)
    }

    /// Returns a clone of the underlying Redis pool.
    /// Used to create per-endpoint limiter instances with different configs.
    pub fn pool(&self) -> Arc<deadpool_redis::Pool> {
        self.pool.clone()
    }

    /// Builds the rate limit key from IP and optional token.
    ///
    /// If a token (user ID) is provided, rate limits per user.
    /// Otherwise, rate limits per IP address.
    pub fn key_for(ip: &str, user_id: Option<&str>) -> String {
        match user_id {
            Some(uid) => format!("user:{uid}"),
            None => format!("ip:{ip}"),
        }
    }

    /// Builds a tenant-scoped rate limit key (M18).
    ///
    /// Scopes the key by tenant_id so each tenant has independent rate limits.
    pub fn key_for_tenant(ip: &str, user_id: Option<&str>, tenant_id: Option<&str>) -> String {
        let base = Self::key_for(ip, user_id);
        match tenant_id {
            Some(tid) => format!("t:{tid}:{base}"),
            None => base,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let cfg = RateLimitConfig::default();
        assert_eq!(cfg.max_requests, 100);
        assert_eq!(cfg.window_secs, 60);
    }

    #[test]
    fn key_for_ip() {
        let key = RateLimiter::key_for("192.168.1.1", None);
        assert_eq!(key, "ip:192.168.1.1");
    }

    #[test]
    fn key_for_user() {
        let key = RateLimiter::key_for("192.168.1.1", Some("user-123"));
        assert_eq!(key, "user:user-123");
    }

    #[test]
    fn key_for_tenant_scoped() {
        let key = RateLimiter::key_for_tenant("192.168.1.1", Some("user-123"), Some("org-a"));
        assert_eq!(key, "t:org-a:user:user-123");
    }

    #[test]
    fn key_for_tenant_no_tenant() {
        let key = RateLimiter::key_for_tenant("192.168.1.1", Some("user-123"), None);
        assert_eq!(key, "user:user-123");
    }

    #[test]
    fn tenant_keys_differ() {
        let key_a = RateLimiter::key_for_tenant("192.168.1.1", Some("user-123"), Some("org-a"));
        let key_b = RateLimiter::key_for_tenant("192.168.1.1", Some("user-123"), Some("org-b"));
        assert_ne!(
            key_a, key_b,
            "Rate limit keys for different tenants must differ"
        );
    }
}
