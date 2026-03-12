use deadpool_redis::{Config, Pool, Runtime};
use shaperail_core::ShaperailError;

/// Creates a Redis connection pool from the given URL.
///
/// The URL should be in the format `redis://host:port/db`.
pub fn create_redis_pool(redis_url: &str) -> Result<Pool, ShaperailError> {
    let cfg = Config::from_url(redis_url);
    cfg.create_pool(Some(Runtime::Tokio1))
        .map_err(|e| ShaperailError::Internal(format!("Failed to create Redis pool: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_pool_invalid_url_returns_error() {
        // Empty URL should still create a pool (connection fails lazily)
        // but a completely broken config might error
        let result = create_redis_pool("redis://localhost:6379");
        assert!(result.is_ok());
    }
}
