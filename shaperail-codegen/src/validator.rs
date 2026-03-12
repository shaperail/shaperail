use shaperail_core::{FieldType, ResourceDefinition};

/// A semantic validation error for a resource definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Validate a parsed `ResourceDefinition` for semantic correctness.
///
/// Returns a list of all validation errors found. An empty list means the
/// resource is valid.
pub fn validate_resource(rd: &ResourceDefinition) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let res = &rd.resource;

    // Resource name must not be empty
    if res.is_empty() {
        errors.push(err("resource name must not be empty"));
    }

    // Version must be >= 1
    if rd.version == 0 {
        errors.push(err(&format!("resource '{res}': version must be >= 1")));
    }

    // Schema must have at least one field
    if rd.schema.is_empty() {
        errors.push(err(&format!(
            "resource '{res}': schema must have at least one field"
        )));
    }

    // Must have exactly one primary key
    let primary_count = rd.schema.values().filter(|f| f.primary).count();
    if primary_count == 0 {
        errors.push(err(&format!(
            "resource '{res}': schema must have a primary key field"
        )));
    } else if primary_count > 1 {
        errors.push(err(&format!(
            "resource '{res}': schema must have exactly one primary key, found {primary_count}"
        )));
    }

    // Per-field validation
    for (name, field) in &rd.schema {
        // Enum type requires values
        if field.field_type == FieldType::Enum && field.values.is_none() {
            errors.push(err(&format!(
                "resource '{res}': field '{name}' is type enum but has no values"
            )));
        }

        // Non-enum type should not have values
        if field.field_type != FieldType::Enum && field.values.is_some() {
            errors.push(err(&format!(
                "resource '{res}': field '{name}' has values but is not type enum"
            )));
        }

        // Ref field must be uuid type
        if field.reference.is_some() && field.field_type != FieldType::Uuid {
            errors.push(err(&format!(
                "resource '{res}': field '{name}' has ref but is not type uuid"
            )));
        }

        // Ref format must be "resource.field"
        if let Some(ref reference) = field.reference {
            if !reference.contains('.') {
                errors.push(err(&format!(
                    "resource '{res}': field '{name}' ref must be in 'resource.field' format, got '{reference}'"
                )));
            }
        }

        // Array type requires items
        if field.field_type == FieldType::Array && field.items.is_none() {
            errors.push(err(&format!(
                "resource '{res}': field '{name}' is type array but has no items"
            )));
        }

        // Format only valid for string type
        if field.format.is_some() && field.field_type != FieldType::String {
            errors.push(err(&format!(
                "resource '{res}': field '{name}' has format but is not type string"
            )));
        }

        // Primary key should be generated or required
        if field.primary && !field.generated && !field.required {
            errors.push(err(&format!(
                "resource '{res}': primary key field '{name}' must be generated or required"
            )));
        }
    }

    // Endpoint validation
    if let Some(endpoints) = &rd.endpoints {
        for (action, ep) in endpoints {
            // Hooks must be non-empty strings
            if let Some(hooks) = &ep.hooks {
                for hook in hooks {
                    if hook.is_empty() {
                        errors.push(err(&format!(
                            "resource '{res}': endpoint '{action}' has an empty hook name"
                        )));
                    }
                }
            }

            // Input fields must exist in schema
            if let Some(input) = &ep.input {
                for field_name in input {
                    if !rd.schema.contains_key(field_name) {
                        errors.push(err(&format!(
                            "resource '{res}': endpoint '{action}' input field '{field_name}' not found in schema"
                        )));
                    }
                }
            }

            // Filter fields must exist in schema
            if let Some(filters) = &ep.filters {
                for field_name in filters {
                    if !rd.schema.contains_key(field_name) {
                        errors.push(err(&format!(
                            "resource '{res}': endpoint '{action}' filter field '{field_name}' not found in schema"
                        )));
                    }
                }
            }

            // Search fields must exist in schema
            if let Some(search) = &ep.search {
                for field_name in search {
                    if !rd.schema.contains_key(field_name) {
                        errors.push(err(&format!(
                            "resource '{res}': endpoint '{action}' search field '{field_name}' not found in schema"
                        )));
                    }
                }
            }

            // Sort fields must exist in schema
            if let Some(sort) = &ep.sort {
                for field_name in sort {
                    if !rd.schema.contains_key(field_name) {
                        errors.push(err(&format!(
                            "resource '{res}': endpoint '{action}' sort field '{field_name}' not found in schema"
                        )));
                    }
                }
            }

            // soft_delete requires updated_at field in schema
            if ep.soft_delete && !rd.schema.contains_key("updated_at") {
                errors.push(err(&format!(
                    "resource '{res}': endpoint '{action}' has soft_delete but schema has no 'updated_at' field"
                )));
            }
        }
    }

    // Relation validation
    if let Some(relations) = &rd.relations {
        for (name, rel) in relations {
            use shaperail_core::RelationType;

            // belongs_to should have key
            if rel.relation_type == RelationType::BelongsTo && rel.key.is_none() {
                errors.push(err(&format!(
                    "resource '{res}': relation '{name}' is belongs_to but has no key"
                )));
            }

            // has_many/has_one should have foreign_key
            if matches!(
                rel.relation_type,
                RelationType::HasMany | RelationType::HasOne
            ) && rel.foreign_key.is_none()
            {
                errors.push(err(&format!(
                    "resource '{res}': relation '{name}' is {} but has no foreign_key",
                    rel.relation_type
                )));
            }

            // belongs_to key must exist in schema
            if let Some(key) = &rel.key {
                if !rd.schema.contains_key(key) {
                    errors.push(err(&format!(
                        "resource '{res}': relation '{name}' key '{key}' not found in schema"
                    )));
                }
            }
        }
    }

    // Index validation
    if let Some(indexes) = &rd.indexes {
        for (i, idx) in indexes.iter().enumerate() {
            if idx.fields.is_empty() {
                errors.push(err(&format!("resource '{res}': index {i} has no fields")));
            }
            for field_name in &idx.fields {
                if !rd.schema.contains_key(field_name) {
                    errors.push(err(&format!(
                        "resource '{res}': index {i} references field '{field_name}' not in schema"
                    )));
                }
            }
            if let Some(order) = &idx.order {
                if order != "asc" && order != "desc" {
                    errors.push(err(&format!(
                        "resource '{res}': index {i} has invalid order '{order}', must be 'asc' or 'desc'"
                    )));
                }
            }
        }
    }

    errors
}

fn err(message: &str) -> ValidationError {
    ValidationError {
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_resource;

    #[test]
    fn valid_resource_passes() {
        let yaml = include_str!("../../resources/users.yaml");
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.is_empty(), "Expected no errors, got: {errors:?}");
    }

    #[test]
    fn enum_without_values() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  status: { type: enum, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("type enum but has no values")));
    }

    #[test]
    fn ref_field_not_uuid() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  org_id: { type: string, ref: organizations.id }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("has ref but is not type uuid")));
    }

    #[test]
    fn missing_primary_key() {
        let yaml = r#"
resource: items
version: 1
schema:
  name: { type: string, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("must have a primary key")));
    }

    #[test]
    fn soft_delete_without_updated_at() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  delete:
    method: DELETE
    path: /items/:id
    auth: [admin]
    soft_delete: true
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e
            .message
            .contains("soft_delete but schema has no 'updated_at'")));
    }

    #[test]
    fn input_field_not_in_schema() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  create:
    method: POST
    path: /items
    auth: [admin]
    input: [name, nonexistent]
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e
            .message
            .contains("input field 'nonexistent' not found in schema")));
    }

    #[test]
    fn belongs_to_without_key() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
relations:
  org: { resource: organizations, type: belongs_to }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("belongs_to but has no key")));
    }

    #[test]
    fn has_many_without_foreign_key() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
relations:
  orders: { resource: orders, type: has_many }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("has_many but has no foreign_key")));
    }

    #[test]
    fn index_references_missing_field() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
indexes:
  - fields: [missing_field]
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e
            .message
            .contains("references field 'missing_field' not in schema")));
    }

    #[test]
    fn error_message_format() {
        let yaml = r#"
resource: users
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  role: { type: enum }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert_eq!(
            errors[0].message,
            "resource 'users': field 'role' is type enum but has no values"
        );
    }
}
