use shaperail_core::ShaperailError;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Creates a PostgreSQL connection pool from the given database URL.
pub async fn create_pool(
    database_url: &str,
    max_connections: u32,
) -> Result<PgPool, ShaperailError> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Failed to connect to database: {e}")))
}

/// Runs a simple `SELECT 1` health check against the pool.
pub async fn health_check(pool: &PgPool) -> Result<(), ShaperailError> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Database health check failed: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_rejects_invalid_url() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(create_pool("postgresql://bad:bad@localhost:59999/nope", 1));
        assert!(result.is_err());
        if let Err(ShaperailError::Internal(msg)) = result {
            assert!(msg.contains("Failed to connect"));
        }
    }
}
