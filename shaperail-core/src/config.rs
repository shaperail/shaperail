use serde::{Deserialize, Serialize};

use crate::DatabaseEngine;

/// Project-level configuration, parsed from `shaperail.config.yaml`.
///
/// ```yaml
/// project: my-api
/// port: 3000
/// workers: auto
/// database:
///   type: postgresql
///   host: localhost
///   port: 5432
///   name: my_api_db
///   pool_size: 20
/// ```
///
/// Multi-database (M14):
/// ```yaml
/// databases:
///   default:
///     engine: postgres
///     url: ${DATABASE_URL}
///   analytics:
///     engine: mysql
///     url: mysql://user:pass@localhost/analytics
///   cache_db:
///     engine: sqlite
///     url: file:cache.db
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    /// Project name.
    pub project: String,

    /// HTTP server port.
    #[serde(default = "default_port")]
    pub port: u16,

    /// Number of worker threads ("auto" or a number).
    #[serde(default = "default_workers")]
    pub workers: WorkerCount,

    /// Single database configuration (legacy). Ignored if `databases` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<DatabaseConfig>,

    /// Named database connections (M14). When set, resources use `db: <name>` to select connection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub databases: Option<indexmap::IndexMap<String, NamedDatabaseConfig>>,

    /// Cache (Redis) configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheConfig>,

    /// Authentication configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,

    /// Object storage configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<StorageConfig>,

    /// Logging and observability configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingConfig>,

    /// Events and webhooks configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<EventsConfig>,

    /// Enabled API protocols (M15/M16). Default when omitted: `["rest"]`. Allowed: `rest`, `graphql`, `grpc`.
    #[serde(default = "default_protocols")]
    pub protocols: Vec<String>,

    /// GraphQL configuration (M15). Depth and complexity limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphql: Option<GraphQLConfig>,

    /// gRPC configuration (M16). Port and reflection settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc: Option<GrpcConfig>,
}

/// GraphQL-specific configuration (M15).
///
/// ```yaml
/// graphql:
///   depth_limit: 10
///   complexity_limit: 200
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphQLConfig {
    /// Maximum query nesting depth. Default: 16.
    #[serde(default = "default_depth_limit")]
    pub depth_limit: usize,

    /// Maximum query complexity score. Default: 256.
    #[serde(default = "default_complexity_limit")]
    pub complexity_limit: usize,
}

/// gRPC-specific configuration (M16).
///
/// ```yaml
/// grpc:
///   port: 50051
///   reflection: true
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrpcConfig {
    /// gRPC server port. Default: 50051.
    #[serde(default = "default_grpc_port")]
    pub port: u16,

    /// Enable gRPC server reflection (for grpcurl). Default: true.
    #[serde(default = "default_grpc_reflection")]
    pub reflection: bool,
}

fn default_grpc_port() -> u16 {
    50051
}

fn default_grpc_reflection() -> bool {
    true
}

fn default_depth_limit() -> usize {
    16
}

fn default_complexity_limit() -> usize {
    256
}

fn default_port() -> u16 {
    3000
}

fn default_protocols() -> Vec<String> {
    vec!["rest".to_string()]
}

fn default_workers() -> WorkerCount {
    WorkerCount::Auto
}

/// Worker thread count: either automatic or a fixed number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerCount {
    /// Automatically detect based on CPU cores.
    Auto,
    /// Fixed number of workers.
    Fixed(usize),
}

impl Serialize for WorkerCount {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Auto => serializer.serialize_str("auto"),
            Self::Fixed(n) => serializer.serialize_u64(*n as u64),
        }
    }
}

impl<'de> Deserialize<'de> for WorkerCount {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        match &value {
            serde_json::Value::String(s) if s == "auto" => Ok(Self::Auto),
            serde_json::Value::Number(n) => n
                .as_u64()
                .map(|v| Self::Fixed(v as usize))
                .ok_or_else(|| serde::de::Error::custom("workers must be a positive integer")),
            _ => Err(serde::de::Error::custom(
                "workers must be \"auto\" or a positive integer",
            )),
        }
    }
}

/// Database connection configuration (legacy single-DB).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    /// Database type (e.g., "postgresql").
    #[serde(rename = "type")]
    pub db_type: String,

    /// Database host.
    #[serde(default = "default_host")]
    pub host: String,

    /// Database port.
    #[serde(default = "default_db_port")]
    pub port: u16,

    /// Database name.
    pub name: String,

    /// Connection pool size.
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
}

/// Named database connection (M14 multi-database). Used in `databases: <name>: <config>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedDatabaseConfig {
    /// Engine: postgres, mysql, sqlite, mongodb.
    pub engine: DatabaseEngine,

    /// Connection URL. Env var interpolation supported (e.g. ${DATABASE_URL}).
    pub url: String,

    /// Connection pool size (SQL only). Default 20.
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_db_port() -> u16 {
    5432
}

fn default_pool_size() -> u32 {
    20
}

/// Redis cache configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    /// Cache type (e.g., "redis").
    #[serde(rename = "type")]
    pub cache_type: String,

    /// Redis connection URL.
    pub url: String,
}

/// Authentication configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    /// Auth provider (e.g., "jwt").
    pub provider: String,

    /// Environment variable name holding the JWT secret.
    pub secret_env: String,

    /// Token expiry duration (e.g., "24h").
    pub expiry: String,

    /// Refresh token expiry duration (e.g., "30d").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_expiry: Option<String>,
}

/// Object storage configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    /// Storage provider (e.g., "s3", "gcs", "local").
    pub provider: String,

    /// Storage bucket name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,

    /// Cloud region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

/// Logging and observability configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    /// Log level (e.g., "info", "debug", "warn").
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Log format (e.g., "json", "pretty").
    #[serde(default = "default_log_format")]
    pub format: String,

    /// OpenTelemetry OTLP endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otlp_endpoint: Option<String>,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "json".to_string()
}

/// Events and webhooks configuration.
///
/// ```yaml
/// events:
///   subscribers:
///     - event: "user.created"
///       targets:
///         - type: webhook
///           url: "https://example.com/hooks/user-created"
///         - type: job
///           name: send_welcome_email
///         - type: channel
///           name: notifications
///           room: "org:{org_id}"
///   webhooks:
///     secret_env: WEBHOOK_SECRET
///     timeout_secs: 30
///     max_retries: 3
///   inbound:
///     - path: /webhooks/stripe
///       secret_env: STRIPE_WEBHOOK_SECRET
///       events: ["payment.completed", "subscription.updated"]
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventsConfig {
    /// Event subscriber definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscribers: Vec<EventSubscriber>,

    /// Outbound webhook global settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhooks: Option<WebhookConfig>,

    /// Inbound webhook endpoint definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inbound: Vec<InboundWebhookConfig>,
}

/// An event subscriber routes events to targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventSubscriber {
    /// Event name pattern (e.g., "user.created", "*.deleted").
    pub event: String,

    /// Targets to dispatch the event to.
    pub targets: Vec<EventTarget>,
}

/// A target for event dispatch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum EventTarget {
    /// Enqueue a background job.
    Job { name: String },
    /// POST to an external webhook URL.
    Webhook { url: String },
    /// Broadcast to a WebSocket channel/room.
    Channel {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        room: Option<String>,
    },
    /// Execute a hook function.
    Hook { name: String },
}

/// Global outbound webhook settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookConfig {
    /// Environment variable holding the HMAC signing secret.
    #[serde(default = "default_webhook_secret_env")]
    pub secret_env: String,

    /// HTTP timeout for webhook delivery in seconds.
    #[serde(default = "default_webhook_timeout")]
    pub timeout_secs: u64,

    /// Maximum retry attempts for failed deliveries.
    #[serde(default = "default_webhook_max_retries")]
    pub max_retries: u32,
}

fn default_webhook_secret_env() -> String {
    "WEBHOOK_SECRET".to_string()
}

fn default_webhook_timeout() -> u64 {
    30
}

fn default_webhook_max_retries() -> u32 {
    3
}

/// Inbound webhook endpoint configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboundWebhookConfig {
    /// URL path for the inbound webhook (e.g., "/webhooks/stripe").
    pub path: String,

    /// Environment variable holding the verification secret.
    pub secret_env: String,

    /// Event names this endpoint accepts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_config_minimal() {
        let json = r#"{"project": "my-app"}"#;
        let cfg: ProjectConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.project, "my-app");
        assert_eq!(cfg.port, 3000);
        assert_eq!(cfg.workers, WorkerCount::Auto);
        assert!(cfg.database.is_none());
        assert_eq!(cfg.protocols, vec!["rest"]);
    }

    #[test]
    fn project_config_full() {
        let json = r#"{
            "project": "my-api",
            "port": 8080,
            "workers": 4,
            "database": {
                "type": "postgresql",
                "host": "db.example.com",
                "port": 5433,
                "name": "my_db",
                "pool_size": 10
            },
            "cache": {
                "type": "redis",
                "url": "redis://localhost:6379"
            },
            "auth": {
                "provider": "jwt",
                "secret_env": "JWT_SECRET",
                "expiry": "24h",
                "refresh_expiry": "30d"
            },
            "storage": {
                "provider": "s3",
                "bucket": "my-bucket",
                "region": "us-east-1"
            },
            "logging": {
                "level": "debug",
                "format": "json",
                "otlp_endpoint": "http://localhost:4317"
            }
        }"#;
        let cfg: ProjectConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.workers, WorkerCount::Fixed(4));
        let db = cfg.database.unwrap();
        assert_eq!(db.db_type, "postgresql");
        assert_eq!(db.host, "db.example.com");
        assert_eq!(db.port, 5433);
        assert_eq!(db.pool_size, 10);
        let cache = cfg.cache.unwrap();
        assert_eq!(cache.cache_type, "redis");
        let auth = cfg.auth.unwrap();
        assert_eq!(auth.provider, "jwt");
        assert_eq!(auth.refresh_expiry.as_deref(), Some("30d"));
        let storage = cfg.storage.unwrap();
        assert_eq!(storage.provider, "s3");
        let logging = cfg.logging.unwrap();
        assert_eq!(logging.level, "debug");
        assert_eq!(
            logging.otlp_endpoint.as_deref(),
            Some("http://localhost:4317")
        );
    }

    #[test]
    fn worker_count_auto() {
        let wc: WorkerCount = serde_json::from_str("\"auto\"").unwrap();
        assert_eq!(wc, WorkerCount::Auto);
    }

    #[test]
    fn worker_count_fixed() {
        let wc: WorkerCount = serde_json::from_str("8").unwrap();
        assert_eq!(wc, WorkerCount::Fixed(8));
    }

    #[test]
    fn database_config_defaults() {
        let json = r#"{"type": "postgresql", "name": "test_db"}"#;
        let db: DatabaseConfig = serde_json::from_str(json).unwrap();
        assert_eq!(db.host, "localhost");
        assert_eq!(db.port, 5432);
        assert_eq!(db.pool_size, 20);
    }

    #[test]
    fn logging_config_defaults() {
        let json = r#"{}"#;
        let log: LoggingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(log.level, "info");
        assert_eq!(log.format, "json");
        assert!(log.otlp_endpoint.is_none());
    }

    #[test]
    fn project_config_serde_roundtrip() {
        let cfg = ProjectConfig {
            project: "roundtrip-test".to_string(),
            port: 3000,
            workers: WorkerCount::Auto,
            database: Some(DatabaseConfig {
                db_type: "postgresql".to_string(),
                host: "localhost".to_string(),
                port: 5432,
                name: "test".to_string(),
                pool_size: 20,
            }),
            databases: None,
            cache: None,
            auth: None,
            storage: None,
            logging: None,
            events: None,
            protocols: vec!["rest".to_string()],
            graphql: None,
            grpc: None,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ProjectConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn events_config_serde() {
        let json = r#"{
            "subscribers": [
                {
                    "event": "users.created",
                    "targets": [
                        {"type": "job", "name": "send_welcome_email"},
                        {"type": "webhook", "url": "https://example.com/hook"},
                        {"type": "channel", "name": "notifications", "room": "org:123"},
                        {"type": "hook", "name": "validate_org"}
                    ]
                },
                {
                    "event": "*.deleted",
                    "targets": [
                        {"type": "job", "name": "cleanup_job"}
                    ]
                }
            ],
            "webhooks": {
                "secret_env": "MY_SECRET",
                "timeout_secs": 15,
                "max_retries": 5
            },
            "inbound": [
                {
                    "path": "/webhooks/stripe",
                    "secret_env": "STRIPE_SECRET",
                    "events": ["payment.completed"]
                }
            ]
        }"#;
        let cfg: EventsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.subscribers.len(), 2);
        assert_eq!(cfg.subscribers[0].event, "users.created");
        assert_eq!(cfg.subscribers[0].targets.len(), 4);
        assert!(
            matches!(&cfg.subscribers[0].targets[0], EventTarget::Job { name } if name == "send_welcome_email")
        );
        assert!(
            matches!(&cfg.subscribers[0].targets[1], EventTarget::Webhook { url } if url == "https://example.com/hook")
        );
        assert!(
            matches!(&cfg.subscribers[0].targets[2], EventTarget::Channel { name, room } if name == "notifications" && room.as_deref() == Some("org:123"))
        );
        assert!(
            matches!(&cfg.subscribers[0].targets[3], EventTarget::Hook { name } if name == "validate_org")
        );
        let webhooks = cfg.webhooks.unwrap();
        assert_eq!(webhooks.secret_env, "MY_SECRET");
        assert_eq!(webhooks.timeout_secs, 15);
        assert_eq!(webhooks.max_retries, 5);
        assert_eq!(cfg.inbound.len(), 1);
        assert_eq!(cfg.inbound[0].path, "/webhooks/stripe");
    }

    #[test]
    fn webhook_config_defaults() {
        let json = r#"{}"#;
        let cfg: WebhookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.secret_env, "WEBHOOK_SECRET");
        assert_eq!(cfg.timeout_secs, 30);
        assert_eq!(cfg.max_retries, 3);
    }

    #[test]
    fn events_config_empty() {
        let json = r#"{"subscribers": [], "inbound": []}"#;
        let cfg: EventsConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.subscribers.is_empty());
        assert!(cfg.webhooks.is_none());
        assert!(cfg.inbound.is_empty());
    }

    #[test]
    fn named_database_config() {
        let json =
            r#"{"engine": "postgres", "url": "postgresql://localhost/mydb", "pool_size": 10}"#;
        let cfg: NamedDatabaseConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.engine, DatabaseEngine::Postgres);
        assert_eq!(cfg.url, "postgresql://localhost/mydb");
        assert_eq!(cfg.pool_size, 10);
    }

    #[test]
    fn project_config_databases() {
        let json = r#"{
            "project": "multi-db",
            "databases": {
                "default": {"engine": "postgres", "url": "postgresql:///main"},
                "analytics": {"engine": "mysql", "url": "mysql://localhost/analytics"}
            }
        }"#;
        let cfg: ProjectConfig = serde_json::from_str(json).unwrap();
        let dbs = cfg.databases.as_ref().unwrap();
        assert_eq!(dbs.len(), 2);
        assert_eq!(dbs.get("default").unwrap().engine, DatabaseEngine::Postgres);
        assert_eq!(dbs.get("analytics").unwrap().engine, DatabaseEngine::MySQL);
    }

    #[test]
    fn project_config_protocols() {
        let json = r#"{"project": "gql-api", "protocols": ["rest", "graphql"]}"#;
        let cfg: ProjectConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.protocols, vec!["rest", "graphql"]);
    }

    #[test]
    fn project_config_grpc_protocol() {
        let json = r#"{"project": "grpc-api", "protocols": ["rest", "grpc"]}"#;
        let cfg: ProjectConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.protocols, vec!["rest", "grpc"]);
        assert!(cfg.grpc.is_none());
    }

    #[test]
    fn grpc_config_defaults() {
        let json = r#"{}"#;
        let cfg: GrpcConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.port, 50051);
        assert!(cfg.reflection);
    }

    #[test]
    fn grpc_config_custom() {
        let json = r#"{"port": 9090, "reflection": false}"#;
        let cfg: GrpcConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.port, 9090);
        assert!(!cfg.reflection);
    }

    #[test]
    fn project_config_with_grpc() {
        let json = r#"{
            "project": "grpc-app",
            "protocols": ["rest", "grpc"],
            "grpc": {"port": 50052, "reflection": true}
        }"#;
        let cfg: ProjectConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.protocols, vec!["rest", "grpc"]);
        let grpc = cfg.grpc.unwrap();
        assert_eq!(grpc.port, 50052);
        assert!(grpc.reflection);
    }
}
