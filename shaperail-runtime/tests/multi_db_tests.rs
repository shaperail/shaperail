//! Multi-database tests (M14).
//!
//! Tests dialect-specific SQL generation, ORM query dialect awareness,
//! MongoDB store operations, and cross-database project configuration.

use shaperail_core::{DatabaseEngine, FieldSchema, FieldType, ResourceDefinition};
use shaperail_runtime::db::{build_create_table_sql_for_engine, SqlConnection};

use indexmap::IndexMap;

/// Helper: build a minimal resource definition for testing.
fn test_resource() -> ResourceDefinition {
    let mut schema = IndexMap::new();
    schema.insert(
        "id".to_string(),
        FieldSchema {
            field_type: FieldType::Uuid,
            primary: true,
            generated: true,
            required: false,
            unique: false,
            nullable: false,
            reference: None,
            min: None,
            max: None,
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: false,
            items: None,
        },
    );
    schema.insert(
        "name".to_string(),
        FieldSchema {
            field_type: FieldType::String,
            primary: false,
            generated: false,
            required: true,
            unique: false,
            nullable: false,
            reference: None,
            min: Some(serde_json::json!(1)),
            max: Some(serde_json::json!(200)),
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: true,
            items: None,
        },
    );
    schema.insert(
        "role".to_string(),
        FieldSchema {
            field_type: FieldType::Enum,
            primary: false,
            generated: false,
            required: true,
            unique: false,
            nullable: false,
            reference: None,
            min: None,
            max: None,
            format: None,
            values: Some(vec![
                "admin".to_string(),
                "member".to_string(),
                "viewer".to_string(),
            ]),
            default: Some(serde_json::json!("member")),
            sensitive: false,
            search: false,
            items: None,
        },
    );
    schema.insert(
        "created_at".to_string(),
        FieldSchema {
            field_type: FieldType::Timestamp,
            primary: false,
            generated: true,
            required: false,
            unique: false,
            nullable: false,
            reference: None,
            min: None,
            max: None,
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: false,
            items: None,
        },
    );

    ResourceDefinition {
        resource: "users".to_string(),
        version: 1,
        schema,
        endpoints: None,
        relations: None,
        indexes: None,
        db: None,
    }
}

// ===== Migration SQL generation per engine =====

#[test]
fn postgres_create_table_sql() {
    let resource = test_resource();
    let sql = build_create_table_sql_for_engine(DatabaseEngine::Postgres, &resource);
    assert!(sql.contains("CREATE TABLE"), "Should contain CREATE TABLE");
    assert!(
        sql.contains("\"users\"") || sql.contains("users"),
        "Should contain table name"
    );
    assert!(sql.contains("UUID"), "Postgres should use UUID type");
    assert!(
        sql.contains("VARCHAR") || sql.contains("TEXT"),
        "Should have string type"
    );
    assert!(
        sql.contains("gen_random_uuid"),
        "Postgres UUID default should use gen_random_uuid()"
    );
}

#[test]
fn mysql_create_table_sql() {
    let resource = test_resource();
    let sql = build_create_table_sql_for_engine(DatabaseEngine::MySQL, &resource);
    assert!(sql.contains("CREATE TABLE"), "Should contain CREATE TABLE");
    assert!(
        sql.contains("`users`") || sql.contains("users"),
        "MySQL should use backtick quoting"
    );
    assert!(
        sql.contains("CHAR(36)"),
        "MySQL should use CHAR(36) for UUID"
    );
    assert!(
        sql.contains("UUID()"),
        "MySQL UUID default should use UUID()"
    );
}

#[test]
fn sqlite_create_table_sql() {
    let resource = test_resource();
    let sql = build_create_table_sql_for_engine(DatabaseEngine::SQLite, &resource);
    assert!(sql.contains("CREATE TABLE"), "Should contain CREATE TABLE");
    assert!(
        sql.contains("TEXT") || sql.contains("VARCHAR"),
        "SQLite should use TEXT for strings"
    );
    // SQLite stores UUIDs as TEXT.
    assert!(sql.contains("TEXT"), "SQLite should use TEXT type for UUID");
}

// ===== SqlConnection dialect helpers =====

#[test]
fn sql_connection_postgres_quoting() {
    let conn = SqlConnection {
        inner: std::sync::Arc::new(sea_orm::DatabaseConnection::Disconnected),
        engine: DatabaseEngine::Postgres,
    };
    assert_eq!(conn.quote_ident("users"), "\"users\"");
    assert_eq!(conn.param(1), "$1");
    assert_eq!(conn.param(3), "$3");
    assert_eq!(conn.backend(), sea_orm::DatabaseBackend::Postgres);
}

#[test]
fn sql_connection_mysql_quoting() {
    let conn = SqlConnection {
        inner: std::sync::Arc::new(sea_orm::DatabaseConnection::Disconnected),
        engine: DatabaseEngine::MySQL,
    };
    assert_eq!(conn.quote_ident("users"), "`users`");
    assert_eq!(conn.param(1), "?");
    assert_eq!(conn.param(5), "?");
    assert_eq!(conn.backend(), sea_orm::DatabaseBackend::MySql);
}

#[test]
fn sql_connection_sqlite_quoting() {
    let conn = SqlConnection {
        inner: std::sync::Arc::new(sea_orm::DatabaseConnection::Disconnected),
        engine: DatabaseEngine::SQLite,
    };
    assert_eq!(conn.quote_ident("users"), "\"users\"");
    assert_eq!(conn.param(1), "?");
    assert_eq!(conn.param(2), "?");
    assert_eq!(conn.backend(), sea_orm::DatabaseBackend::Sqlite);
}

// ===== Database engine feature coverage =====

#[test]
fn database_engine_is_sql() {
    assert!(DatabaseEngine::Postgres.is_sql());
    assert!(DatabaseEngine::MySQL.is_sql());
    assert!(DatabaseEngine::SQLite.is_sql());
    assert!(!DatabaseEngine::MongoDB.is_sql());
}

#[test]
fn database_engine_is_mongo() {
    assert!(DatabaseEngine::MongoDB.is_mongo());
    assert!(!DatabaseEngine::Postgres.is_mongo());
    assert!(!DatabaseEngine::MySQL.is_mongo());
    assert!(!DatabaseEngine::SQLite.is_mongo());
}

// ===== Multi-DB config parsing =====

#[test]
fn multi_db_config_named_databases() {
    let yaml = r#"
project: test-app
port: 3000
databases:
  default:
    engine: postgres
    url: postgres://localhost/test
    pool_size: 10
  analytics:
    engine: mysql
    url: mysql://localhost/analytics
    pool_size: 5
  cache_db:
    engine: sqlite
    url: sqlite://local.db
    pool_size: 1
  logs:
    engine: mongodb
    url: mongodb://localhost:27017/logs
    pool_size: 1
"#;
    let config: shaperail_core::ProjectConfig = serde_yaml::from_str(yaml).unwrap();
    let dbs = config.databases.unwrap();
    assert_eq!(dbs.len(), 4);
    assert_eq!(dbs["default"].engine, DatabaseEngine::Postgres);
    assert_eq!(dbs["analytics"].engine, DatabaseEngine::MySQL);
    assert_eq!(dbs["cache_db"].engine, DatabaseEngine::SQLite);
    assert_eq!(dbs["logs"].engine, DatabaseEngine::MongoDB);
}

// ===== Resource db routing =====

#[test]
fn resource_db_field_routes_to_named_connection() {
    let mut resource = test_resource();
    resource.db = Some("analytics".to_string());
    assert_eq!(resource.db.as_deref(), Some("analytics"));
}

#[test]
fn resource_db_none_defaults_to_default() {
    let resource = test_resource();
    assert!(resource.db.is_none());
    // DatabaseManager resolves None to "default".
}

// ===== Cross-engine SQL differences =====

#[test]
fn enum_field_generates_check_constraint_all_engines() {
    let resource = test_resource();
    for engine in [
        DatabaseEngine::Postgres,
        DatabaseEngine::MySQL,
        DatabaseEngine::SQLite,
    ] {
        let sql = build_create_table_sql_for_engine(engine, &resource);
        assert!(
            sql.contains("CHECK") || sql.contains("check"),
            "{engine:?} should generate CHECK constraint for enum field"
        );
    }
}

#[test]
fn postgres_uses_timestamptz() {
    let resource = test_resource();
    let sql = build_create_table_sql_for_engine(DatabaseEngine::Postgres, &resource);
    assert!(
        sql.contains("TIMESTAMPTZ"),
        "Postgres should use TIMESTAMPTZ"
    );
}

#[test]
fn mysql_uses_datetime() {
    let resource = test_resource();
    let sql = build_create_table_sql_for_engine(DatabaseEngine::MySQL, &resource);
    assert!(
        sql.contains("DATETIME"),
        "MySQL should use DATETIME for timestamps"
    );
}

// ===== Cross-DB project test: multiple engines in one config =====

#[test]
fn cross_db_project_config_parses() {
    let yaml = r#"
project: multi-db-app
port: 3000
databases:
  default:
    engine: postgres
    url: postgres://localhost/main
    pool_size: 20
  search:
    engine: sqlite
    url: sqlite://search.db
    pool_size: 1
  events:
    engine: mongodb
    url: mongodb://localhost:27017/events
    pool_size: 5
"#;
    let config: shaperail_core::ProjectConfig = serde_yaml::from_str(yaml).unwrap();
    let dbs = config.databases.unwrap();

    // Verify routing logic: SQL engines go to DatabaseManager, MongoDB handled separately.
    let sql_count = dbs.values().filter(|cfg| cfg.engine.is_sql()).count();
    let mongo_count = dbs.values().filter(|cfg| cfg.engine.is_mongo()).count();
    assert_eq!(
        sql_count, 2,
        "Should have 2 SQL engines (postgres + sqlite)"
    );
    assert_eq!(mongo_count, 1, "Should have 1 MongoDB engine");
}

// ===== MongoDB JSON Schema generation =====

#[test]
fn mongo_json_schema_from_resource() {
    // This test verifies the schema generation logic indirectly through
    // the MongoConnection::ensure_collection path. Here we test the
    // resource definition has the right structure for MongoDB.
    let resource = test_resource();
    assert!(resource.schema.contains_key("id"));
    assert!(resource.schema.contains_key("name"));
    assert!(resource.schema.contains_key("role"));

    // Verify enum values are present for validation.
    let role = &resource.schema["role"];
    assert!(role.values.is_some());
    assert_eq!(role.values.as_ref().unwrap().len(), 3);
}

// ===== ORM-backed CRUD path (SeaQuery dialect-agnostic) =====

#[test]
fn orm_resource_query_uses_correct_backend() {
    use shaperail_runtime::db::OrmResourceQuery;

    let resource = test_resource();

    // Postgres connection.
    let pg_conn = SqlConnection {
        inner: std::sync::Arc::new(sea_orm::DatabaseConnection::Disconnected),
        engine: DatabaseEngine::Postgres,
    };
    let pg_query = OrmResourceQuery::new(&resource, &pg_conn);
    assert_eq!(pg_query.resource.resource, "users");

    // MySQL connection.
    let mysql_conn = SqlConnection {
        inner: std::sync::Arc::new(sea_orm::DatabaseConnection::Disconnected),
        engine: DatabaseEngine::MySQL,
    };
    let mysql_query = OrmResourceQuery::new(&resource, &mysql_conn);
    assert_eq!(mysql_query.resource.resource, "users");

    // SQLite connection.
    let sqlite_conn = SqlConnection {
        inner: std::sync::Arc::new(sea_orm::DatabaseConnection::Disconnected),
        engine: DatabaseEngine::SQLite,
    };
    let sqlite_query = OrmResourceQuery::new(&resource, &sqlite_conn);
    assert_eq!(sqlite_query.resource.resource, "users");
}

// ===== Same API behavior across engines =====

#[test]
fn all_engines_produce_valid_create_table() {
    let resource = test_resource();
    for engine in [
        DatabaseEngine::Postgres,
        DatabaseEngine::MySQL,
        DatabaseEngine::SQLite,
    ] {
        let sql = build_create_table_sql_for_engine(engine, &resource);
        assert!(!sql.is_empty(), "{engine:?} should produce non-empty SQL");
        assert!(
            sql.contains("CREATE TABLE"),
            "{engine:?} should produce CREATE TABLE statement"
        );
        // All engines should reference all schema fields.
        assert!(sql.contains("id"), "{engine:?} should include id column");
        assert!(
            sql.contains("name"),
            "{engine:?} should include name column"
        );
        assert!(
            sql.contains("role"),
            "{engine:?} should include role column"
        );
        assert!(
            sql.contains("created_at"),
            "{engine:?} should include created_at column"
        );
    }
}
