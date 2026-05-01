use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{EndpointSpec, FieldSchema, IndexSpec, RelationSpec};

/// Complete definition of a Shaperail resource, parsed from a resource YAML file.
///
/// This is the central type that all codegen and runtime modules consume.
///
/// ```yaml
/// resource: users
/// version: 1
/// schema:
///   id: { type: uuid, primary: true, generated: true }
///   email: { type: string, format: email, unique: true, required: true }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceDefinition {
    /// Snake_case plural name of the resource (e.g., "users").
    pub resource: String,

    /// Schema version number (starts at 1).
    pub version: u32,

    /// Named database connection for this resource (M14). Default: "default".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db: Option<String>,

    /// Tenant isolation key (M18). References a schema field (must be type uuid)
    /// that identifies the tenant. When set, all queries are automatically scoped
    /// to the authenticated user's `tenant_id` claim. `super_admin` bypasses the filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_key: Option<String>,

    /// Field definitions, keyed by field name. Uses IndexMap to preserve declaration order.
    pub schema: IndexMap<String, FieldSchema>,

    /// Endpoint definitions, keyed by action name (e.g., "list", "create").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<IndexMap<String, EndpointSpec>>,

    /// Relationship definitions, keyed by relation name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relations: Option<IndexMap<String, RelationSpec>>,

    /// Additional database indexes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexes: Option<Vec<IndexSpec>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AuthRule, CacheSpec, EndpointSpec, FieldType, HttpMethod, PaginationStyle, RelationType,
    };

    fn sample_resource() -> ResourceDefinition {
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
                transient: false,
            },
        );
        schema.insert(
            "email".to_string(),
            FieldSchema {
                field_type: FieldType::String,
                primary: false,
                generated: false,
                required: true,
                unique: true,
                nullable: false,
                reference: None,
                min: None,
                max: None,
                format: Some("email".to_string()),
                values: None,
                default: None,
                sensitive: false,
                search: false,
                items: None,
                transient: false,
            },
        );

        let mut endpoints = IndexMap::new();
        endpoints.insert(
            "list".to_string(),
            EndpointSpec {
                method: Some(HttpMethod::Get),
                path: Some("/users".to_string()),
                auth: Some(AuthRule::Roles(vec![
                    "member".to_string(),
                    "admin".to_string(),
                ])),
                input: None,
                filters: Some(vec!["role".to_string()]),
                search: Some(vec!["email".to_string()]),
                pagination: Some(PaginationStyle::Cursor),
                sort: None,
                cache: Some(CacheSpec {
                    ttl: 60,
                    invalidate_on: None,
                }),
                controller: None,
                events: None,
                jobs: None,
                subscribers: None,
                handler: None,
                upload: None,
                rate_limit: None,
                soft_delete: false,
            },
        );

        let mut relations = IndexMap::new();
        relations.insert(
            "orders".to_string(),
            RelationSpec {
                resource: "orders".to_string(),
                relation_type: RelationType::HasMany,
                key: None,
                foreign_key: Some("user_id".to_string()),
            },
        );

        ResourceDefinition {
            resource: "users".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: Some(endpoints),
            relations: Some(relations),
            indexes: Some(vec![IndexSpec {
                fields: vec!["created_at".to_string()],
                unique: false,
                order: Some("desc".to_string()),
            }]),
        }
    }

    #[test]
    fn resource_definition_construction() {
        let rd = sample_resource();
        assert_eq!(rd.resource, "users");
        assert_eq!(rd.version, 1);
        assert_eq!(rd.schema.len(), 2);
        assert!(rd.schema.contains_key("id"));
        assert!(rd.schema.contains_key("email"));
    }

    #[test]
    fn resource_definition_serde_roundtrip() {
        let rd = sample_resource();
        let json = serde_json::to_string_pretty(&rd).unwrap();
        let back: ResourceDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(rd, back);
    }

    #[test]
    fn resource_definition_preserves_field_order() {
        let rd = sample_resource();
        let keys: Vec<&String> = rd.schema.keys().collect();
        assert_eq!(keys, vec!["id", "email"]);
    }

    #[test]
    fn resource_definition_optional_sections() {
        let rd = ResourceDefinition {
            resource: "tags".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema: IndexMap::new(),
            endpoints: None,
            relations: None,
            indexes: None,
        };
        assert!(rd.endpoints.is_none());
        assert!(rd.relations.is_none());
        assert!(rd.indexes.is_none());

        let json = serde_json::to_string(&rd).unwrap();
        assert!(!json.contains("endpoints"));
        assert!(!json.contains("relations"));
        assert!(!json.contains("indexes"));
    }
}
