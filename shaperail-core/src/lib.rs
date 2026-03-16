//! # shaperail-core
//!
//! Shared foundation types for the Shaperail framework.
//!
//! This crate defines the core data structures that all other Shaperail crates
//! consume: resource definitions, field types, endpoint specs, error handling,
//! and project configuration.

mod channel;
mod config;
mod database;
mod endpoint;
mod error;
mod field_type;
mod relation;
mod resource;
mod saga;
mod schema;
mod workspace;

pub use channel::{ChannelDefinition, ChannelHooks, WsClientMessage, WsServerMessage};
pub use config::{
    AuthConfig, CacheConfig, DatabaseConfig, EventSubscriber, EventTarget, EventsConfig,
    GraphQLConfig, GrpcConfig, InboundWebhookConfig, LoggingConfig, NamedDatabaseConfig,
    ProjectConfig, StorageConfig, WebhookConfig, WorkerCount,
};
pub use database::DatabaseEngine;
pub use endpoint::{
    AuthRule, CacheSpec, ControllerSpec, EndpointSpec, HttpMethod, PaginationStyle, UploadSpec,
    WASM_HOOK_PREFIX,
};
pub use error::{FieldError, ShaperailError};
pub use field_type::FieldType;
pub use relation::{IndexSpec, RelationSpec, RelationType};
pub use resource::ResourceDefinition;
pub use saga::{SagaDefinition, SagaExecutionStatus, SagaStep};
pub use schema::FieldSchema;
pub use workspace::{
    InterServiceClientConfig, ServiceDefinition, ServiceRegistryEntry, ServiceStatus, SharedConfig,
    WorkspaceConfig,
};
