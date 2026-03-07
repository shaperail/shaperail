//! # steel-core
//!
//! Shared foundation types for the SteelAPI framework.
//!
//! This crate defines the core data structures that all other SteelAPI crates
//! consume: resource definitions, field types, endpoint specs, error handling,
//! and project configuration.

mod channel;
mod config;
mod endpoint;
mod error;
mod field_type;
mod relation;
mod resource;
mod schema;

pub use channel::{ChannelDefinition, ChannelHooks, WsClientMessage, WsServerMessage};
pub use config::{
    AuthConfig, CacheConfig, DatabaseConfig, EventSubscriber, EventTarget, EventsConfig,
    InboundWebhookConfig, LoggingConfig, ProjectConfig, StorageConfig, WebhookConfig, WorkerCount,
};
pub use endpoint::{AuthRule, CacheSpec, EndpointSpec, HttpMethod, PaginationStyle, UploadSpec};
pub use error::{FieldError, SteelError};
pub use field_type::FieldType;
pub use relation::{IndexSpec, RelationSpec, RelationType};
pub use resource::ResourceDefinition;
pub use schema::FieldSchema;
