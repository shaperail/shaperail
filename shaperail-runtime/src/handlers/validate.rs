use shaperail_core::{FieldError, FieldType, ResourceDefinition, ShaperailError};

/// Validates input data against the resource schema.
///
/// Checks required fields, type correctness, min/max constraints,
/// enum values, and string format.
pub fn validate_input(
    data: &serde_json::Map<String, serde_json::Value>,
    resource: &ResourceDefinition,
) -> Result<(), ShaperailError> {
    let mut errors = Vec::new();

    for (name, field) in &resource.schema {
        // Skip generated and primary fields — they're auto-populated
        if field.generated || field.primary {
            continue;
        }

        let value = data.get(name);

        // Check required fields
        if field.required
            && field.default.is_none()
            && (value.is_none() || value.is_some_and(|v| v.is_null()))
        {
            errors.push(FieldError {
                field: name.clone(),
                message: format!("{name} is required"),
                code: "required".to_string(),
            });
            continue;
        }

        let Some(value) = value else { continue };
        if value.is_null() {
            continue;
        }

        // Validate enum values
        if field.field_type == FieldType::Enum {
            if let Some(allowed) = &field.values {
                if let Some(s) = value.as_str() {
                    if !allowed.contains(&s.to_string()) {
                        errors.push(FieldError {
                            field: name.clone(),
                            message: format!("{name} must be one of: {}", allowed.join(", ")),
                            code: "invalid_enum".to_string(),
                        });
                    }
                }
            }
        }

        // Validate string min/max length
        if field.field_type == FieldType::String || field.field_type == FieldType::Enum {
            if let Some(s) = value.as_str() {
                if let Some(min) = &field.min {
                    if let Some(min_len) = min.as_u64() {
                        if (s.len() as u64) < min_len {
                            errors.push(FieldError {
                                field: name.clone(),
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
                                field: name.clone(),
                                message: format!("{name} must be at most {max_len} characters"),
                                code: "too_long".to_string(),
                            });
                        }
                    }
                }
            }
        }

        // Validate numeric min/max
        if field.field_type == FieldType::Integer
            || field.field_type == FieldType::Bigint
            || field.field_type == FieldType::Number
        {
            if let Some(n) = value.as_f64() {
                if let Some(min) = &field.min {
                    if let Some(min_val) = min.as_f64() {
                        if n < min_val {
                            errors.push(FieldError {
                                field: name.clone(),
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
                                field: name.clone(),
                                message: format!("{name} must be at most {max_val}"),
                                code: "too_large".to_string(),
                            });
                        }
                    }
                }
            }
        }

        // Validate email format (basic check)
        if field.format.as_deref() == Some("email") {
            if let Some(s) = value.as_str() {
                if !s.contains('@') || !s.contains('.') {
                    errors.push(FieldError {
                        field: name.clone(),
                        message: format!("{name} must be a valid email address"),
                        code: "invalid_format".to_string(),
                    });
                }
            }
        }

        // Validate URL format (basic check)
        if field.format.as_deref() == Some("url") {
            if let Some(s) = value.as_str() {
                if !s.starts_with("http://") && !s.starts_with("https://") {
                    errors.push(FieldError {
                        field: name.clone(),
                        message: format!("{name} must be a valid URL"),
                        code: "invalid_format".to_string(),
                    });
                }
            }
        }

        // Validate UUID format
        if field.field_type == FieldType::Uuid {
            if let Some(s) = value.as_str() {
                if uuid::Uuid::parse_str(s).is_err() {
                    errors.push(FieldError {
                        field: name.clone(),
                        message: format!("{name} must be a valid UUID"),
                        code: "invalid_uuid".to_string(),
                    });
                }
            }
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
            },
        );

        ResourceDefinition {
            resource: "users".to_string(),
            version: 1,
            db: None,
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
}
