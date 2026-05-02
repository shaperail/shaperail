//! Database engine and multi-database configuration types for M14.

use serde::{Deserialize, Serialize};

/// Supported database engines for multi-database (M14).
///
/// SQL engines (Postgres, MySQL, SQLite) use the ORM layer; MongoDB uses its own driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseEngine {
    /// PostgreSQL — full feature coverage.
    Postgres,

    /// MySQL / MariaDB — 95% feature coverage.
    MySQL,

    /// SQLite — 85% feature coverage (e.g. no full-text search like Postgres).
    SQLite,

    /// MongoDB — 75% feature coverage via mongodb crate.
    MongoDB,
}

impl DatabaseEngine {
    /// Default engine when not specified (single-DB backward compat).
    pub const fn default_engine() -> Self {
        Self::Postgres
    }

    /// Returns true for SQL backends (Postgres, MySQL, SQLite).
    pub const fn is_sql(&self) -> bool {
        matches!(self, Self::Postgres | Self::MySQL | Self::SQLite)
    }

    /// Returns true for MongoDB.
    pub const fn is_mongo(&self) -> bool {
        matches!(self, Self::MongoDB)
    }
}

impl Default for DatabaseEngine {
    fn default() -> Self {
        Self::default_engine()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_is_sql() {
        assert!(DatabaseEngine::Postgres.is_sql());
        assert!(DatabaseEngine::MySQL.is_sql());
        assert!(DatabaseEngine::SQLite.is_sql());
        assert!(!DatabaseEngine::MongoDB.is_sql());
    }

    #[test]
    fn engine_serde() {
        let s = serde_json::to_string(&DatabaseEngine::Postgres).unwrap();
        assert_eq!(s, r#""postgres""#);
        let e: DatabaseEngine = serde_json::from_str(&s).unwrap();
        assert_eq!(e, DatabaseEngine::Postgres);
    }

    #[test]
    fn engine_is_mongo() {
        assert!(DatabaseEngine::MongoDB.is_mongo());
        assert!(!DatabaseEngine::Postgres.is_mongo());
        assert!(!DatabaseEngine::MySQL.is_mongo());
        assert!(!DatabaseEngine::SQLite.is_mongo());
    }

    #[test]
    fn engine_default_is_postgres() {
        assert_eq!(DatabaseEngine::default(), DatabaseEngine::Postgres);
        assert_eq!(DatabaseEngine::default_engine(), DatabaseEngine::Postgres);
    }

    #[test]
    fn all_engine_variants_serde() {
        let pairs = [
            (DatabaseEngine::Postgres, "postgres"),
            (DatabaseEngine::MySQL, "mysql"),
            (DatabaseEngine::SQLite, "sqlite"),
            (DatabaseEngine::MongoDB, "mongodb"),
        ];
        for (engine, expected_str) in pairs {
            let json = serde_json::to_string(&engine).unwrap();
            assert_eq!(json, format!("\"{expected_str}\""));
            let back: DatabaseEngine = serde_json::from_str(&json).unwrap();
            assert_eq!(back, engine);
        }
    }
}
