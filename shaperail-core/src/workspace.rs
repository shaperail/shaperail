use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{AuthConfig, CacheConfig};

/// Workspace configuration parsed from `shaperail.workspace.yaml`.
///
/// Declares multiple services that form a distributed system.
///
/// ```yaml
/// workspace: my-platform
/// services:
///   users-api:
///     path: services/users-api
///     port: 3001
///   orders-api:
///     path: services/orders-api
///     port: 3002
///     depends_on: [users-api]
/// shared:
///   cache:
///     type: redis
///     url: redis://localhost:6379
///   auth:
///     provider: jwt
///     secret_env: JWT_SECRET
///     expiry: 24h
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    /// Workspace name.
    pub workspace: String,

    /// Named services in the workspace.
    pub services: IndexMap<String, ServiceDefinition>,

    /// Shared configuration inherited by all services.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared: Option<SharedConfig>,
}

/// A single service within a workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceDefinition {
    /// Relative path to the service directory (from workspace root).
    pub path: String,

    /// HTTP port for this service.
    #[serde(default = "default_service_port")]
    pub port: u16,

    /// Services this service depends on (must start first).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

fn default_service_port() -> u16 {
    3000
}

/// Shared configuration inherited by all services in a workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedConfig {
    /// Shared Redis cache configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheConfig>,

    /// Shared authentication configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
}

/// Entry stored in Redis service registry. Services register on startup and
/// update their heartbeat periodically.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceRegistryEntry {
    /// Service name (matches key in workspace config).
    pub name: String,

    /// Base URL for this service (e.g. "http://localhost:3001").
    pub url: String,

    /// HTTP port.
    pub port: u16,

    /// Resource names this service exposes.
    pub resources: Vec<String>,

    /// Enabled protocols (rest, graphql, grpc).
    pub protocols: Vec<String>,

    /// Current service status.
    pub status: ServiceStatus,

    /// ISO 8601 timestamp of initial registration.
    pub registered_at: String,

    /// ISO 8601 timestamp of last heartbeat.
    pub last_heartbeat: String,
}

/// Service health status in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    /// Service is starting up.
    Starting,
    /// Service is healthy and accepting requests.
    Healthy,
    /// Service has missed heartbeats.
    Unhealthy,
    /// Service has been stopped.
    Stopped,
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Starting => write!(f, "starting"),
            Self::Healthy => write!(f, "healthy"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Stopped => write!(f, "stopped"),
        }
    }
}

/// Configuration for an auto-generated inter-service client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterServiceClientConfig {
    /// Target service name.
    pub service: String,

    /// Base URL (resolved from registry at runtime).
    pub base_url: String,

    /// Request timeout in seconds.
    #[serde(default = "default_client_timeout")]
    pub timeout_secs: u64,

    /// Number of retries on transient failures.
    #[serde(default = "default_client_retries")]
    pub retry_count: u32,
}

fn default_client_timeout() -> u64 {
    10
}

fn default_client_retries() -> u32 {
    3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_config_minimal() {
        let json = r#"{
            "workspace": "my-platform",
            "services": {
                "api": {"path": "services/api"}
            }
        }"#;
        let cfg: WorkspaceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.workspace, "my-platform");
        assert_eq!(cfg.services.len(), 1);
        let api = cfg.services.get("api").unwrap();
        assert_eq!(api.path, "services/api");
        assert_eq!(api.port, 3000);
        assert!(api.depends_on.is_empty());
        assert!(cfg.shared.is_none());
    }

    #[test]
    fn workspace_config_full() {
        let json = r#"{
            "workspace": "my-platform",
            "services": {
                "users-api": {
                    "path": "services/users-api",
                    "port": 3001
                },
                "orders-api": {
                    "path": "services/orders-api",
                    "port": 3002,
                    "depends_on": ["users-api"]
                }
            },
            "shared": {
                "cache": {
                    "type": "redis",
                    "url": "redis://localhost:6379"
                },
                "auth": {
                    "provider": "jwt",
                    "secret_env": "JWT_SECRET",
                    "expiry": "24h"
                }
            }
        }"#;
        let cfg: WorkspaceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.workspace, "my-platform");
        assert_eq!(cfg.services.len(), 2);

        let orders = cfg.services.get("orders-api").unwrap();
        assert_eq!(orders.port, 3002);
        assert_eq!(orders.depends_on, vec!["users-api"]);

        let shared = cfg.shared.unwrap();
        assert!(shared.cache.is_some());
        assert!(shared.auth.is_some());
    }

    #[test]
    fn workspace_config_unknown_field_fails() {
        let json = r#"{
            "workspace": "test",
            "services": {},
            "unknown_field": true
        }"#;
        let err = serde_json::from_str::<WorkspaceConfig>(json);
        assert!(err.is_err());
    }

    #[test]
    fn service_registry_entry_serde_roundtrip() {
        let entry = ServiceRegistryEntry {
            name: "users-api".to_string(),
            url: "http://localhost:3001".to_string(),
            port: 3001,
            resources: vec!["users".to_string(), "profiles".to_string()],
            protocols: vec!["rest".to_string()],
            status: ServiceStatus::Healthy,
            registered_at: "2026-01-01T00:00:00Z".to_string(),
            last_heartbeat: "2026-01-01T00:01:00Z".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: ServiceRegistryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, back);
    }

    #[test]
    fn service_status_display() {
        assert_eq!(ServiceStatus::Starting.to_string(), "starting");
        assert_eq!(ServiceStatus::Healthy.to_string(), "healthy");
        assert_eq!(ServiceStatus::Unhealthy.to_string(), "unhealthy");
        assert_eq!(ServiceStatus::Stopped.to_string(), "stopped");
    }

    #[test]
    fn inter_service_client_config_defaults() {
        let json = r#"{"service": "users-api", "base_url": "http://localhost:3001"}"#;
        let cfg: InterServiceClientConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.service, "users-api");
        assert_eq!(cfg.timeout_secs, 10);
        assert_eq!(cfg.retry_count, 3);
    }

    #[test]
    fn service_definition_defaults() {
        let json = r#"{"path": "services/api"}"#;
        let svc: ServiceDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(svc.port, 3000);
        assert!(svc.depends_on.is_empty());
    }

    #[test]
    fn workspace_config_serde_roundtrip() {
        let cfg = WorkspaceConfig {
            workspace: "test-workspace".to_string(),
            services: {
                let mut m = IndexMap::new();
                m.insert(
                    "svc-a".to_string(),
                    ServiceDefinition {
                        path: "services/svc-a".to_string(),
                        port: 3001,
                        depends_on: vec![],
                    },
                );
                m.insert(
                    "svc-b".to_string(),
                    ServiceDefinition {
                        path: "services/svc-b".to_string(),
                        port: 3002,
                        depends_on: vec!["svc-a".to_string()],
                    },
                );
                m
            },
            shared: None,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: WorkspaceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }
}
