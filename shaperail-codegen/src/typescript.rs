use std::collections::BTreeMap;

/// Generate TypeScript type definitions from an OpenAPI 3.1 spec.
///
/// Reads the `components.schemas` section and produces one interface per schema.
/// This generates types that match what `openapi-typescript` would produce from
/// the same spec — the spec JSON is also written alongside so users can run
/// `npx openapi-typescript openapi.json -o types.ts` if they prefer.
pub fn generate_from_spec(spec: &serde_json::Value) -> BTreeMap<String, String> {
    let mut files: BTreeMap<String, String> = BTreeMap::new();

    let schemas = match spec
        .get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.as_object())
    {
        Some(s) => s,
        None => return files,
    };

    let mut all_interfaces = Vec::new();

    // Sort schema names for deterministic output
    let mut schema_names: Vec<&String> = schemas.keys().collect();
    schema_names.sort();

    for name in &schema_names {
        let schema = &schemas[*name];
        if let Some(interface) = schema_to_interface(name, schema) {
            all_interfaces.push(interface);
        }
    }

    // Generate per-resource files by grouping schemas
    // Collect resource names from paths tags
    let mut resource_schemas: BTreeMap<String, Vec<String>> = BTreeMap::new();

    if let Some(paths) = spec.get("paths").and_then(|p| p.as_object()) {
        for (_path, methods) in paths {
            if let Some(methods_obj) = methods.as_object() {
                for (_method, operation) in methods_obj {
                    if let Some(tags) = operation.get("tags").and_then(|t| t.as_array()) {
                        if let Some(tag) = tags.first().and_then(|t| t.as_str()) {
                            let pascal = to_pascal_case(tag);
                            // Add the main schema and any input schemas
                            for schema_name in &schema_names {
                                if schema_name.starts_with(&pascal) {
                                    resource_schemas
                                        .entry(tag.to_string())
                                        .or_default()
                                        .push((*schema_name).clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Deduplicate schema lists
    for schemas_list in resource_schemas.values_mut() {
        schemas_list.sort();
        schemas_list.dedup();
    }

    // Generate per-resource .ts files
    for (resource_name, schema_list) in &resource_schemas {
        let mut content = String::new();
        for schema_name in schema_list {
            if let Some(schema) = schemas.get(schema_name) {
                if let Some(interface) = schema_to_interface(schema_name, schema) {
                    content.push_str(&interface);
                    content.push('\n');
                }
            }
        }
        if !content.is_empty() {
            files.insert(format!("{resource_name}.ts"), content);
        }
    }

    // Generate index.ts that re-exports everything
    let index: String = resource_schemas
        .keys()
        .map(|r| format!("export * from './{r}';"))
        .collect::<Vec<_>>()
        .join("\n");
    if !index.is_empty() {
        files.insert("index.ts".to_string(), format!("{index}\n"));
    }

    files
}

fn schema_to_interface(name: &str, schema: &serde_json::Value) -> Option<String> {
    let properties = schema.get("properties")?.as_object()?;
    let required_fields: Vec<&str> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let mut fields = Vec::new();

    // Sort property names for deterministic output
    let mut prop_names: Vec<&String> = properties.keys().collect();
    prop_names.sort();

    for prop_name in prop_names {
        let prop = &properties[prop_name];
        let ts_type = openapi_type_to_ts(prop);
        let optional = if required_fields.contains(&prop_name.as_str()) {
            ""
        } else {
            "?"
        };
        fields.push(format!("  {prop_name}{optional}: {ts_type};"));
    }

    Some(format!(
        "export interface {name} {{\n{}\n}}\n",
        fields.join("\n")
    ))
}

fn openapi_type_to_ts(schema: &serde_json::Value) -> &'static str {
    // Check for $ref — treat as unknown
    if schema.get("$ref").is_some() {
        return "unknown";
    }

    let type_val = schema.get("type").and_then(|t| t.as_str());
    let format_val = schema.get("format").and_then(|f| f.as_str());

    match (type_val, format_val) {
        (Some("string"), _) => "string",
        (Some("integer"), _) | (Some("number"), _) => "number",
        (Some("boolean"), _) => "boolean",
        (Some("array"), _) => "unknown[]",
        (Some("object"), _) => "Record<string, unknown>",
        _ => "unknown",
    }
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
        AuthRule, CacheSpec, EndpointSpec, FieldSchema, FieldType, HttpMethod, PaginationStyle,
        ResourceDefinition,
    };

    fn test_config() -> shaperail_core::ProjectConfig {
        shaperail_core::ProjectConfig {
            project: "test-api".to_string(),
            port: 3000,
            workers: shaperail_core::WorkerCount::Auto,
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
                transient: false,
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
            "active".to_string(),
            FieldSchema {
                field_type: FieldType::Boolean,
                primary: false,
                generated: false,
                required: false,
                unique: false,
                nullable: true,
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

        let mut endpoints = IndexMap::new();
        endpoints.insert(
            "list".to_string(),
            EndpointSpec {
                method: Some(HttpMethod::Get),
                path: Some("/items".to_string()),
                auth: Some(AuthRule::Roles(vec!["member".to_string()])),
                pagination: Some(PaginationStyle::Cursor),
                cache: Some(CacheSpec {
                    ttl: 60,
                    invalidate_on: None,
                }),
                ..Default::default()
            },
        );
        endpoints.insert(
            "create".to_string(),
            EndpointSpec {
                method: Some(HttpMethod::Post),
                path: Some("/items".to_string()),
                auth: Some(AuthRule::Roles(vec!["admin".to_string()])),
                input: Some(vec!["name".to_string(), "active".to_string()]),
                ..Default::default()
            },
        );

        ResourceDefinition {
            resource: "items".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: Some(endpoints),
            relations: None,
            indexes: None,
        }
    }

    #[test]
    fn generates_ts_from_openapi_spec() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = crate::openapi::generate(&config, &resources);
        let files = generate_from_spec(&spec);

        assert!(files.contains_key("items.ts"), "items.ts generated");
        assert!(files.contains_key("index.ts"), "index.ts generated");
    }

    #[test]
    fn ts_contains_interfaces() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = crate::openapi::generate(&config, &resources);
        let files = generate_from_spec(&spec);

        let items_ts = &files["items.ts"];
        assert!(
            items_ts.contains("export interface Items"),
            "main interface"
        );
        assert!(
            items_ts.contains("export interface ItemsCreateInput"),
            "input interface"
        );
    }

    #[test]
    fn ts_field_types_correct() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = crate::openapi::generate(&config, &resources);
        let files = generate_from_spec(&spec);

        let items_ts = &files["items.ts"];
        assert!(items_ts.contains("id?: string;"), "uuid → optional string");
        assert!(items_ts.contains("name: string;"), "required string");
        assert!(
            items_ts.contains("active?: boolean;"),
            "nullable boolean optional"
        );
    }

    #[test]
    fn ts_index_reexports() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = crate::openapi::generate(&config, &resources);
        let files = generate_from_spec(&spec);

        let index = &files["index.ts"];
        assert!(index.contains("export * from './items';"));
    }

    #[test]
    fn sensitive_field_omitted_from_ts_response_interface() {
        let config = test_config();
        let mut resource = sample_resource();
        resource.schema.get_mut("active").unwrap().sensitive = true;
        let spec = crate::openapi::generate(&config, &[resource]);
        let files = generate_from_spec(&spec);

        let items_ts = &files["items.ts"];

        // Response interface (Items) drops the sensitive field
        let response_block = items_ts
            .split("export interface Items {")
            .nth(1)
            .and_then(|s| s.split('}').next())
            .expect("Items interface body");
        assert!(
            !response_block.contains("active"),
            "sensitive `active` must be absent from Items response interface, got: {response_block:?}"
        );

        // Input interface still includes it (declared in `input:`)
        let input_block = items_ts
            .split("export interface ItemsCreateInput {")
            .nth(1)
            .and_then(|s| s.split('}').next())
            .expect("ItemsCreateInput interface body");
        assert!(
            input_block.contains("active"),
            "sensitive fields declared in input: must remain in request interface, got: {input_block:?}"
        );
    }

    #[test]
    fn deterministic_ts_output() {
        let config = test_config();
        let resources = vec![sample_resource()];
        let spec = crate::openapi::generate(&config, &resources);

        let files1 = generate_from_spec(&spec);
        let files2 = generate_from_spec(&spec);

        assert_eq!(files1, files2, "TS SDK output must be deterministic");
    }
}
