//! Migration runner (M14). Supports single-DB (legacy) and multi-DB modes.
//!
//! In multi-DB mode, migrations are run on all SQL connections in the DatabaseManager
//! in dependency order ("default" first, then others alphabetically).

use shaperail_core::{DatabaseEngine, ShaperailError};
use sqlx::PgPool;
use std::path::Path;

use super::manager::{DatabaseManager, SqlConnection};

/// Runs all pending SQL migrations from the given directory (legacy single-DB mode).
///
/// Migrations are `.sql` files in the directory, applied in lexicographic order.
/// Uses sqlx's built-in migration support with a `_sqlx_migrations` tracking table.
pub async fn run_migrations(pool: &PgPool, migrations_dir: &Path) -> Result<(), ShaperailError> {
    let migrator = sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Failed to load migrations: {e}")))?;

    migrator
        .run(pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Migration failed: {e}")))?;

    tracing::info!("Migrations applied successfully");
    Ok(())
}

/// Run migrations on all SQL engines in the DatabaseManager (M14 multi-DB mode).
///
/// Runs "default" connection first (dependency root), then other connections alphabetically.
/// Each engine uses SeaORM to execute raw migration SQL. Migrations are loaded from
/// the given directory and executed in lexicographic order.
pub async fn run_migrations_multi(
    manager: &DatabaseManager,
    migrations_dir: &Path,
) -> Result<(), ShaperailError> {
    let migrator = sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Failed to load migrations: {e}")))?;

    // Collect connections in dependency order: "default" first, then alphabetical.
    let mut ordered: Vec<(&str, &SqlConnection)> = Vec::new();
    let all: Vec<(&str, &SqlConnection)> = manager.all_connections().collect();

    // Default first.
    if let Some(default) = all.iter().find(|(name, _)| *name == "default") {
        ordered.push(*default);
    }
    // Then others alphabetically.
    let mut others: Vec<(&str, &SqlConnection)> = all
        .iter()
        .filter(|(name, _)| *name != "default")
        .copied()
        .collect();
    others.sort_by_key(|(name, _)| *name);
    ordered.extend(others);

    for (name, conn) in ordered {
        tracing::info!("Running migrations on '{}' ({:?})", name, conn.engine);
        run_migrations_on_connection(name, conn, &migrator).await?;
    }

    tracing::info!("All database migrations applied successfully");
    Ok(())
}

/// Run migrations on a single SeaORM connection.
///
/// Uses the connection's engine to adapt migration SQL as needed.
async fn run_migrations_on_connection(
    name: &str,
    conn: &SqlConnection,
    migrator: &sqlx::migrate::Migrator,
) -> Result<(), ShaperailError> {
    use sea_orm::ConnectionTrait;

    let backend = conn.backend();

    // Ensure migration tracking table exists.
    let create_tracking = match conn.engine {
        DatabaseEngine::MySQL => "CREATE TABLE IF NOT EXISTS `_sqlx_migrations` (
                `version` BIGINT PRIMARY KEY,
                `description` TEXT NOT NULL,
                `installed_on` DATETIME(6) NOT NULL DEFAULT (CURRENT_TIMESTAMP(6)),
                `success` BOOLEAN NOT NULL,
                `checksum` BLOB NOT NULL,
                `execution_time` BIGINT NOT NULL
            )"
        .to_string(),
        DatabaseEngine::SQLite => "CREATE TABLE IF NOT EXISTS \"_sqlx_migrations\" (
                \"version\" INTEGER PRIMARY KEY,
                \"description\" TEXT NOT NULL,
                \"installed_on\" TEXT NOT NULL DEFAULT (datetime('now')),
                \"success\" INTEGER NOT NULL,
                \"checksum\" BLOB NOT NULL,
                \"execution_time\" INTEGER NOT NULL
            )"
        .to_string(),
        _ => "CREATE TABLE IF NOT EXISTS \"_sqlx_migrations\" (
                \"version\" BIGINT PRIMARY KEY,
                \"description\" TEXT NOT NULL,
                \"installed_on\" TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                \"success\" BOOLEAN NOT NULL,
                \"checksum\" BYTEA NOT NULL,
                \"execution_time\" BIGINT NOT NULL
            )"
        .to_string(),
    };

    conn.inner
        .execute(sea_orm::Statement::from_string(backend, create_tracking))
        .await
        .map_err(|e| {
            ShaperailError::Internal(format!(
                "Failed to create migration tracking table on '{name}': {e}"
            ))
        })?;

    // Get already-applied versions.
    let applied_rows = conn
        .inner
        .query_all(sea_orm::Statement::from_string(
            backend,
            "SELECT version FROM _sqlx_migrations WHERE success = true".replace(
                "true",
                if conn.engine == DatabaseEngine::SQLite {
                    "1"
                } else {
                    "true"
                },
            ),
        ))
        .await
        .map_err(|e| {
            ShaperailError::Internal(format!(
                "Failed to query migration versions on '{name}': {e}"
            ))
        })?;

    let applied: std::collections::HashSet<i64> = applied_rows
        .iter()
        .filter_map(|row| {
            use sea_orm::TryGetable;
            i64::try_get(row, "", "version").ok()
        })
        .collect();

    // Apply each pending migration.
    for migration in migrator.migrations.iter() {
        let version = migration.version;
        if applied.contains(&version) {
            continue;
        }

        let sql = match migration.migration_type {
            sqlx::migrate::MigrationType::Simple => migration.sql.to_string(),
            _ => {
                tracing::warn!(
                    "Skipping non-simple migration {} on '{name}'",
                    migration.description
                );
                continue;
            }
        };

        let start = std::time::Instant::now();
        // Execute the migration SQL.
        let result = conn
            .inner
            .execute(sea_orm::Statement::from_string(backend, sql))
            .await;
        let elapsed = start.elapsed().as_nanos() as i64;

        let success = result.is_ok();
        if let Err(ref e) = result {
            tracing::error!(
                "Migration {} failed on '{name}': {e}",
                migration.description
            );
        }

        // Record in tracking table.
        let checksum_hex = hex::encode(&migration.checksum);
        let record_sql = match conn.engine {
            DatabaseEngine::MySQL => format!(
                "INSERT INTO `_sqlx_migrations` (`version`, `description`, `success`, `checksum`, `execution_time`) \
                 VALUES ({version}, '{}', {success}, X'{}', {elapsed})",
                migration.description.replace('\'', "''"),
                checksum_hex,
            ),
            _ => format!(
                "INSERT INTO \"_sqlx_migrations\" (\"version\", \"description\", \"success\", \"checksum\", \"execution_time\") \
                 VALUES ({version}, '{}', {success}, '\\x{}', {elapsed})",
                migration.description.replace('\'', "''"),
                checksum_hex,
            ),
        };
        let _ = conn
            .inner
            .execute(sea_orm::Statement::from_string(backend, record_sql))
            .await;

        if !success {
            return Err(ShaperailError::Internal(format!(
                "Migration '{}' failed on '{name}': {}",
                migration.description,
                result.err().map(|e| e.to_string()).unwrap_or_default()
            )));
        }

        tracing::info!(
            "Applied migration '{}' on '{name}' ({:.1}ms)",
            migration.description,
            elapsed as f64 / 1_000_000.0
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn nonexistent_dir_returns_error() {
        let rt = tokio::runtime::Runtime::new().unwrap();
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
