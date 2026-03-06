use serde::{Deserialize, Serialize};

/// Project-level configuration, parsed from `steel.config.yaml`.
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Project name.
    pub project: String,

    /// HTTP server port.
    #[serde(default = "default_port")]
    pub port: u16,

    /// Number of worker threads ("auto" or a number).
    #[serde(default = "default_workers")]
    pub workers: WorkerCount,

    /// Database configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<DatabaseConfig>,

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
}

fn default_port() -> u16 {
    3000
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

/// Database connection configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct CacheConfig {
    /// Cache type (e.g., "redis").
    #[serde(rename = "type")]
    pub cache_type: String,

    /// Redis connection URL.
    pub url: String,
}

/// Authentication configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
            cache: None,
            auth: None,
            storage: None,
            logging: None,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ProjectConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }
}
