use std::collections::BTreeMap;

use shaperail_core::{
    EndpointSpec, FieldSchema, FieldType, HttpMethod, PaginationStyle, ProjectConfig,
    ResourceDefinition,
};

/// Generate an OpenAPI 3.1 specification from a set of resource definitions.
///
/// Uses `BTreeMap` throughout for deterministic key ordering — same input always
/// produces byte-identical output.
pub fn generate(config: &ProjectConfig, resources: &[ResourceDefinition]) -> serde_json::Value {
    let mut paths = BTreeMap::new();
    let mut schemas = BTreeMap::new();

    // Standard error schema
    schemas.insert(
        "ErrorResponse".to_string(),
        serde_json::json!({
            "type": "object",
            "properties": {
                "error": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "string" },
                        "status": { "type": "integer" },
                        "message": { "type": "string" },
                        "request_id": { "type": "string" },
                        "details": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "field": { "type": "string" },
                                    "message": { "type": "string" }
                                }
                            }
                        }
                    },
                    "required": ["code", "status", "message"]
                }
            },
            "required": ["error"]
        }),
    );

    // Sort resources by name for deterministic output
    let mut sorted_resources: Vec<&ResourceDefinition> = resources.iter().collect();
    sorted_resources.sort_by_key(|r| &r.resource);

    for resource in sorted_resources {
        let struct_name = to_pascal_case(&resource.resource);

        // Full resource schema (response)
        schemas.insert(struct_name.clone(), build_resource_schema(resource));

        // Input schemas for create/update
        if let Some(endpoints) = &resource.endpoints {
            for (action, ep) in endpoints {
                if let Some(input_fields) = &ep.input {
                    let input_name = format!("{struct_name}{}Input", to_pascal_case(action));
                    schemas.insert(
                        input_name,
                        build_input_schema(resource, input_fields, action == "create"),
                    );
                }
            }
        }

        // Generate paths from endpoints
        if let Some(endpoints) = &resource.endpoints {
            // Sort endpoints by action name for determinism
            let mut sorted_endpoints: Vec<(&String, &EndpointSpec)> = endpoints.iter().collect();
            sorted_endpoints.sort_by_key(|(name, _)| *name);

            for (action, ep) in sorted_endpoints {
                let openapi_path =
                    format!("/v{}{}", resource.version, ep.path.replace(":id", "{id}"));
                let method = ep.method.to_string().to_lowercase();

                let operation =
                    build_operation(&struct_name, resource, &resource.resource, action, ep);

                let entry = paths
                    .entry(openapi_path)
                    .or_insert_with(BTreeMap::<String, serde_json::Value>::new);
                entry.insert(method, operation);
            }
        }
    }

    // Convert BTreeMap<String, BTreeMap<String, Value>> to Value for paths
    let paths_value: serde_json::Value = serde_json::to_value(&paths)
        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));

    serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": config.project,
            "version": "1.0.0"
        },
        "paths": paths_value,
        "components": {
            "schemas": serde_json::Value::Object(
                schemas.into_iter().collect()
            ),
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT"
                },
                "apiKeyAuth": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-API-Key"
                }
            }
        }
    })
}

/// Serialize the spec to JSON with deterministic key ordering.
pub fn to_json(spec: &serde_json::Value) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(spec)
}

/// Serialize the spec to YAML with deterministic key ordering.
pub fn to_yaml(spec: &serde_json::Value) -> Result<String, serde_yaml::Error> {
    serde_yaml::to_string(spec)
}

fn build_resource_schema(resource: &ResourceDefinition) -> serde_json::Value {
    let mut properties = BTreeMap::new();
    let mut required_fields = Vec::new();

    for (name, schema) in &resource.schema {
        properties.insert(name.clone(), field_schema_to_openapi(schema));
        if schema.required && !schema.generated {
            required_fields.push(serde_json::Value::String(name.clone()));
        }
    }

    let mut result = serde_json::json!({
        "type": "object",
        "properties": serde_json::Value::Object(properties.into_iter().collect()),
    });

    if !required_fields.is_empty() {
        result["required"] = serde_json::Value::Array(required_fields);
    }

    result
}

fn build_input_schema(
    resource: &ResourceDefinition,
    input_fields: &[String],
    is_create: bool,
) -> serde_json::Value {
    let mut properties = BTreeMap::new();
    let mut required_fields = Vec::new();

    for field_name in input_fields {
        if let Some(schema) = resource.schema.get(field_name) {
            properties.insert(field_name.clone(), field_schema_to_openapi(schema));
            if is_create && schema.required {
                required_fields.push(serde_json::Value::String(field_name.clone()));
            }
        }
    }

    let mut result = serde_json::json!({
        "type": "object",
        "properties": serde_json::Value::Object(properties.into_iter().collect()),
    });

    if !required_fields.is_empty() {
        result["required"] = serde_json::Value::Array(required_fields);
    }

    result
}

fn build_multipart_input_schema(
    resource: &ResourceDefinition,
    input_fields: &[String],
    upload_field: &str,
    is_create: bool,
) -> serde_json::Value {
    let mut properties = BTreeMap::new();
    let mut required_fields = Vec::new();

    for field_name in input_fields {
        if let Some(schema) = resource.schema.get(field_name) {
            let property = if field_name == upload_field {
                serde_json::json!({
                    "type": "string",
                    "format": "binary"
                })
            } else {
                field_schema_to_openapi(schema)
            };

            properties.insert(field_name.clone(), property);
            if is_create && schema.required {
                required_fields.push(serde_json::Value::String(field_name.clone()));
            }
        }
    }

    let mut result = serde_json::json!({
        "type": "object",
        "properties": serde_json::Value::Object(properties.into_iter().collect()),
    });

    if !required_fields.is_empty() {
        result["required"] = serde_json::Value::Array(required_fields);
    }

    result
}

fn field_schema_to_openapi(schema: &FieldSchema) -> serde_json::Value {
    let mut obj = BTreeMap::new();

    match &schema.field_type {
        FieldType::Uuid => {
            obj.insert("type".to_string(), serde_json::json!("string"));
            obj.insert("format".to_string(), serde_json::json!("uuid"));
        }
        FieldType::String => {
            obj.insert("type".to_string(), serde_json::json!("string"));
        }
        FieldType::Integer => {
            obj.insert("type".to_string(), serde_json::json!("integer"));
        }
        FieldType::Bigint => {
            obj.insert("type".to_string(), serde_json::json!("integer"));
            obj.insert("format".to_string(), serde_json::json!("int64"));
        }
        FieldType::Number => {
            obj.insert("type".to_string(), serde_json::json!("number"));
        }
        FieldType::Boolean => {
            obj.insert("type".to_string(), serde_json::json!("boolean"));
        }
        FieldType::Timestamp => {
            obj.insert("type".to_string(), serde_json::json!("string"));
            obj.insert("format".to_string(), serde_json::json!("date-time"));
        }
        FieldType::Date => {
            obj.insert("type".to_string(), serde_json::json!("string"));
            obj.insert("format".to_string(), serde_json::json!("date"));
        }
        FieldType::Enum => {
            obj.insert("type".to_string(), serde_json::json!("string"));
            if let Some(values) = &schema.values {
                obj.insert("enum".to_string(), serde_json::json!(values));
            }
        }
        FieldType::Json => {
            obj.insert("type".to_string(), serde_json::json!("object"));
        }
        FieldType::Array => {
            obj.insert("type".to_string(), serde_json::json!("array"));
            obj.insert("items".to_string(), serde_json::json!({}));
        }
        FieldType::File => {
            obj.insert("type".to_string(), serde_json::json!("string"));
            obj.insert("format".to_string(), serde_json::json!("uri"));
        }
    }

    // Add format override from schema (e.g., "email")
    if let Some(format) = &schema.format {
        // Don't override format already set by type (uuid, date-time, etc.)
        if !obj.contains_key("format") {
            obj.insert("format".to_string(), serde_json::json!(format));
        }
    }

    // Add min/max constraints
    if let Some(min) = &schema.min {
        match &schema.field_type {
            FieldType::String => {
                obj.insert("minLength".to_string(), min.clone());
            }
            FieldType::Integer | FieldType::Bigint | FieldType::Number => {
                obj.insert("minimum".to_string(), min.clone());
            }
            _ => {}
        }
    }
    if let Some(max) = &schema.max {
        match &schema.field_type {
            FieldType::String => {
                obj.insert("maxLength".to_string(), max.clone());
            }
            FieldType::Integer | FieldType::Bigint | FieldType::Number => {
                obj.insert("maximum".to_string(), max.clone());
            }
            _ => {}
        }
    }

    // Add default
    if let Some(default) = &schema.default {
        obj.insert("default".to_string(), default.clone());
    }

    serde_json::Value::Object(obj.into_iter().collect())
}

fn build_operation(
    struct_name: &str,
    resource: &ResourceDefinition,
    resource_name: &str,
    action: &str,
    ep: &EndpointSpec,
) -> serde_json::Value {
    let mut operation = BTreeMap::new();

    operation.insert(
        "operationId".to_string(),
        serde_json::json!(format!("{resource_name}_{action}")),
    );
    operation.insert("tags".to_string(), serde_json::json!([resource_name]));

    // Parameters
    let mut parameters = Vec::new();

    // Path parameters
    if ep.path.contains(":id") {
        parameters.push(serde_json::json!({
            "name": "id",
            "in": "path",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
        }));
    }

    // Filter parameters
    if let Some(filters) = &ep.filters {
        for filter in filters {
            parameters.push(serde_json::json!({
                "name": format!("filter[{filter}]"),
                "in": "query",
                "required": false,
                "schema": { "type": "string" },
                "description": format!("Filter by {filter}")
            }));
        }
    }

    // Search parameter
    if let Some(search_fields) = &ep.search {
        if !search_fields.is_empty() {
            parameters.push(serde_json::json!({
                "name": "search",
                "in": "query",
                "required": false,
                "schema": { "type": "string" },
                "description": format!("Full-text search across: {}", search_fields.join(", "))
            }));
        }
    }

    // Sort parameter
    if ep.sort.is_some() || ep.pagination.is_some() {
        parameters.push(serde_json::json!({
            "name": "sort",
            "in": "query",
            "required": false,
            "schema": { "type": "string" },
            "description": "Sort fields (prefix with - for descending, e.g., -created_at,name)"
        }));
    }

    // Pagination parameters
    if let Some(pagination) = &ep.pagination {
        match pagination {
            PaginationStyle::Cursor => {
                parameters.push(serde_json::json!({
                    "name": "cursor",
                    "in": "query",
                    "required": false,
                    "schema": { "type": "string" },
                    "description": "Cursor for the next page"
                }));
                parameters.push(serde_json::json!({
                    "name": "limit",
                    "in": "query",
                    "required": false,
                    "schema": { "type": "integer", "default": 20, "minimum": 1, "maximum": 100 },
                    "description": "Number of items per page"
                }));
            }
            PaginationStyle::Offset => {
                parameters.push(serde_json::json!({
                    "name": "offset",
                    "in": "query",
                    "required": false,
                    "schema": { "type": "integer", "default": 0, "minimum": 0 },
                    "description": "Number of items to skip"
                }));
                parameters.push(serde_json::json!({
                    "name": "limit",
                    "in": "query",
                    "required": false,
                    "schema": { "type": "integer", "default": 20, "minimum": 1, "maximum": 100 },
                    "description": "Number of items per page"
                }));
            }
        }
    }

    // Field selection
    if ep.method == HttpMethod::Get {
        parameters.push(serde_json::json!({
            "name": "fields",
            "in": "query",
            "required": false,
            "schema": { "type": "string" },
            "description": "Comma-separated list of fields to include in response"
        }));
    }

    if !parameters.is_empty() {
        operation.insert(
            "parameters".to_string(),
            serde_json::Value::Array(parameters),
        );
    }

    // Request body
    if let Some(input_fields) = &ep.input {
        if !input_fields.is_empty() {
            let request_body = if let Some(upload) = &ep.upload {
                serde_json::json!({
                    "required": true,
                    "content": {
                        "multipart/form-data": {
                            "schema": build_multipart_input_schema(
                                resource,
                                input_fields,
                                &upload.field,
                                action == "create",
                            )
                        }
                    }
                })
            } else {
                let input_schema_name = format!("{struct_name}{}Input", to_pascal_case(action));
                serde_json::json!({
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": {
                                "$ref": format!("#/components/schemas/{input_schema_name}")
                            }
                        }
                    }
                })
            };

            operation.insert("requestBody".to_string(), request_body);
        }
    }

    // Responses
    let mut responses = BTreeMap::new();

    // Success response
    let success_status = match ep.method {
        HttpMethod::Post => "201",
        HttpMethod::Delete => "204",
        _ => "200",
    };

    if ep.method == HttpMethod::Delete {
        responses.insert(
            success_status.to_string(),
            serde_json::json!({ "description": "Deleted successfully" }),
        );
    } else if ep.pagination.is_some() {
        // List response with pagination meta
        responses.insert(
            success_status.to_string(),
            serde_json::json!({
                "description": "Successful response",
                "content": {
                    "application/json": {
                        "schema": {
                            "type": "object",
                            "properties": {
                                "data": {
                                    "type": "array",
                                    "items": {
                                        "$ref": format!("#/components/schemas/{struct_name}")
                                    }
                                },
                                "meta": {
                                    "type": "object",
                                    "properties": {
                                        "cursor": { "type": "string" },
                                        "has_more": { "type": "boolean" },
                                        "total": { "type": "integer" }
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        );
    } else {
        responses.insert(
            success_status.to_string(),
            serde_json::json!({
                "description": "Successful response",
                "content": {
                    "application/json": {
                        "schema": {
                            "type": "object",
                            "properties": {
                                "data": {
                                    "$ref": format!("#/components/schemas/{struct_name}")
                                }
                            }
                        }
                    }
                }
            }),
        );
    }

    // Standard error responses
    let error_ref = serde_json::json!({
        "content": {
            "application/json": {
                "schema": {
                    "$ref": "#/components/schemas/ErrorResponse"
                }
            }
        }
    });

    let mut add_error = |status: &str, description: &str| {
        let mut resp = error_ref.clone();
        resp["description"] = serde_json::json!(description);
        responses.insert(status.to_string(), resp);
    };

    add_error("401", "Unauthorized");
    add_error("403", "Forbidden");

    if ep.path.contains(":id") {
        add_error("404", "Not found");
    }

    if ep.input.is_some() {
        add_error("422", "Validation error");
    }

    add_error("429", "Rate limited");
    add_error("500", "Internal server error");

    operation.insert(
        "responses".to_string(),
        serde_json::Value::Object(responses.into_iter().collect()),
    );

    // Security
    if let Some(auth) = &ep.auth {
        if !auth.is_public() {
            operation.insert(
                "security".to_string(),
                serde_json::json!([
                    { "bearerAuth": [] },
                    { "apiKeyAuth": [] }
                ]),
            );
        }
    }

    // Vendor extensions
    if let Some(controller) = &ep.controller {
        let mut ctrl = serde_json::Map::new();
        if let Some(before) = &controller.before {
            ctrl.insert("before".to_string(), serde_json::json!(before));
        }
        if let Some(after) = &controller.after {
            ctrl.insert("after".to_string(), serde_json::json!(after));
        }
        operation.insert(
            "x-shaperail-controller".to_string(),
            serde_json::json!(ctrl),
        );
    }
    if let Some(events) = &ep.events {
        if !events.is_empty() {
            operation.insert("x-shaperail-events".to_string(), serde_json::json!(events));
        }
    }

    serde_json::Value::Object(operation.into_iter().collect())
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use shaperail_core::{
        AuthRule, CacheSpec, FieldSchema, FieldType, HttpMethod, PaginationStyle, UploadSpec,
    };

    fn test_config() -> ProjectConfig {
        ProjectConfig {
            project: "test-api".to_string(),
            port: 3000,
            workers: shaperail_core::WorkerCount::Auto,
            database: None,
            databases: None,
            cache: None,
            auth: None,
            storage: None,
            logging: None,
            events: None,
            protocols: vec!["rest".to_string()],
            graphql: None,
            grpc: None,
        }
    }

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
                search: true,
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

        let mut endpoints = IndexMap::new();
        endpoints.insert(
            "list".to_string(),
            EndpointSpec {
                method: HttpMethod::Get,
                path: "/users".to_string(),
                auth: Some(AuthRule::Roles(vec![
                    "member".to_string(),
                    "admin".to_string(),
                ])),
                input: None,
                filters: Some(vec!["role".to_string()]),
                search: Some(vec!["name".to_string(), "email".to_string()]),
                pagination: Some(PaginationStyle::Cursor),
                sort: None,
                cache: Some(CacheSpec {
                    ttl: 60,
                    invalidate_on: None,
                }),
                controller: None,
                events: None,
                jobs: None,
                upload: None,
                soft_delete: false,
            },
        );
        endpoints.insert(
            "create".to_string(),
            EndpointSpec {
                method: HttpMethod::Post,
                path: "/users".to_string(),
                auth: Some(AuthRule::Roles(vec!["admin".to_string()])),
                input: Some(vec![
                    "email".to_string(),
                    "name".to_string(),
                    "role".to_string(),
                ]),
                filters: None,
                search: None,
                pagination: None,
                sort: None,
                cache: None,
                controller: Some(shaperail_core::ControllerSpec {
                    before: Some("validate_org".to_string()),
                    after: None,
                }),
                events: Some(vec!["user.created".to_string()]),
                jobs: Some(vec!["send_welcome_email".to_string()]),
                upload: None,
                soft_delete: false,
            },
        );
        endpoints.insert(
            "update".to_string(),
            EndpointSpec {
                method: HttpMethod::Patch,
                path: "/users/:id".to_string(),
                auth: Some(AuthRule::Roles(vec![
                    "admin".to_string(),
                    "owner".to_string(),
                ])),
                input: Some(vec!["name".to_string(), "role".to_string()]),
                filters: None,
                search: None,
                pagination: None,
                sort: None,
                cache: None,
                controller: None,
                events: None,
                jobs: None,
                upload: None,
                soft_delete: false,
            },
        );
        endpoints.insert(
            "delete".to_string(),
            EndpointSpec {
                method: HttpMethod::Delete,
                path: "/users/:id".to_string(),
                auth: Some(AuthRule::Roles(vec!["admin".to_string()])),
                input: None,
                filters: None,
                search: None,
                pagination: None,
                sort: None,
                cache: None,
                controller: None,
                events: None,
                jobs: None,
                upload: None,
                soft_delete: true,
            },
        );

        ResourceDefinition {
            resource: "users".to_string(),
            version: 1,
            db: None,
            schema,
            endpoints: Some(endpoints),
            relations: None,
            indexes: None,
        }
    }

    fn upload_resource() -> ResourceDefinition {
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
            "title".to_string(),
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
                search: false,
                items: None,
            },
        );
        schema.insert(
            "attachment".to_string(),
            FieldSchema {
                field_type: FieldType::File,
                primary: false,
                generated: false,
                required: true,
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

        let mut endpoints = IndexMap::new();
        endpoints.insert(
            "create".to_string(),
            EndpointSpec {
                method: HttpMethod::Post,
                path: "/assets".to_string(),
                auth: None,
                input: Some(vec!["title".to_string(), "attachment".to_string()]),
                filters: None,
                search: None,
                pagination: None,
                sort: None,
                cache: None,
                controller: None,
                events: None,
                jobs: None,
                upload: Some(UploadSpec {
                    field: "attachment".to_string(),
                    storage: "local".to_string(),
                    max_size: "5mb".to_string(),
                    types: Some(vec!["image/png".to_string()]),
                }),
                soft_delete: false,
            },
        );

        ResourceDefinition {
            resource: "assets".to_string(),
            version: 1,
            db: None,
            schema,
            endpoints: Some(endpoints),
            relations: None,
            indexes: None,
        }
    }

    #[test]
    fn generates_valid_openapi_31_spec() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        assert_eq!(spec["openapi"], "3.1.0");
        assert_eq!(spec["info"]["title"], "test-api");
        assert_eq!(spec["info"]["version"], "1.0.0");
        assert!(spec["paths"].is_object());
        assert!(spec["components"]["schemas"].is_object());
        assert!(spec["components"]["securitySchemes"].is_object());
    }

    #[test]
    fn deterministic_output() {
        let config = test_config();
        let resources = vec![sample_resource()];

        let spec1 = generate(&config, &resources);
        let spec2 = generate(&config, &resources);

        let json1 = to_json(&spec1).expect("serialize 1");
        let json2 = to_json(&spec2).expect("serialize 2");

        assert_eq!(json1, json2, "OpenAPI spec must be deterministic");
    }

    #[test]
    fn documents_all_endpoints() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let paths = spec["paths"].as_object().expect("paths object");

        // /users should have GET and POST
        let users_path = paths.get("/v1/users").expect("/v1/users path");
        assert!(users_path.get("get").is_some(), "GET /v1/users");
        assert!(users_path.get("post").is_some(), "POST /v1/users");

        // /v1/users/{id} should have PATCH and DELETE
        let users_id_path = paths.get("/v1/users/{id}").expect("/v1/users/{{id}} path");
        assert!(users_id_path.get("patch").is_some(), "PATCH /users/{{id}}");
        assert!(
            users_id_path.get("delete").is_some(),
            "DELETE /users/{{id}}"
        );
    }

    #[test]
    fn pagination_params_documented() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let list_op = &spec["paths"]["/v1/users"]["get"];
        let params = list_op["parameters"].as_array().expect("params array");

        let param_names: Vec<&str> = params.iter().filter_map(|p| p["name"].as_str()).collect();

        assert!(param_names.contains(&"cursor"), "cursor param");
        assert!(param_names.contains(&"limit"), "limit param");
    }

    #[test]
    fn filter_params_documented() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let list_op = &spec["paths"]["/v1/users"]["get"];
        let params = list_op["parameters"].as_array().expect("params array");

        let param_names: Vec<&str> = params.iter().filter_map(|p| p["name"].as_str()).collect();

        assert!(param_names.contains(&"filter[role]"), "filter[role] param");
    }

    #[test]
    fn search_param_documented() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let list_op = &spec["paths"]["/v1/users"]["get"];
        let params = list_op["parameters"].as_array().expect("params array");

        let param_names: Vec<&str> = params.iter().filter_map(|p| p["name"].as_str()).collect();

        assert!(param_names.contains(&"search"), "search param");
    }

    #[test]
    fn standard_error_responses() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        // Check create endpoint has 401, 403, 422, 429, 500
        let create_op = &spec["paths"]["/v1/users"]["post"];
        let responses = create_op["responses"].as_object().expect("responses");

        assert!(responses.contains_key("401"), "401 Unauthorized");
        assert!(responses.contains_key("403"), "403 Forbidden");
        assert!(responses.contains_key("422"), "422 Validation error");
        assert!(responses.contains_key("429"), "429 Rate limited");
        assert!(responses.contains_key("500"), "500 Internal server error");

        // Check get (list) has 401, 403, 429, 500 but NOT 404 (no :id)
        let list_op = &spec["paths"]["/v1/users"]["get"];
        let list_responses = list_op["responses"].as_object().expect("responses");
        assert!(!list_responses.contains_key("404"), "list has no 404");

        // Check update has 404 (has :id)
        let update_op = &spec["paths"]["/v1/users/{id}"]["patch"];
        let update_responses = update_op["responses"].as_object().expect("responses");
        assert!(update_responses.contains_key("404"), "update has 404");
    }

    #[test]
    fn vendor_extensions() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let create_op = &spec["paths"]["/v1/users"]["post"];
        assert_eq!(
            create_op["x-shaperail-controller"],
            serde_json::json!({"before": "validate_org"})
        );
        assert_eq!(
            create_op["x-shaperail-events"],
            serde_json::json!(["user.created"])
        );
    }

    #[test]
    fn enum_values_in_schema() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let role_prop = &spec["components"]["schemas"]["Users"]["properties"]["role"];
        assert_eq!(
            role_prop["enum"],
            serde_json::json!(["admin", "member", "viewer"])
        );
        assert_eq!(role_prop["default"], serde_json::json!("member"));
    }

    #[test]
    fn input_schemas_generated() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let schemas = spec["components"]["schemas"].as_object().expect("schemas");
        assert!(
            schemas.contains_key("UsersCreateInput"),
            "create input schema"
        );
        assert!(
            schemas.contains_key("UsersUpdateInput"),
            "update input schema"
        );
    }

    #[test]
    fn request_body_references_input_schema() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let create_op = &spec["paths"]["/v1/users"]["post"];
        let schema_ref = &create_op["requestBody"]["content"]["application/json"]["schema"]["$ref"];
        assert_eq!(schema_ref, "#/components/schemas/UsersCreateInput");
    }

    #[test]
    fn upload_request_body_uses_multipart_form_data() {
        let config = test_config();
        let resources = vec![upload_resource()];
        let spec = generate(&config, &resources);

        let create_op = &spec["paths"]["/v1/assets"]["post"];
        let schema = &create_op["requestBody"]["content"]["multipart/form-data"]["schema"];

        assert_eq!(schema["properties"]["attachment"]["type"], "string");
        assert_eq!(schema["properties"]["attachment"]["format"], "binary");
        assert_eq!(schema["properties"]["title"]["type"], "string");
    }

    #[test]
    fn security_on_authenticated_endpoints() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let list_op = &spec["paths"]["/v1/users"]["get"];
        assert!(
            list_op["security"].is_array(),
            "auth endpoints have security"
        );
    }

    #[test]
    fn string_constraints_in_schema() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let name_prop = &spec["components"]["schemas"]["Users"]["properties"]["name"];
        assert_eq!(name_prop["minLength"], 1);
        assert_eq!(name_prop["maxLength"], 200);
    }

    #[test]
    fn json_and_yaml_output() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let json = to_json(&spec).expect("json");
        assert!(json.contains("\"openapi\": \"3.1.0\""));

        let yaml = to_yaml(&spec).expect("yaml");
        assert!(yaml.contains("openapi: 3.1.0"));
    }

    #[test]
    fn delete_returns_204() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let delete_op = &spec["paths"]["/v1/users/{id}"]["delete"];
        let responses = delete_op["responses"].as_object().expect("responses");
        assert!(responses.contains_key("204"), "delete returns 204");
    }

    #[test]
    fn list_response_envelope() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let list_resp = &spec["paths"]["/v1/users"]["get"]["responses"]["200"]["content"]
            ["application/json"]["schema"];
        assert!(list_resp["properties"]["data"]["type"] == "array");
        assert!(list_resp["properties"]["meta"]["type"] == "object");
    }

    #[test]
    fn error_response_schema_exists() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = generate(&config, &resources);

        let schemas = spec["components"]["schemas"].as_object().expect("schemas");
        assert!(schemas.contains_key("ErrorResponse"));

        let err = &schemas["ErrorResponse"];
        assert!(err["properties"]["error"]["properties"]["code"].is_object());
        assert!(err["properties"]["error"]["properties"]["status"].is_object());
        assert!(err["properties"]["error"]["properties"]["message"].is_object());
    }
}
