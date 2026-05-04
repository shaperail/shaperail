use shaperail_core::{FieldType, HttpMethod, ResourceDefinition, WASM_HOOK_PREFIX};

/// A structured diagnostic with error code, human message, fix suggestion, and example.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Diagnostic {
    /// Stable error code (e.g., "SR001").
    pub code: &'static str,
    /// Human-readable error message.
    pub error: String,
    /// Suggested fix action.
    pub fix: String,
    /// Inline YAML example showing the correct pattern.
    pub example: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.error)
    }
}

/// Validate a parsed `ResourceDefinition` and return structured diagnostics
/// with fix suggestions. This is the "AI-friendly" counterpart to `validator::validate_resource`.
pub fn diagnose_resource(rd: &ResourceDefinition) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let res = &rd.resource;

    if res.is_empty() {
        diags.push(Diagnostic {
            code: "SR001",
            error: "resource name must not be empty".into(),
            fix: "add a snake_case plural name to the 'resource' key".into(),
            example: "resource: users".into(),
        });
    }

    if rd.version == 0 {
        diags.push(Diagnostic {
            code: "SR002",
            error: format!("resource '{res}': version must be >= 1"),
            fix: "set version to 1 or higher".into(),
            example: "version: 1".into(),
        });
    }

    if rd.schema.is_empty() {
        diags.push(Diagnostic {
            code: "SR003",
            error: format!("resource '{res}': schema must have at least one field"),
            fix: "add at least an id field to the schema section".into(),
            example: "schema:\n  id: { type: uuid, primary: true, generated: true }".into(),
        });
    }

    let primary_count = rd.schema.values().filter(|f| f.primary).count();
    if primary_count == 0 && !rd.schema.is_empty() {
        diags.push(Diagnostic {
            code: "SR004",
            error: format!("resource '{res}': schema must have a primary key field"),
            fix: "add 'primary: true' to one field (typically 'id')".into(),
            example: "id: { type: uuid, primary: true, generated: true }".into(),
        });
    } else if primary_count > 1 {
        diags.push(Diagnostic {
            code: "SR005",
            error: format!(
                "resource '{res}': schema must have exactly one primary key, found {primary_count}"
            ),
            fix: "remove 'primary: true' from all fields except one".into(),
            example: "id: { type: uuid, primary: true, generated: true }".into(),
        });
    }

    for (name, field) in &rd.schema {
        if field.field_type == FieldType::Enum && field.values.is_none() {
            diags.push(Diagnostic {
                code: "SR010",
                error: format!("resource '{res}': field '{name}' is type enum but has no values"),
                fix: format!("add 'values: [value1, value2]' to the '{name}' field"),
                example: format!("{name}: {{ type: enum, values: [option_a, option_b] }}"),
            });
        }

        if field.field_type != FieldType::Enum && field.values.is_some() {
            diags.push(Diagnostic {
                code: "SR011",
                error: format!("resource '{res}': field '{name}' has values but is not type enum"),
                fix: format!("change the type to 'enum' or remove 'values' from '{name}'"),
                example: format!("{name}: {{ type: enum, values: [...] }}"),
            });
        }

        if field.reference.is_some() && field.field_type != FieldType::Uuid {
            diags.push(Diagnostic {
                code: "SR012",
                error: format!("resource '{res}': field '{name}' has ref but is not type uuid"),
                fix: format!("change the type of '{name}' to uuid"),
                example: format!(
                    "{name}: {{ type: uuid, ref: {}, required: true }}",
                    field.reference.as_deref().unwrap_or("resource.id")
                ),
            });
        }

        if let Some(ref reference) = field.reference {
            if !reference.contains('.') {
                diags.push(Diagnostic {
                    code: "SR013",
                    error: format!(
                        "resource '{res}': field '{name}' ref must be in 'resource.field' format, got '{reference}'"
                    ),
                    fix: "use 'resource_name.field_name' format for the ref value".into(),
                    example: format!("{name}: {{ type: uuid, ref: organizations.id }}"),
                });
            }
        }

        if field.field_type == FieldType::Array && field.items.is_none() {
            diags.push(Diagnostic {
                code: "SR014",
                error: format!("resource '{res}': field '{name}' is type array but has no items"),
                fix: format!("add 'items: <element_type>' to the '{name}' field"),
                example: format!("{name}: {{ type: array, items: string }}"),
            });
        }

        if let Some(items_spec) = &field.items {
            if items_spec.field_type == FieldType::Array {
                diags.push(Diagnostic {
                    code: "SR076",
                    error: format!("resource '{res}': field '{name}' has nested array items"),
                    fix: "change items to type: json (nested arrays are not supported)".to_string(),
                    example: format!("{name}: {{ type: json }}"),
                });
            }
            if items_spec.field_type == FieldType::Enum && items_spec.values.is_none() {
                diags.push(Diagnostic {
                    code: "SR077",
                    error: format!("resource '{res}': field '{name}' enum items missing values"),
                    fix: "add `values: [...]` to items".to_string(),
                    example: format!(
                        "{name}: {{ type: array, items: {{ type: enum, values: [a, b] }} }}"
                    ),
                });
            }
            if items_spec.format.is_some() && items_spec.field_type != FieldType::String {
                diags.push(Diagnostic {
                    code: "SR078",
                    error: format!(
                        "resource '{res}': field '{name}' items.format only valid on string"
                    ),
                    fix: "remove items.format or change items.type to string".to_string(),
                    example: format!(
                        "{name}: {{ type: array, items: {{ type: string, format: email }} }}"
                    ),
                });
            }
            if items_spec.reference.is_some() && items_spec.field_type != FieldType::Uuid {
                diags.push(Diagnostic {
                    code: "SR079",
                    error: format!(
                        "resource '{res}': field '{name}' items.ref requires items.type uuid"
                    ),
                    fix: "change items.type to uuid, or remove items.ref".to_string(),
                    example: format!(
                        "{name}: {{ type: array, items: {{ type: uuid, ref: organizations.id }} }}"
                    ),
                });
            }
            if let Some(reference) = &items_spec.reference {
                if !reference.contains('.') {
                    diags.push(Diagnostic {
                        code: "SR080",
                        error: format!(
                            "resource '{res}': field '{name}' items.ref must be 'resource.field'"
                        ),
                        fix: "write items.ref as 'resource_name.column_name'".to_string(),
                        example: "items: { type: uuid, ref: organizations.id }".to_string(),
                    });
                }
            }
        }

        if field.format.is_some() && field.field_type != FieldType::String {
            diags.push(Diagnostic {
                code: "SR015",
                error: format!(
                    "resource '{res}': field '{name}' has format but is not type string"
                ),
                fix: format!("change the type of '{name}' to string, or remove 'format'"),
                example: format!(
                    "{name}: {{ type: string, format: {} }}",
                    field.format.as_deref().unwrap_or("email")
                ),
            });
        }

        if field.primary && !field.generated && !field.required {
            diags.push(Diagnostic {
                code: "SR016",
                error: format!(
                    "resource '{res}': primary key field '{name}' must be generated or required"
                ),
                fix: format!("add 'generated: true' or 'required: true' to '{name}'"),
                example: format!("{name}: {{ type: uuid, primary: true, generated: true }}"),
            });
        }
    }

    // Tenant key validation
    if let Some(ref tenant_key) = rd.tenant_key {
        match rd.schema.get(tenant_key) {
            Some(field) => {
                if field.field_type != FieldType::Uuid {
                    diags.push(Diagnostic {
                        code: "SR020",
                        error: format!(
                            "resource '{res}': tenant_key '{tenant_key}' must reference a uuid field, found {}",
                            field.field_type
                        ),
                        fix: format!("change the type of '{tenant_key}' to uuid"),
                        example: format!(
                            "{tenant_key}: {{ type: uuid, ref: organizations.id, required: true }}"
                        ),
                    });
                }
            }
            None => {
                diags.push(Diagnostic {
                    code: "SR021",
                    error: format!(
                        "resource '{res}': tenant_key '{tenant_key}' not found in schema"
                    ),
                    fix: format!("add a '{tenant_key}' uuid field to the schema"),
                    example: format!(
                        "{tenant_key}: {{ type: uuid, ref: organizations.id, required: true }}"
                    ),
                });
            }
        }
    }

    // Endpoint validation
    if let Some(endpoints) = &rd.endpoints {
        for (action, ep) in endpoints {
            if let Some(controller) = &ep.controller {
                if let Some(before) = &controller.before {
                    let names = before.names();
                    if names.is_empty() {
                        diags.push(Diagnostic {
                            code: "SR063",
                            error: format!(
                                "resource '{res}': endpoint '{action}' has an empty controller.before list"
                            ),
                            fix: "remove `before:` or list at least one hook name".into(),
                            example: "controller: { before: [validate_currency, validate_org] }".into(),
                        });
                    }
                    for name in names {
                        if name.is_empty() {
                            diags.push(Diagnostic {
                                code: "SR030",
                                error: format!(
                                    "resource '{res}': endpoint '{action}' has an empty controller.before name"
                                ),
                                fix: "provide a function name for controller.before".into(),
                                example: "controller: { before: validate_input }".into(),
                            });
                            continue;
                        }
                        diagnose_controller_name(res, action, "before", name, &mut diags);
                    }
                }
                if let Some(after) = &controller.after {
                    let names = after.names();
                    if names.is_empty() {
                        diags.push(Diagnostic {
                            code: "SR063",
                            error: format!(
                                "resource '{res}': endpoint '{action}' has an empty controller.after list"
                            ),
                            fix: "remove `after:` or list at least one hook name".into(),
                            example: "controller: { after: [enrich_response, audit_log] }".into(),
                        });
                    }
                    for name in names {
                        if name.is_empty() {
                            diags.push(Diagnostic {
                                code: "SR031",
                                error: format!(
                                    "resource '{res}': endpoint '{action}' has an empty controller.after name"
                                ),
                                fix: "provide a function name for controller.after".into(),
                                example: "controller: { after: enrich_response }".into(),
                            });
                            continue;
                        }
                        diagnose_controller_name(res, action, "after", name, &mut diags);
                    }
                }
            }

            if let Some(events) = &ep.events {
                for event in events {
                    if event.is_empty() {
                        diags.push(Diagnostic {
                            code: "SR032",
                            error: format!(
                                "resource '{res}': endpoint '{action}' has an empty event name"
                            ),
                            fix: "use 'resource.action' format for event names".into(),
                            example: format!("events: [{res}.created]"),
                        });
                    }
                }
            }

            if let Some(jobs) = &ep.jobs {
                for job in jobs {
                    if job.is_empty() {
                        diags.push(Diagnostic {
                            code: "SR033",
                            error: format!(
                                "resource '{res}': endpoint '{action}' has an empty job name"
                            ),
                            fix: "provide a snake_case job name".into(),
                            example: "jobs: [send_notification]".into(),
                        });
                    }
                }
            }

            // Input/filter/search/sort fields must exist
            for (field_kind, fields) in [
                ("input", &ep.input),
                ("filter", &ep.filters),
                ("search", &ep.search),
                ("sort", &ep.sort),
            ] {
                if let Some(fields) = fields {
                    for field_name in fields {
                        if !rd.schema.contains_key(field_name) {
                            diags.push(Diagnostic {
                                code: "SR040",
                                error: format!(
                                    "resource '{res}': endpoint '{action}' {field_kind} field '{field_name}' not found in schema"
                                ),
                                fix: format!(
                                    "add '{field_name}' to the schema, or remove it from {field_kind}"
                                ),
                                example: format!("{field_name}: {{ type: string, required: true }}"),
                            });
                        }
                    }
                }
            }

            if ep.soft_delete && !rd.schema.contains_key("deleted_at") {
                diags.push(Diagnostic {
                    code: "SR041",
                    error: format!(
                        "resource '{res}': endpoint '{action}' has soft_delete but schema has no 'deleted_at' field"
                    ),
                    fix: "add 'deleted_at: { type: timestamp, nullable: true }' to the schema".into(),
                    example: "deleted_at: { type: timestamp, nullable: true }".into(),
                });
            }

            if let Some(upload) = &ep.upload {
                if !matches!(
                    *ep.method(),
                    HttpMethod::Post | HttpMethod::Patch | HttpMethod::Put
                ) {
                    diags.push(Diagnostic {
                        code: "SR050",
                        error: format!(
                            "resource '{res}': endpoint '{action}' uses upload but method must be POST, PATCH, or PUT"
                        ),
                        fix: "change the method to POST, PATCH, or PUT".into(),
                        example: "method: POST".into(),
                    });
                }

                match rd.schema.get(&upload.field) {
                    Some(field) if field.field_type == FieldType::File => {}
                    Some(_) => diags.push(Diagnostic {
                        code: "SR051",
                        error: format!(
                            "resource '{res}': endpoint '{action}' upload field '{}' must be type file",
                            upload.field
                        ),
                        fix: format!("change '{}' to type file in the schema", upload.field),
                        example: format!("{}: {{ type: file, required: true }}", upload.field),
                    }),
                    None => diags.push(Diagnostic {
                        code: "SR052",
                        error: format!(
                            "resource '{res}': endpoint '{action}' upload field '{}' not found in schema",
                            upload.field
                        ),
                        fix: format!("add '{}' as a file field in the schema", upload.field),
                        example: format!("{}: {{ type: file, required: true }}", upload.field),
                    }),
                }

                if !matches!(upload.storage.as_str(), "local" | "s3" | "gcs" | "azure") {
                    diags.push(Diagnostic {
                        code: "SR053",
                        error: format!(
                            "resource '{res}': endpoint '{action}' upload storage '{}' is invalid",
                            upload.storage
                        ),
                        fix: "use one of: local, s3, gcs, azure".into(),
                        example: "upload: { field: file, storage: s3, max_size: 5mb }".into(),
                    });
                }

                if !ep
                    .input
                    .as_ref()
                    .is_some_and(|fields| fields.iter().any(|field| field == &upload.field))
                {
                    diags.push(Diagnostic {
                        code: "SR054",
                        error: format!(
                            "resource '{res}': endpoint '{action}' upload field '{}' must appear in input",
                            upload.field
                        ),
                        fix: format!("add '{}' to the input array", upload.field),
                        example: format!("input: [{}]", upload.field),
                    });
                }
            }

            // SR073 / SR074: subscriber event and handler must not be empty
            if let Some(subs) = &ep.subscribers {
                for (i, sub) in subs.iter().enumerate() {
                    if sub.event.is_empty() {
                        diags.push(Diagnostic {
                            code: "SR073",
                            error: format!(
                                "resource '{res}': endpoint '{action}' subscriber[{i}] has an empty event pattern"
                            ),
                            fix: "provide a non-empty event pattern (e.g., \"user.created\" or \"*.deleted\")".into(),
                            example: format!(
                                "subscribers:\n  - event: {res}.created\n    handler: my_handler"
                            ),
                        });
                    }
                    if sub.handler.is_empty() {
                        diags.push(Diagnostic {
                            code: "SR074",
                            error: format!(
                                "resource '{res}': endpoint '{action}' subscriber[{i}] has an empty handler name"
                            ),
                            fix: "provide a non-empty handler name (e.g., \"send_welcome_email\")".into(),
                            example: "subscribers:\n  - event: user.created\n    handler: send_welcome_email".into(),
                        });
                    }
                }
            }

            // SR075: non-convention endpoints must declare a handler
            const CONVENTIONS: &[&str] = &["list", "get", "create", "update", "delete"];
            if !CONVENTIONS.contains(&action.as_str()) && ep.handler.is_none() {
                diags.push(Diagnostic {
                    code: "SR075",
                    error: format!(
                        "resource '{res}': endpoint '{action}' is not a standard action (list/get/create/update/delete) and has no 'handler:' declared",
                    ),
                    fix: "add a 'handler: <function_name>' field pointing to a function in resources/<resource>.controller.rs".into(),
                    example: format!(
                        "{action}:\n  method: POST\n  path: /{name}/{action}\n  auth: [admin]\n  handler: {action}_{name}",
                        action = action,
                        name = rd.resource
                    ),
                });
            }
        }
    }

    // Relation validation
    if let Some(relations) = &rd.relations {
        for (name, rel) in relations {
            use shaperail_core::RelationType;

            if rel.relation_type == RelationType::BelongsTo && rel.key.is_none() {
                diags.push(Diagnostic {
                    code: "SR060",
                    error: format!(
                        "resource '{res}': relation '{name}' is belongs_to but has no key"
                    ),
                    fix: format!("add 'key: {res}_id' to the relation (the local FK field)"),
                    example: format!(
                        "{name}: {{ resource: {}, type: belongs_to, key: {}_id }}",
                        rel.resource, rel.resource
                    ),
                });
            }

            if matches!(
                rel.relation_type,
                RelationType::HasMany | RelationType::HasOne
            ) && rel.foreign_key.is_none()
            {
                diags.push(Diagnostic {
                    code: "SR061",
                    error: format!(
                        "resource '{res}': relation '{name}' is {} but has no foreign_key",
                        rel.relation_type
                    ),
                    fix: format!(
                        "add 'foreign_key: {res}_id' to the relation (the FK on the related table)"
                    ),
                    example: format!(
                        "{name}: {{ resource: {}, type: {}, foreign_key: {res}_id }}",
                        rel.resource, rel.relation_type
                    ),
                });
            }

            if let Some(key) = &rel.key {
                if !rd.schema.contains_key(key) {
                    diags.push(Diagnostic {
                        code: "SR062",
                        error: format!(
                            "resource '{res}': relation '{name}' key '{key}' not found in schema"
                        ),
                        fix: format!("add '{key}' as a uuid field in the schema"),
                        example: format!(
                            "{key}: {{ type: uuid, ref: {}.id, required: true }}",
                            rel.resource
                        ),
                    });
                }
            }
        }
    }

    // Index validation
    if let Some(indexes) = &rd.indexes {
        for (i, idx) in indexes.iter().enumerate() {
            if idx.fields.is_empty() {
                diags.push(Diagnostic {
                    code: "SR070",
                    error: format!("resource '{res}': index {i} has no fields"),
                    fix: "add at least one field to the index".into(),
                    example: "- { fields: [field_name] }".into(),
                });
            }
            for field_name in &idx.fields {
                if !rd.schema.contains_key(field_name) {
                    diags.push(Diagnostic {
                        code: "SR071",
                        error: format!(
                            "resource '{res}': index {i} references field '{field_name}' not in schema"
                        ),
                        fix: format!("add '{field_name}' to the schema, or remove it from the index"),
                        example: format!("{field_name}: {{ type: string, required: true }}"),
                    });
                }
            }
            if let Some(order) = &idx.order {
                if order != "asc" && order != "desc" {
                    diags.push(Diagnostic {
                        code: "SR072",
                        error: format!(
                            "resource '{res}': index {i} has invalid order '{order}', must be 'asc' or 'desc'"
                        ),
                        fix: "use 'asc' or 'desc' for the index order".into(),
                        example: "- { fields: [created_at], order: desc }".into(),
                    });
                }
            }
        }
    }

    diags
}

fn diagnose_controller_name(
    res: &str,
    action: &str,
    phase: &str,
    name: &str,
    diags: &mut Vec<Diagnostic>,
) {
    if let Some(wasm_path) = name.strip_prefix(WASM_HOOK_PREFIX) {
        if wasm_path.is_empty() {
            diags.push(Diagnostic {
                code: "SR035",
                error: format!(
                    "resource '{res}': endpoint '{action}' controller.{phase} has 'wasm:' prefix but no path"
                ),
                fix: "provide a .wasm file path after the 'wasm:' prefix".into(),
                example: format!("controller: {{ {phase}: \"wasm:./plugins/validator.wasm\" }}"),
            });
        } else if !wasm_path.ends_with(".wasm") {
            diags.push(Diagnostic {
                code: "SR036",
                error: format!(
                    "resource '{res}': endpoint '{action}' controller.{phase} WASM path must end with '.wasm', got '{wasm_path}'"
                ),
                fix: "ensure the WASM plugin path ends with '.wasm'".into(),
                example: format!("controller: {{ {phase}: \"wasm:./plugins/validator.wasm\" }}"),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_resource;

    #[test]
    fn valid_resource_produces_no_diagnostics() {
        let yaml = include_str!("../../../resources/users.yaml");
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(diags.is_empty(), "Expected no diagnostics, got: {diags:?}");
    }

    #[test]
    fn enum_without_values_has_fix_suggestion() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  status: { type: enum, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        let d = diags.iter().find(|d| d.code == "SR010").unwrap();
        assert!(d.fix.contains("values"));
        assert!(d.example.contains("values:"));
    }

    #[test]
    fn missing_primary_key_has_fix_suggestion() {
        let yaml = r#"
resource: items
version: 1
schema:
  name: { type: string, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        let d = diags.iter().find(|d| d.code == "SR004").unwrap();
        assert!(d.fix.contains("primary: true"));
    }

    #[test]
    fn diagnostics_serialize_to_json() {
        let d = Diagnostic {
            code: "SR010",
            error: "field 'role' is type enum but has no values".into(),
            fix: "add values".into(),
            example: "role: { type: enum, values: [a, b] }".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("SR010"));
        assert!(json.contains("fix"));
    }

    #[test]
    fn subscriber_with_empty_event_has_fix_suggestion() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    auth: [admin]
    subscribers:
      - event: ""
        handler: my_handler
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        let d = diags.iter().find(|d| d.code == "SR073");
        assert!(
            d.is_some(),
            "Expected SR073 diagnostic for empty subscriber event"
        );
        assert!(d.unwrap().fix.contains("event"));
    }

    #[test]
    fn subscriber_with_empty_handler_has_fix_suggestion() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    auth: [admin]
    subscribers:
      - event: items.created
        handler: ""
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        let d = diags.iter().find(|d| d.code == "SR074");
        assert!(
            d.is_some(),
            "Expected SR074 diagnostic for empty subscriber handler"
        );
        assert!(d.unwrap().fix.contains("handler"));
    }

    #[test]
    fn non_convention_endpoint_without_handler_produces_sr075() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  archive:
    method: POST
    path: /items/:id/archive
    auth: [admin]
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        let d = diags.iter().find(|d| d.code == "SR075");
        assert!(
            d.is_some(),
            "Expected SR075 for non-convention endpoint missing handler"
        );
        assert!(d.unwrap().fix.contains("handler"));
    }

    #[test]
    fn non_convention_endpoint_with_handler_no_sr075() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  archive:
    method: POST
    path: /items/:id/archive
    auth: [admin]
    handler: archive_item
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        let has_sr075 = diags.iter().any(|d| d.code == "SR075");
        assert!(!has_sr075, "SR075 should not fire when handler is present");
    }

    // -- Diagnostic::Display --

    #[test]
    fn diagnostic_display_format() {
        let d = Diagnostic {
            code: "SR010",
            error: "field 'role' is type enum but has no values".into(),
            fix: "add values".into(),
            example: "role: { type: enum, values: [a, b] }".into(),
        };
        let s = d.to_string();
        assert!(s.contains("SR010"), "Expected code in display");
        assert!(s.contains("enum"), "Expected error text in display");
    }

    // -- format_feature_warnings --

    #[test]
    fn format_feature_warnings_empty_returns_empty_string() {
        use crate::feature_check::format_feature_warnings;
        let s = format_feature_warnings(&[]);
        assert!(s.is_empty());
    }

    #[test]
    fn format_feature_warnings_nonempty_includes_feature_name() {
        use crate::feature_check::{format_feature_warnings, RequiredFeature};
        let feats = vec![RequiredFeature {
            feature: "wasm-plugins",
            reason: "resource 'items' uses WASM".into(),
            enable_hint: "Add to Cargo.toml: shaperail-runtime = { features = [\"wasm-plugins\"] }"
                .into(),
        }];
        let s = format_feature_warnings(&feats);
        assert!(s.contains("wasm-plugins"));
        assert!(s.contains("WASM"));
    }

    // -- SR002: version 0 --

    #[test]
    fn sr002_version_zero() {
        let yaml = r#"
resource: items
version: 0
schema:
  id: { type: uuid, primary: true, generated: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR002"),
            "Expected SR002, got: {diags:?}"
        );
        let d = diags.iter().find(|d| d.code == "SR002").unwrap();
        assert!(d.fix.contains("version"));
    }

    // -- SR003: empty schema --

    #[test]
    fn sr003_empty_schema() {
        use indexmap::IndexMap;
        use shaperail_core::ResourceDefinition;
        let rd = ResourceDefinition {
            resource: "items".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema: IndexMap::new(),
            endpoints: None,
            relations: None,
            indexes: None,
        };
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR003"),
            "Expected SR003, got: {diags:?}"
        );
    }

    // -- SR005: multiple primary keys --

    #[test]
    fn sr005_multiple_primary_keys() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:  { type: uuid, primary: true, generated: true }
  alt: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR005"),
            "Expected SR005, got: {diags:?}"
        );
    }

    // -- SR011: non-enum with values --

    #[test]
    fn sr011_non_enum_with_values() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:   { type: uuid, primary: true, generated: true }
  name: { type: string, required: true, values: ["a", "b"] }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR011"),
            "Expected SR011, got: {diags:?}"
        );
        let d = diags.iter().find(|d| d.code == "SR011").unwrap();
        assert!(d.fix.contains("enum") || d.fix.contains("values"));
    }

    // -- SR012: ref on non-uuid field --

    #[test]
    fn sr012_ref_on_non_uuid() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:     { type: uuid, primary: true, generated: true }
  org_id: { type: string, ref: organizations.id }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR012"),
            "Expected SR012, got: {diags:?}"
        );
    }

    // -- SR013: ref missing dot --

    #[test]
    fn sr013_ref_bad_format() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:     { type: uuid, primary: true, generated: true }
  org_id: { type: uuid, ref: organizations }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR013"),
            "Expected SR013, got: {diags:?}"
        );
        let d = diags.iter().find(|d| d.code == "SR013").unwrap();
        assert!(d.fix.contains("resource_name.field_name") || d.fix.contains("format"));
    }

    // -- SR014: array without items --

    #[test]
    fn sr014_array_without_items() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:   { type: uuid, primary: true, generated: true }
  tags: { type: array }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR014"),
            "Expected SR014, got: {diags:?}"
        );
    }

    // -- SR015: format on non-string --

    #[test]
    fn sr015_format_on_non_string() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:  { type: uuid, primary: true, generated: true }
  age: { type: integer, required: true, format: email }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR015"),
            "Expected SR015, got: {diags:?}"
        );
    }

    // -- SR016: primary not generated/required --

    #[test]
    fn sr016_primary_not_generated_or_required() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR016"),
            "Expected SR016, got: {diags:?}"
        );
        let d = diags.iter().find(|d| d.code == "SR016").unwrap();
        assert!(d.fix.contains("generated: true") || d.fix.contains("required: true"));
    }

    // -- SR020: tenant_key not uuid --

    #[test]
    fn sr020_tenant_key_not_uuid() {
        let yaml = r#"
resource: items
version: 1
tenant_key: org_name
schema:
  id:       { type: uuid, primary: true, generated: true }
  org_name: { type: string, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR020"),
            "Expected SR020, got: {diags:?}"
        );
    }

    // -- SR021: tenant_key not in schema --

    #[test]
    fn sr021_tenant_key_not_in_schema() {
        let yaml = r#"
resource: items
version: 1
tenant_key: missing_field
schema:
  id: { type: uuid, primary: true, generated: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR021"),
            "Expected SR021, got: {diags:?}"
        );
        let d = diags.iter().find(|d| d.code == "SR021").unwrap();
        assert!(d.fix.contains("missing_field") || d.fix.contains("add"));
    }

    // -- SR035: wasm: prefix but empty path --

    #[test]
    fn sr035_wasm_empty_path() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:   { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  create:
    input: [name]
    controller: { before: "wasm:" }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR035"),
            "Expected SR035, got: {diags:?}"
        );
    }

    // -- SR036: wasm path does not end with .wasm --

    #[test]
    fn sr036_wasm_path_no_extension() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:   { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  create:
    input: [name]
    controller: { after: "wasm:./plugins/my_plugin" }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR036"),
            "Expected SR036, got: {diags:?}"
        );
    }

    // -- SR040: input/filter/search/sort field not in schema --

    #[test]
    fn sr040_input_field_not_in_schema() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:   { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  create:
    input: [name, ghost_field]
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR040"),
            "Expected SR040, got: {diags:?}"
        );
    }

    #[test]
    fn sr040_filter_field_not_in_schema() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:   { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  list:
    auth: public
    filters: [name, missing_filter]
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR040"),
            "Expected SR040, got: {diags:?}"
        );
    }

    // -- SR041: soft_delete without deleted_at --

    #[test]
    fn sr041_soft_delete_without_deleted_at() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:   { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  delete:
    auth: [admin]
    soft_delete: true
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR041"),
            "Expected SR041, got: {diags:?}"
        );
        let d = diags.iter().find(|d| d.code == "SR041").unwrap();
        assert!(d.fix.contains("deleted_at"));
    }

    // -- SR060: belongs_to without key --

    #[test]
    fn sr060_belongs_to_without_key() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
relations:
  org: { resource: organizations, type: belongs_to }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR060"),
            "Expected SR060, got: {diags:?}"
        );
        let d = diags.iter().find(|d| d.code == "SR060").unwrap();
        assert!(d.fix.contains("key"));
    }

    // -- SR061: has_many without foreign_key --

    #[test]
    fn sr061_has_many_without_foreign_key() {
        let yaml = r#"
resource: users
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
relations:
  orders: { resource: orders, type: has_many }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR061"),
            "Expected SR061, got: {diags:?}"
        );
        let d = diags.iter().find(|d| d.code == "SR061").unwrap();
        assert!(d.fix.contains("foreign_key"));
    }

    #[test]
    fn sr061_has_one_without_foreign_key() {
        let yaml = r#"
resource: users
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
relations:
  profile: { resource: profiles, type: has_one }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR061"),
            "Expected SR061 for has_one, got: {diags:?}"
        );
    }

    // -- SR062: relation key not in schema --

    #[test]
    fn sr062_relation_key_not_in_schema() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
relations:
  org: { resource: organizations, type: belongs_to, key: missing_fk }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR062"),
            "Expected SR062, got: {diags:?}"
        );
    }

    // -- SR070: index with no fields --

    #[test]
    fn sr070_index_no_fields() {
        use indexmap::IndexMap;
        use shaperail_core::{FieldSchema, FieldType, IndexSpec, ResourceDefinition};

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
        let rd = ResourceDefinition {
            resource: "items".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: None,
            relations: None,
            indexes: Some(vec![IndexSpec {
                fields: vec![],
                unique: false,
                order: None,
            }]),
        };
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR070"),
            "Expected SR070, got: {diags:?}"
        );
    }

    // -- SR071: index field not in schema --

    #[test]
    fn sr071_index_field_not_in_schema() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
indexes:
  - fields: [missing_field]
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR071"),
            "Expected SR071, got: {diags:?}"
        );
    }

    // -- SR072: index with invalid order --

    #[test]
    fn sr072_index_invalid_order() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:         { type: uuid, primary: true, generated: true }
  created_at: { type: timestamp, generated: true }
indexes:
  - fields: [created_at]
    order: INVALID
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR072"),
            "Expected SR072, got: {diags:?}"
        );
        let d = diags.iter().find(|d| d.code == "SR072").unwrap();
        assert!(d.fix.contains("asc") || d.fix.contains("desc"));
    }

    // ── New SR codes added in v0.11.x ─────────────────────────────────────

    #[test]
    fn sr030_empty_controller_before_name() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    input: [id]
    controller: { before: "" }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR030"),
            "Expected SR030, got: {diags:?}"
        );
    }

    #[test]
    fn sr031_empty_controller_after_name() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    input: [id]
    controller: { after: "" }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR031"),
            "Expected SR031, got: {diags:?}"
        );
    }

    #[test]
    fn sr032_empty_event_name_in_diagnostics() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    input: [id]
    events: [""]
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR032"),
            "Expected SR032, got: {diags:?}"
        );
    }

    #[test]
    fn sr033_empty_job_name_in_diagnostics() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    input: [id]
    jobs: [""]
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR033"),
            "Expected SR033, got: {diags:?}"
        );
    }

    #[test]
    fn sr050_upload_on_get_method() {
        let yaml = r#"
resource: assets
version: 1
schema:
  id:   { type: uuid, primary: true, generated: true }
  file: { type: file, required: true }
endpoints:
  upload_file:
    method: GET
    path: /assets/upload
    input: [file]
    upload:
      field: file
      storage: s3
      max_size: 5mb
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR050"),
            "Expected SR050 (upload on GET), got: {diags:?}"
        );
    }

    #[test]
    fn sr051_upload_field_wrong_type() {
        let yaml = r#"
resource: assets
version: 1
schema:
  id:    { type: uuid, primary: true, generated: true }
  title: { type: string, required: true }
endpoints:
  upload_file:
    method: POST
    path: /assets/upload
    input: [title]
    upload:
      field: title
      storage: s3
      max_size: 5mb
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR051"),
            "Expected SR051 (upload field not type file), got: {diags:?}"
        );
    }

    #[test]
    fn sr052_upload_field_missing_from_schema() {
        let yaml = r#"
resource: assets
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  upload_file:
    method: POST
    path: /assets/upload
    input: [attachment]
    upload:
      field: attachment
      storage: s3
      max_size: 5mb
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR052"),
            "Expected SR052 (upload field not in schema), got: {diags:?}"
        );
    }

    #[test]
    fn sr053_upload_invalid_storage_backend() {
        let yaml = r#"
resource: assets
version: 1
schema:
  id:   { type: uuid, primary: true, generated: true }
  file: { type: file, required: true }
endpoints:
  upload_file:
    method: POST
    path: /assets/upload
    input: [file]
    upload:
      field: file
      storage: ftp
      max_size: 5mb
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR053"),
            "Expected SR053 (invalid storage 'ftp'), got: {diags:?}"
        );
    }

    #[test]
    fn sr054_upload_field_not_in_input() {
        let yaml = r#"
resource: assets
version: 1
schema:
  id:   { type: uuid, primary: true, generated: true }
  file: { type: file, required: true }
endpoints:
  upload_file:
    method: POST
    path: /assets/upload
    input: []
    upload:
      field: file
      storage: s3
      max_size: 5mb
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR054"),
            "Expected SR054 (upload field not in input), got: {diags:?}"
        );
    }

    #[test]
    fn sr050_to_sr054_all_clear_for_valid_upload_endpoint() {
        let yaml = r#"
resource: assets
version: 1
schema:
  id:    { type: uuid, primary: true, generated: true }
  file:  { type: file, required: true }
  title: { type: string, required: true }
endpoints:
  upload_file:
    method: POST
    path: /assets/upload
    input: [file, title]
    upload:
      field: file
      storage: s3
      max_size: 10mb
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        let upload_diags: Vec<_> = diags
            .iter()
            .filter(|d| ["SR050", "SR051", "SR052", "SR053", "SR054"].contains(&d.code))
            .collect();
        assert!(
            upload_diags.is_empty(),
            "Valid upload endpoint should produce no SR050-054 diags, got: {upload_diags:?}"
        );
    }

    #[test]
    fn sr073_subscriber_empty_event() {
        let yaml = r#"
resource: notifications
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  on_user_created:
    method: POST
    path: /notifications/on_user_created
    handler: on_user_created_handler
    subscribers:
      - event: ""
        handler: send_welcome
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR073"),
            "Expected SR073, got: {diags:?}"
        );
    }

    #[test]
    fn sr074_subscriber_empty_handler() {
        let yaml = r#"
resource: notifications
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  on_user_created:
    method: POST
    path: /notifications/on_user_created
    handler: on_user_created_handler
    subscribers:
      - event: user.created
        handler: ""
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR074"),
            "Expected SR074, got: {diags:?}"
        );
    }

    // -- SR063: empty controller hook list --

    #[test]
    fn sr063_empty_controller_before_list() {
        let yaml = r#"
resource: orders
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    auth: public
    controller: { before: [] }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR063"),
            "Expected SR063, got: {diags:?}"
        );
    }

    #[test]
    fn sr063_empty_controller_after_list() {
        let yaml = r#"
resource: orders
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    auth: public
    controller: { after: [] }
"#;
        let rd = parse_resource(yaml).unwrap();
        let diags = diagnose_resource(&rd);
        assert!(
            diags.iter().any(|d| d.code == "SR063"),
            "Expected SR063, got: {diags:?}"
        );
    }
}
