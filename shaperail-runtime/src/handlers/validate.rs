use shaperail_core::{FieldError, FieldSchema, FieldType, ResourceDefinition, ShaperailError};

/// Phase 1 validation — runs BEFORE the `before:` controller.
///
/// Checks type / format / min / max / enum rules on every field PRESENT in `data`.
/// Does not check `required: true` — that's deferred to phase 2 so a controller can
/// populate fields after this runs.
pub fn validate_input_shape(
    data: &serde_json::Map<String, serde_json::Value>,
    resource: &ResourceDefinition,
) -> Result<(), ShaperailError> {
    let mut errors = Vec::new();
    for (name, field) in &resource.schema {
        if field.generated || field.primary {
            continue;
        }
        if let Some(value) = data.get(name) {
            if !value.is_null() {
                check_field_rules(name, field, value, &mut errors);
            }
        }
    }
    finalize(errors)
}

/// Phase 2 validation — runs AFTER the `before:` controller, before persistence.
///
/// Checks every required, non-primary, non-generated field without a default is
/// present in `data`. Includes transient fields — they must come from the request
/// body even though they won't be persisted. Re-runs rule checks on any keys
/// the controller injected (i.e. keys not present pre-controller).
pub fn validate_required_present(
    data: &serde_json::Map<String, serde_json::Value>,
    resource: &ResourceDefinition,
    pre_controller_keys: &std::collections::HashSet<String>,
) -> Result<(), ShaperailError> {
    let mut errors = Vec::new();
    for (name, field) in &resource.schema {
        if field.generated || field.primary {
            continue;
        }

        if field.required && field.default.is_none() && data.get(name).is_none_or(|v| v.is_null()) {
            errors.push(FieldError {
                field: name.clone(),
                message: format!("{name} is required"),
                code: "required".to_string(),
            });
            continue;
        }

        if !pre_controller_keys.contains(name) {
            if let Some(value) = data.get(name) {
                if !value.is_null() {
                    check_field_rules(name, field, value, &mut errors);
                }
            }
        }
    }
    finalize(errors)
}

/// Removes every transient field from `data` in-place. Call AFTER phase 2 and
/// BEFORE `INSERT`/`UPDATE` so the SQL generator never references a non-column.
pub fn strip_transient_fields(
    data: &mut serde_json::Map<String, serde_json::Value>,
    resource: &ResourceDefinition,
) {
    for (name, field) in &resource.schema {
        if field.transient {
            data.remove(name);
        }
    }
}

/// Combined validation for paths without a `before:` controller (and tests).
/// Equivalent to `validate_input_shape` followed by `validate_required_present`
/// with no controller-injected keys.
pub fn validate_input(
    data: &serde_json::Map<String, serde_json::Value>,
    resource: &ResourceDefinition,
) -> Result<(), ShaperailError> {
    validate_input_shape(data, resource)?;
    let keys: std::collections::HashSet<String> = data.keys().cloned().collect();
    validate_required_present(data, resource, &keys)
}

fn finalize(errors: Vec<FieldError>) -> Result<(), ShaperailError> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ShaperailError::Validation(errors))
    }
}

fn check_field_rules(
    name: &str,
    field: &FieldSchema,
    value: &serde_json::Value,
    errors: &mut Vec<FieldError>,
) {
    if field.field_type == FieldType::Enum {
        if let Some(allowed) = &field.values {
            if let Some(s) = value.as_str() {
                if !allowed.contains(&s.to_string()) {
                    errors.push(FieldError {
                        field: name.to_string(),
                        message: format!("{name} must be one of: {}", allowed.join(", ")),
                        code: "invalid_enum".to_string(),
                    });
                }
            }
        }
    }

    if field.field_type == FieldType::String || field.field_type == FieldType::Enum {
        if let Some(s) = value.as_str() {
            if let Some(min) = &field.min {
                if let Some(min_len) = min.as_u64() {
                    if (s.len() as u64) < min_len {
                        errors.push(FieldError {
                            field: name.to_string(),
                            message: format!("{name} must be at least {min_len} characters"),
                            code: "too_short".to_string(),
                        });
                    }
                }
            }
            if let Some(max) = &field.max {
                if let Some(max_len) = max.as_u64() {
                    if s.len() as u64 > max_len {
                        errors.push(FieldError {
                            field: name.to_string(),
                            message: format!("{name} must be at most {max_len} characters"),
                            code: "too_long".to_string(),
                        });
                    }
                }
            }
        }
    }

    if field.field_type == FieldType::Integer
        || field.field_type == FieldType::Bigint
        || field.field_type == FieldType::Number
    {
        if let Some(n) = value.as_f64() {
            if let Some(min) = &field.min {
                if let Some(min_val) = min.as_f64() {
                    if n < min_val {
                        errors.push(FieldError {
                            field: name.to_string(),
                            message: format!("{name} must be at least {min_val}"),
                            code: "too_small".to_string(),
                        });
                    }
                }
            }
            if let Some(max) = &field.max {
                if let Some(max_val) = max.as_f64() {
                    if n > max_val {
                        errors.push(FieldError {
                            field: name.to_string(),
                            message: format!("{name} must be at most {max_val}"),
                            code: "too_large".to_string(),
                        });
                    }
                }
            }
        }
    }

    if field.format.as_deref() == Some("email") {
        if let Some(s) = value.as_str() {
            if !s.contains('@') || !s.contains('.') {
                errors.push(FieldError {
                    field: name.to_string(),
                    message: format!("{name} must be a valid email address"),
                    code: "invalid_format".to_string(),
                });
            }
        }
    }

    if field.format.as_deref() == Some("url") {
        if let Some(s) = value.as_str() {
            if !s.starts_with("http://") && !s.starts_with("https://") {
                errors.push(FieldError {
                    field: name.to_string(),
                    message: format!("{name} must be a valid URL"),
                    code: "invalid_format".to_string(),
                });
            }
        }
    }

    if field.field_type == FieldType::Uuid {
        if let Some(s) = value.as_str() {
            if uuid::Uuid::parse_str(s).is_err() {
                errors.push(FieldError {
                    field: name.to_string(),
                    message: format!("{name} must be a valid UUID"),
                    code: "invalid_uuid".to_string(),
                });
            }
        }
    }

    if field.field_type == FieldType::Array {
        if let Some(items_spec) = &field.items {
            match value {
                serde_json::Value::Array(elements) => {
                    for (idx, element) in elements.iter().enumerate() {
                        check_item_rules(name, idx, items_spec, element, errors);
                    }
                }
                _ => {
                    errors.push(FieldError {
                        field: name.to_string(),
                        message: format!("{name} must be an array"),
                        code: "invalid_type".to_string(),
                    });
                }
            }
        }
    }
}

fn check_item_rules(
    field_name: &str,
    idx: usize,
    items: &shaperail_core::ItemsSpec,
    value: &serde_json::Value,
    errors: &mut Vec<FieldError>,
) {
    let path = format!("{field_name}[{idx}]");

    // String / enum length checks
    if matches!(items.field_type, FieldType::String | FieldType::Enum) {
        if let Some(s) = value.as_str() {
            if let Some(min) = items.min.as_ref().and_then(|v| v.as_u64()) {
                if (s.len() as u64) < min {
                    errors.push(FieldError {
                        field: path.clone(),
                        message: format!("{path} must be at least {min} characters"),
                        code: "too_short".to_string(),
                    });
                }
            }
            if let Some(max) = items.max.as_ref().and_then(|v| v.as_u64()) {
                if (s.len() as u64) > max {
                    errors.push(FieldError {
                        field: path.clone(),
                        message: format!("{path} must be at most {max} characters"),
                        code: "too_long".to_string(),
                    });
                }
            }
        }
    }

    // Numeric range checks
    if matches!(
        items.field_type,
        FieldType::Integer | FieldType::Bigint | FieldType::Number
    ) {
        if let Some(n) = value.as_f64() {
            if let Some(min) = items.min.as_ref().and_then(|v| v.as_f64()) {
                if n < min {
                    errors.push(FieldError {
                        field: path.clone(),
                        message: format!("{path} must be at least {min}"),
                        code: "too_small".to_string(),
                    });
                }
            }
            if let Some(max) = items.max.as_ref().and_then(|v| v.as_f64()) {
                if n > max {
                    errors.push(FieldError {
                        field: path.clone(),
                        message: format!("{path} must be at most {max}"),
                        code: "too_large".to_string(),
                    });
                }
            }
        }
    }

    // Enum allowlist
    if items.field_type == FieldType::Enum {
        if let (Some(allowed), Some(s)) = (&items.values, value.as_str()) {
            if !allowed.contains(&s.to_string()) {
                errors.push(FieldError {
                    field: path.clone(),
                    message: format!("{path} must be one of: {}", allowed.join(", ")),
                    code: "invalid_enum".to_string(),
                });
            }
        }
    }

    // UUID parse
    if items.field_type == FieldType::Uuid {
        if let Some(s) = value.as_str() {
            if uuid::Uuid::parse_str(s).is_err() {
                errors.push(FieldError {
                    field: path.clone(),
                    message: format!("{path} must be a valid UUID"),
                    code: "invalid_uuid".to_string(),
                });
            }
        }
    }

    // Email / URL format on string elements
    if items.format.as_deref() == Some("email") {
        if let Some(s) = value.as_str() {
            if !s.contains('@') || !s.contains('.') {
                errors.push(FieldError {
                    field: path.clone(),
                    message: format!("{path} must be a valid email address"),
                    code: "invalid_format".to_string(),
                });
            }
        }
    }
    if items.format.as_deref() == Some("url") {
        if let Some(s) = value.as_str() {
            if !s.starts_with("http://") && !s.starts_with("https://") {
                errors.push(FieldError {
                    field: path.clone(),
                    message: format!("{path} must be a valid URL"),
                    code: "invalid_format".to_string(),
                });
            }
        }
    }
}

/// Validates that every element of every `items.ref` array field exists in
/// the referenced table. Postgres-only. Runs after phase-2 validation,
/// before INSERT/UPDATE.
///
/// Issues at most one query per FK-array column per write, and only when
/// the field is present and non-empty.
pub async fn validate_item_references(
    data: &serde_json::Map<String, serde_json::Value>,
    resource: &ResourceDefinition,
    pool: &sqlx::PgPool,
) -> Result<(), ShaperailError> {
    let mut errors = Vec::new();

    for (name, field) in &resource.schema {
        let Some(items) = &field.items else { continue };
        let Some(reference) = &items.reference else {
            continue;
        };
        if items.field_type != FieldType::Uuid {
            continue;
        }

        let Some(serde_json::Value::Array(elements)) = data.get(name) else {
            continue;
        };
        if elements.is_empty() {
            continue;
        }

        let Some((table, column)) = reference.split_once('.') else {
            continue;
        };

        // Parse all elements as UUIDs (any malformed element should already have
        // been caught by phase-1 validation; defensive parse here too).
        let mut uuids: Vec<uuid::Uuid> = Vec::with_capacity(elements.len());
        for element in elements {
            if let Some(s) = element.as_str() {
                if let Ok(u) = uuid::Uuid::parse_str(s) {
                    uuids.push(u);
                }
            }
        }
        if uuids.is_empty() {
            continue;
        }

        // SAFETY: table/column come from a validated `resource.field` reference
        // string in the resource YAML, which has already been parser-checked.
        // They are NOT user input at runtime.
        let sql = format!(
            "SELECT COUNT(DISTINCT \"{column}\") FROM \"{table}\" WHERE \"{column}\" = ANY($1::uuid[])"
        );
        let row: (i64,) = sqlx::query_as(&sql)
            .bind(&uuids)
            .fetch_one(pool)
            .await
            .map_err(|e| ShaperailError::Internal(format!("items.ref check failed: {e}")))?;

        let found = row.0 as usize;
        let distinct: std::collections::HashSet<_> = uuids.iter().collect();
        if found < distinct.len() {
            // Compute missing IDs by querying once more (cheap, capped at distinct.len()).
            let sql_present = format!(
                "SELECT \"{column}\" FROM \"{table}\" WHERE \"{column}\" = ANY($1::uuid[])"
            );
            let present_rows: Vec<(uuid::Uuid,)> = sqlx::query_as(&sql_present)
                .bind(&uuids)
                .fetch_all(pool)
                .await
                .map_err(|e| ShaperailError::Internal(format!("items.ref check failed: {e}")))?;
            let present: std::collections::HashSet<uuid::Uuid> =
                present_rows.into_iter().map(|(u,)| u).collect();
            let missing: Vec<uuid::Uuid> = distinct
                .into_iter()
                .filter(|u| !present.contains(u))
                .copied()
                .take(5)
                .collect();
            errors.push(FieldError {
                field: name.clone(),
                message: format!(
                    "{name} contains references that do not exist in {reference}: {}",
                    missing
                        .iter()
                        .map(|u| u.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                code: "invalid_reference".to_string(),
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ShaperailError::Validation(errors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use shaperail_core::FieldSchema;

    fn test_resource() -> ResourceDefinition {
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
                min: Some(serde_json::json!(1)),
                max: Some(serde_json::json!(200)),
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
        schema.insert(
            "role".to_string(),
            FieldSchema {
                field_type: FieldType::Enum,
                primary: false,
                generated: false,
                required: false,
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
                transient: false,
            },
        );

        ResourceDefinition {
            resource: "users".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: None,
            relations: None,
            indexes: None,
        }
    }

    #[test]
    fn valid_input_passes() {
        let resource = test_resource();
        let mut data = serde_json::Map::new();
        data.insert("name".to_string(), serde_json::json!("Alice"));
        data.insert("email".to_string(), serde_json::json!("alice@example.com"));

        let result = validate_input(&data, &resource);
        assert!(result.is_ok());
    }

    #[test]
    fn missing_required_field() {
        let resource = test_resource();
        let mut data = serde_json::Map::new();
        data.insert("email".to_string(), serde_json::json!("alice@example.com"));
        // name is missing

        let result = validate_input(&data, &resource);
        assert!(result.is_err());
        if let Err(ShaperailError::Validation(errors)) = result {
            assert!(errors
                .iter()
                .any(|e| e.field == "name" && e.code == "required"));
        }
    }

    #[test]
    fn string_too_short() {
        let resource = test_resource();
        let mut data = serde_json::Map::new();
        data.insert("name".to_string(), serde_json::json!(""));
        data.insert("email".to_string(), serde_json::json!("alice@example.com"));

        let result = validate_input(&data, &resource);
        assert!(result.is_err());
        if let Err(ShaperailError::Validation(errors)) = result {
            assert!(errors
                .iter()
                .any(|e| e.field == "name" && e.code == "too_short"));
        }
    }

    #[test]
    fn invalid_enum_value() {
        let resource = test_resource();
        let mut data = serde_json::Map::new();
        data.insert("name".to_string(), serde_json::json!("Alice"));
        data.insert("email".to_string(), serde_json::json!("alice@example.com"));
        data.insert("role".to_string(), serde_json::json!("superuser"));

        let result = validate_input(&data, &resource);
        assert!(result.is_err());
        if let Err(ShaperailError::Validation(errors)) = result {
            assert!(errors
                .iter()
                .any(|e| e.field == "role" && e.code == "invalid_enum"));
        }
    }

    #[test]
    fn invalid_email_format() {
        let resource = test_resource();
        let mut data = serde_json::Map::new();
        data.insert("name".to_string(), serde_json::json!("Alice"));
        data.insert("email".to_string(), serde_json::json!("not-an-email"));

        let result = validate_input(&data, &resource);
        assert!(result.is_err());
        if let Err(ShaperailError::Validation(errors)) = result {
            assert!(errors
                .iter()
                .any(|e| e.field == "email" && e.code == "invalid_format"));
        }
    }

    #[test]
    fn default_skips_required_check() {
        let resource = test_resource();
        let mut data = serde_json::Map::new();
        data.insert("name".to_string(), serde_json::json!("Alice"));
        data.insert("email".to_string(), serde_json::json!("alice@example.com"));
        // role is not provided but has a default

        let result = validate_input(&data, &resource);
        assert!(result.is_ok());
    }

    fn array_resource(items: shaperail_core::ItemsSpec, required: bool) -> ResourceDefinition {
        let mut schema = indexmap::IndexMap::new();
        schema.insert(
            "tags".to_string(),
            FieldSchema {
                field_type: FieldType::Array,
                primary: false,
                generated: false,
                required,
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
                items: Some(items),
                transient: false,
            },
        );
        ResourceDefinition {
            resource: "items".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: None,
            relations: None,
            indexes: None,
        }
    }

    #[test]
    fn array_string_item_too_short() {
        let mut items = shaperail_core::ItemsSpec::of(FieldType::String);
        items.min = Some(serde_json::json!(3));
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert("tags".to_string(), serde_json::json!(["ab", "abcd"]));

        let result = validate_input(&data, &resource);
        let errors = match result {
            Err(ShaperailError::Validation(e)) => e,
            _ => panic!("expected validation error"),
        };
        assert!(errors
            .iter()
            .any(|e| e.field == "tags[0]" && e.code == "too_short"));
        assert!(errors.iter().all(|e| e.field != "tags[1]"));
    }

    #[test]
    fn array_enum_item_invalid() {
        let mut items = shaperail_core::ItemsSpec::of(FieldType::Enum);
        items.values = Some(vec!["red".to_string(), "blue".to_string()]);
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert("tags".to_string(), serde_json::json!(["red", "purple"]));

        let result = validate_input(&data, &resource);
        let errors = match result {
            Err(ShaperailError::Validation(e)) => e,
            _ => panic!("expected validation error"),
        };
        assert!(errors
            .iter()
            .any(|e| e.field == "tags[1]" && e.code == "invalid_enum"));
    }

    #[test]
    fn array_uuid_item_invalid() {
        let items = shaperail_core::ItemsSpec::of(FieldType::Uuid);
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert(
            "tags".to_string(),
            serde_json::json!(["00000000-0000-0000-0000-000000000001", "not-a-uuid"]),
        );

        let result = validate_input(&data, &resource);
        let errors = match result {
            Err(ShaperailError::Validation(e)) => e,
            _ => panic!("expected validation error"),
        };
        assert!(errors
            .iter()
            .any(|e| e.field == "tags[1]" && e.code == "invalid_uuid"));
    }

    #[test]
    fn array_integer_item_too_large() {
        let mut items = shaperail_core::ItemsSpec::of(FieldType::Integer);
        items.max = Some(serde_json::json!(10));
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert("tags".to_string(), serde_json::json!([1, 5, 100]));

        let result = validate_input(&data, &resource);
        let errors = match result {
            Err(ShaperailError::Validation(e)) => e,
            _ => panic!("expected validation error"),
        };
        assert!(errors
            .iter()
            .any(|e| e.field == "tags[2]" && e.code == "too_large"));
    }

    #[test]
    fn array_empty_passes() {
        let items = shaperail_core::ItemsSpec::of(FieldType::String);
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert("tags".to_string(), serde_json::json!([]));

        assert!(validate_input(&data, &resource).is_ok());
    }

    #[test]
    fn array_value_must_be_array() {
        let items = shaperail_core::ItemsSpec::of(FieldType::String);
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert("tags".to_string(), serde_json::json!("not-an-array"));

        let result = validate_input(&data, &resource);
        let errors = match result {
            Err(ShaperailError::Validation(e)) => e,
            _ => panic!("expected validation error"),
        };
        assert!(errors
            .iter()
            .any(|e| e.field == "tags" && e.code == "invalid_type"));
    }
}
