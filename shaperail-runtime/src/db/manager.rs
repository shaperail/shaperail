//! Multi-database connection manager (M14).
//!
//! Holds named SQL connections via SeaORM. Resources use `db: <name>` to select
//! a connection; when absent, the "default" connection is used.

use indexmap::IndexMap;
use shaperail_core::{DatabaseEngine, NamedDatabaseConfig, ShaperailError};
use std::sync::Arc;

/// A single SQL database connection (SeaORM). Used for CRUD via ORM; migrations
/// may use the underlying pool separately.
#[derive(Clone)]
pub struct SqlConnection {
    /// SeaORM connection (Postgres, MySQL, or SQLite depending on config).
    pub inner: Arc<sea_orm::DatabaseConnection>,
    /// Engine for dialect-specific behavior.
    pub engine: DatabaseEngine,
}

/// Multi-database connection manager. Maps connection names to SQL connections.
///
/// Built from `ProjectConfig::databases` or from a single URL (default connection).
pub struct DatabaseManager {
    /// Named SQL connections. At least "default" when using SQL backends.
    connections: IndexMap<String, SqlConnection>,
}

impl DatabaseManager {
    /// Create a manager with a single "default" Postgres connection from URL.
    ///
    /// Used when `databases` is not set (legacy single-DB).
    pub async fn from_url(url: &str, pool_size: u32) -> Result<Self, ShaperailError> {
        let mut connections = IndexMap::new();
        let mut opt = sea_orm::ConnectOptions::new(url.to_string());
        opt.max_connections(pool_size).min_connections(1);
        let conn = sea_orm::Database::connect(opt)
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to connect to database: {e}")))?;
        connections.insert(
            "default".to_string(),
            SqlConnection {
                inner: Arc::new(conn),
                engine: DatabaseEngine::Postgres,
            },
        );
        Ok(Self { connections })
    }

    /// Create a manager from the `databases` config map.
    ///
    /// Only SQL engines (Postgres, MySQL, SQLite) are supported in this manager;
    /// MongoDB would be handled separately.
    pub async fn from_named_config(
        databases: &IndexMap<String, NamedDatabaseConfig>,
    ) -> Result<Self, ShaperailError> {
        let mut connections = IndexMap::new();
        for (name, cfg) in databases {
            if !cfg.engine.is_sql() {
                continue;
            }
            let url = &cfg.url;
            let mut opt = sea_orm::ConnectOptions::new(url.clone());
            opt.max_connections(cfg.pool_size).min_connections(1);
            let conn = sea_orm::Database::connect(opt).await.map_err(|e| {
                ShaperailError::Internal(format!("Failed to connect to database '{name}': {e}"))
            })?;
            connections.insert(
                name.clone(),
                SqlConnection {
                    inner: Arc::new(conn),
                    engine: cfg.engine,
                },
            );
        }
        if connections.is_empty() {
            return Err(ShaperailError::Internal(
                "No SQL databases configured in databases config".to_string(),
            ));
        }
        Ok(Self { connections })
    }

    /// Get the SQL connection for the given name. Returns None if name is not found
    /// or refers to a non-SQL backend (e.g. MongoDB).
    pub fn get_sql(&self, name: &str) -> Option<SqlConnection> {
        self.connections.get(name).cloned()
    }

    /// Connection name to use for a resource when resource.db is None.
    pub const DEFAULT_NAME: &'static str = "default";

    /// Resolve the connection name for a resource (its `db` field or "default").
    pub fn connection_name_for_resource(&self, db: Option<&String>) -> &str {
        if let Some(name) = db {
            if let Some((key, _)) = self.connections.get_key_value(name.as_str()) {
                return key.as_str();
            }
        }
        Self::DEFAULT_NAME
    }

    /// Returns the SQL connection for the given resource (by db name).
    pub fn sql_for_resource(&self, db: Option<&String>) -> Option<SqlConnection> {
        let name = self.connection_name_for_resource(db);
        self.get_sql(name)
    }

    /// Number of SQL connections in this manager.
    pub fn len(&self) -> usize {
        self.connections.len()
    }

    /// True if no connections.
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }
}
