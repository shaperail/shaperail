use serde_json::{json, Value};

/// Generate a JSON Schema (draft 2020-12) that validates Shaperail resource YAML files.
///
/// This schema is the canonical machine-readable definition of the resource format.
/// LLMs and IDEs can use it for autocomplete and validation. The schema is generated
/// from the same types that `parser.rs` uses, ensuring they never drift apart.
pub fn generate_resource_json_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://shaperail.dev/schema/resource.v1.json",
        "title": "Shaperail Resource Definition",
        "description": "Schema for Shaperail resource YAML files. Defines a single API resource with its fields, endpoints, relations, and indexes.",
        "type": "object",
        "required": ["resource", "version", "schema"],
        "additionalProperties": false,
        "properties": {
            "resource": {
                "type": "string",
                "description": "Snake_case plural name of the resource (e.g., 'users', 'blog_posts').",
                "pattern": "^[a-z][a-z0-9_]*$"
            },
            "version": {
                "type": "integer",
                "description": "Schema version number (starts at 1). Drives route prefix: /v{version}/...",
                "minimum": 1
            },
            "db": {
                "type": "string",
                "description": "Named database connection for this resource (M14 multi-DB). Default: 'default'."
            },
            "tenant_key": {
                "type": "string",
                "description": "Tenant isolation key (M18). References a uuid schema field that identifies the tenant. When set, all queries are automatically scoped to the authenticated user's tenant_id claim."
            },
            "schema": {
                "type": "object",
                "description": "Field definitions keyed by field name. At least one field with primary: true is required.",
                "minProperties": 1,
                "additionalProperties": { "$ref": "#/$defs/FieldSchema" }
            },
            "endpoints": {
                "type": "object",
                "description": "Endpoint definitions keyed by action name (e.g., 'list', 'get', 'create', 'update', 'delete').",
                "additionalProperties": { "$ref": "#/$defs/EndpointSpec" }
            },
            "relations": {
                "type": "object",
                "description": "Relationship definitions keyed by relation name.",
                "additionalProperties": { "$ref": "#/$defs/RelationSpec" }
            },
            "indexes": {
                "type": "array",
                "description": "Additional database indexes.",
                "items": { "$ref": "#/$defs/IndexSpec" }
            }
        },
        "$defs": {
            "FieldType": {
                "type": "string",
                "enum": ["uuid", "string", "integer", "number", "boolean", "timestamp", "date", "enum", "json", "array", "file"],
                "description": "The data type of a field."
            },
            "FieldSchema": {
                "type": "object",
                "description": "Definition of a single field in a resource schema.",
                "required": ["type"],
                "additionalProperties": false,
                "properties": {
                    "type": { "$ref": "#/$defs/FieldType" },
                    "primary": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether this field is the primary key. Exactly one field must be primary."
                    },
                    "generated": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether this field is auto-generated (e.g., uuid v4, timestamps)."
                    },
                    "required": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether this field is required (NOT NULL + validated on input)."
                    },
                    "unique": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether this field has a unique constraint."
                    },
                    "nullable": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether this field is explicitly nullable."
                    },
                    "ref": {
                        "type": "string",
                        "description": "Foreign key reference in 'resource.field' format (e.g., 'organizations.id'). Field type must be uuid.",
                        "pattern": "^[a-z][a-z0-9_]*\\.[a-z][a-z0-9_]*$"
                    },
                    "min": {
                        "description": "Minimum value (number) or minimum length (string)."
                    },
                    "max": {
                        "description": "Maximum value (number) or maximum length (string)."
                    },
                    "format": {
                        "type": "string",
                        "description": "String format validation. Only valid when type is 'string'.",
                        "enum": ["email", "url", "uuid"]
                    },
                    "values": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "description": "Allowed values for enum-type fields. Required when type is 'enum'."
                    },
                    "default": {
                        "description": "Default value for this field."
                    },
                    "sensitive": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether this field contains sensitive data (redacted in logs)."
                    },
                    "search": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether this field is included in full-text search."
                    },
                    "items": {
                        "type": "string",
                        "description": "Element type for array fields. Required when type is 'array'.",
                        "enum": ["uuid", "string", "integer", "number", "boolean", "timestamp", "date"]
                    }
                }
            },
            "HttpMethod": {
                "type": "string",
                "enum": ["GET", "POST", "PATCH", "PUT", "DELETE"]
            },
            "AuthRule": {
                "description": "Authentication rule. Use 'public' for no auth, 'owner' for ownership check, or an array of role strings.",
                "oneOf": [
                    { "const": "public" },
                    { "const": "owner" },
                    {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "description": "Requires JWT with one of these roles. Use 'owner' in the array to combine role + ownership check."
                    }
                ]
            },
            "PaginationStyle": {
                "type": "string",
                "enum": ["cursor", "offset"],
                "description": "Pagination strategy for list endpoints."
            },
            "CacheSpec": {
                "type": "object",
                "description": "Cache configuration for an endpoint.",
                "required": ["ttl"],
                "additionalProperties": false,
                "properties": {
                    "ttl": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Time-to-live in seconds."
                    },
                    "invalidate_on": {
                        "type": "array",
                        "items": { "type": "string", "enum": ["create", "update", "delete"] },
                        "description": "Events that invalidate this cache."
                    }
                }
            },
            "UploadSpec": {
                "type": "object",
                "description": "File upload configuration for an endpoint.",
                "required": ["field", "storage", "max_size"],
                "additionalProperties": false,
                "properties": {
                    "field": {
                        "type": "string",
                        "description": "Schema field that stores the file URL. Must be type 'file'."
                    },
                    "storage": {
                        "type": "string",
                        "enum": ["local", "s3", "gcs", "azure"],
                        "description": "Storage backend."
                    },
                    "max_size": {
                        "type": "string",
                        "description": "Maximum file size (e.g., '5mb', '10mb').",
                        "pattern": "^[0-9]+[kmg]b$"
                    },
                    "types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Allowed file extensions (e.g., ['jpg', 'png', 'pdf'])."
                    }
                }
            },
            "ControllerSpec": {
                "type": "object",
                "description": "Controller specification for synchronous in-request business logic. Functions live in resources/<resource>.controller.rs.",
                "additionalProperties": false,
                "properties": {
                    "before": {
                        "type": "string",
                        "description": "Function name called before the DB operation. Prefix with 'wasm:' for WASM plugins (e.g., 'wasm:./plugins/validator.wasm')."
                    },
                    "after": {
                        "type": "string",
                        "description": "Function name called after the DB operation. Prefix with 'wasm:' for WASM plugins."
                    }
                }
            },
            "EndpointSpec": {
                "type": "object",
                "description": "Specification for a single endpoint in a resource.",
                "additionalProperties": false,
                "properties": {
                    "method": { "$ref": "#/$defs/HttpMethod" },
                    "path": {
                        "type": "string",
                        "description": "URL path pattern (e.g., '/users', '/users/:id'). Auto-prefixed with /v{version}.",
                        "pattern": "^/"
                    },
                    "auth": { "$ref": "#/$defs/AuthRule" },
                    "input": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Schema fields accepted as input for create/update. Each must exist in schema."
                    },
                    "filters": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Schema fields available as query filters. Each must exist in schema."
                    },
                    "search": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Schema fields included in full-text search. Each must exist in schema."
                    },
                    "pagination": { "$ref": "#/$defs/PaginationStyle" },
                    "sort": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Schema fields available for sorting. Each must exist in schema."
                    },
                    "cache": { "$ref": "#/$defs/CacheSpec" },
                    "controller": { "$ref": "#/$defs/ControllerSpec" },
                    "events": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Events to emit after successful execution (e.g., ['user.created'])."
                    },
                    "jobs": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Background jobs to enqueue after successful execution (e.g., ['send_welcome_email'])."
                    },
                    "upload": { "$ref": "#/$defs/UploadSpec" },
                    "soft_delete": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether this endpoint performs a soft delete. Requires a 'deleted_at' nullable timestamp field in schema."
                    }
                }
            },
            "RelationType": {
                "type": "string",
                "enum": ["belongs_to", "has_many", "has_one"]
            },
            "RelationSpec": {
                "type": "object",
                "description": "Specification for a relationship to another resource.",
                "required": ["resource", "type"],
                "additionalProperties": false,
                "properties": {
                    "resource": {
                        "type": "string",
                        "description": "Name of the related resource."
                    },
                    "type": { "$ref": "#/$defs/RelationType" },
                    "key": {
                        "type": "string",
                        "description": "Local foreign key field. Required for belongs_to."
                    },
                    "foreign_key": {
                        "type": "string",
                        "description": "Foreign key on the related resource. Required for has_many and has_one."
                    }
                }
            },
            "IndexSpec": {
                "type": "object",
                "description": "Specification for a database index.",
                "required": ["fields"],
                "additionalProperties": false,
                "properties": {
                    "fields": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "description": "Fields included in this index. Each must exist in schema."
                    },
                    "unique": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether this is a unique index."
                    },
                    "order": {
                        "type": "string",
                        "enum": ["asc", "desc"],
                        "description": "Sort order for the index."
                    }
                }
            }
        }
    })
}

/// Render the JSON Schema as a pretty-printed JSON string.
pub fn render_json_schema() -> String {
    serde_json::to_string_pretty(&generate_resource_json_schema())
        .expect("JSON schema serialization cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_valid_json() {
        let schema = generate_resource_json_schema();
        assert_eq!(schema["type"], "object");
        assert!(
            schema["$defs"]["FieldType"]["enum"]
                .as_array()
                .unwrap()
                .len()
                == 11
        );
    }

    #[test]
    fn schema_requires_resource_version_schema() {
        let schema = generate_resource_json_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("resource")));
        assert!(required.contains(&json!("version")));
        assert!(required.contains(&json!("schema")));
    }

    #[test]
    fn schema_disallows_additional_properties() {
        let schema = generate_resource_json_schema();
        assert_eq!(schema["additionalProperties"], false);
    }

    #[test]
    fn render_produces_nonempty_string() {
        let rendered = render_json_schema();
        assert!(rendered.len() > 1000);
        assert!(rendered.contains("$schema"));
    }
}
