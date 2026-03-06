use sqlx::PgPool;
use std::path::Path;
use steel_core::SteelError;

/// Runs all pending SQL migrations from the given directory.
///
/// Migrations are `.sql` files in the directory, applied in lexicographic order.
/// Uses sqlx's built-in migration support with a `_sqlx_migrations` tracking table.
pub async fn run_migrations(pool: &PgPool, migrations_dir: &Path) -> Result<(), SteelError> {
    let migrator = sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .map_err(|e| SteelError::Internal(format!("Failed to load migrations: {e}")))?;

    migrator
        .run(pool)
        .await
        .map_err(|e| SteelError::Internal(format!("Migration failed: {e}")))?;

    tracing::info!("Migrations applied successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn nonexistent_dir_returns_error() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        // We can't run migrations without a pool, but we can test the path check
        // by verifying the Migrator::new fails on a nonexistent directory
        let result = rt.block_on(async {
            sqlx::migrate::Migrator::new(Path::new("/nonexistent/migrations")).await
        });
        assert!(result.is_err());
    }

    #[test]
    fn migration_dir_path_construction() {
        let dir = PathBuf::from("./migrations");
        assert_eq!(dir.to_str().unwrap(), "./migrations");
    }
}
