use shaperail_core::{FieldType, HttpMethod, ResourceDefinition, WASM_HOOK_PREFIX};

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

        // Transient field constraints — `transient: true` means input-only, never persisted.
        // Combinations that imply persistence are nonsensical and rejected loudly.
        if field.transient {
            if field.primary {
                errors.push(err(&format!(
                    "resource '{res}': field '{name}' cannot be both transient and primary"
                )));
            }
            if field.generated {
                errors.push(err(&format!(
                    "resource '{res}': field '{name}' cannot be both transient and generated"
                )));
            }
            if field.reference.is_some() {
                errors.push(err(&format!(
                    "resource '{res}': field '{name}' cannot be both transient and have a ref (foreign keys imply persistence)"
                )));
            }
            if field.unique {
                errors.push(err(&format!(
                    "resource '{res}': field '{name}' cannot be both transient and unique (unique constraints require persistence)"
                )));
            }
            if field.default.is_some() {
                errors.push(err(&format!(
                    "resource '{res}': field '{name}' cannot be both transient and have a default (defaults apply to stored columns)"
                )));
            }
        }
    }

    // Transient fields must appear in at least one endpoint's `input:` list — otherwise
    // they're unreachable: never populated, never validated, never seen anywhere.
    let transient_fields: Vec<&String> = rd
        .schema
        .iter()
        .filter(|(_, f)| f.transient)
        .map(|(name, _)| name)
        .collect();
    if !transient_fields.is_empty() {
        let referenced: std::collections::HashSet<&str> = rd
            .endpoints
            .as_ref()
            .map(|eps| {
                eps.values()
                    .filter_map(|ep| ep.input.as_ref())
                    .flat_map(|inputs| inputs.iter().map(|s| s.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        for name in transient_fields {
            if !referenced.contains(name.as_str()) {
                errors.push(err(&format!(
                    "resource '{res}': transient field '{name}' is not declared in any endpoint's input: list (the field would be unreachable)"
                )));
            }
        }
    }

    // Tenant key validation (M18)
    if let Some(ref tenant_key) = rd.tenant_key {
        match rd.schema.get(tenant_key) {
            Some(field) => {
                if field.field_type != FieldType::Uuid {
                    errors.push(err(&format!(
                        "resource '{res}': tenant_key '{tenant_key}' must reference a uuid field, found {}",
                        field.field_type
                    )));
                }
            }
            None => {
                errors.push(err(&format!(
                    "resource '{res}': tenant_key '{tenant_key}' not found in schema"
                )));
            }
        }
    }

    // Endpoint validation
    if let Some(endpoints) = &rd.endpoints {
        for (action, ep) in endpoints {
            // method and path must be set (either explicitly or via convention defaults)
            if ep.method.is_none() {
                errors.push(err(&format!(
                    "resource '{res}': endpoint '{action}' has no method. Use a known action name (list, get, create, update, delete) or set method explicitly"
                )));
            }
            if ep.path.is_none() {
                errors.push(err(&format!(
                    "resource '{res}': endpoint '{action}' has no path. Use a known action name (list, get, create, update, delete) or set path explicitly"
                )));
            }

            if let Some(controller) = &ep.controller {
                // controller: is only valid on conventional CRUD endpoints.
                if let Some(e) = validate_controller_only_on_crud(res, action, ep) {
                    errors.push(e);
                }

                if let Some(before) = &controller.before {
                    if before.is_empty() {
                        errors.push(err(&format!(
                            "resource '{res}': endpoint '{action}' has an empty controller.before name"
                        )));
                    }
                    validate_controller_name(res, action, "before", before, &mut errors);
                }
                if let Some(after) = &controller.after {
                    if after.is_empty() {
                        errors.push(err(&format!(
                            "resource '{res}': endpoint '{action}' has an empty controller.after name"
                        )));
                    }
                    validate_controller_name(res, action, "after", after, &mut errors);
                }
            }

            if let Some(events) = &ep.events {
                for event in events {
                    if event.is_empty() {
                        errors.push(err(&format!(
                            "resource '{res}': endpoint '{action}' has an empty event name"
                        )));
                    }
                }
            }

            if let Some(jobs) = &ep.jobs {
                for job in jobs {
                    if job.is_empty() {
                        errors.push(err(&format!(
                            "resource '{res}': endpoint '{action}' has an empty job name"
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

            // soft_delete requires deleted_at field in schema
            if ep.soft_delete && !rd.schema.contains_key("deleted_at") {
                errors.push(err(&format!(
                    "resource '{res}': endpoint '{action}' has soft_delete but schema has no 'deleted_at' field"
                )));
            }

            if let Some(upload) = &ep.upload {
                match ep.method.as_ref() {
                    Some(HttpMethod::Post | HttpMethod::Patch | HttpMethod::Put) => {}
                    Some(_) => errors.push(err(&format!(
                        "resource '{res}': endpoint '{action}' uses upload but method must be POST, PATCH, or PUT"
                    ))),
                    None => {} // already reported above
                }

                match rd.schema.get(&upload.field) {
                    Some(field) if field.field_type == FieldType::File => {}
                    Some(_) => errors.push(err(&format!(
                        "resource '{res}': endpoint '{action}' upload field '{}' must be type file",
                        upload.field
                    ))),
                    None => errors.push(err(&format!(
                        "resource '{res}': endpoint '{action}' upload field '{}' not found in schema",
                        upload.field
                    ))),
                }

                if !matches!(upload.storage.as_str(), "local" | "s3" | "gcs" | "azure") {
                    errors.push(err(&format!(
                        "resource '{res}': endpoint '{action}' upload storage '{}' is invalid",
                        upload.storage
                    )));
                }

                if !ep
                    .input
                    .as_ref()
                    .is_some_and(|fields| fields.iter().any(|field| field == &upload.field))
                {
                    errors.push(err(&format!(
                        "resource '{res}': endpoint '{action}' upload field '{}' must appear in input",
                        upload.field
                    )));
                }

                for (suffix, expected_types) in [
                    ("filename", &[FieldType::String][..]),
                    ("mime_type", &[FieldType::String][..]),
                    ("size", &[FieldType::Integer, FieldType::Bigint][..]),
                ] {
                    let companion = format!("{}_{}", upload.field, suffix);
                    if let Some(field) = rd.schema.get(&companion) {
                        if !expected_types.contains(&field.field_type) {
                            let expected = expected_types
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(" or ");
                            errors.push(err(&format!(
                                "resource '{res}': companion upload field '{companion}' must be type {expected}"
                            )));
                        }
                    }
                }
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

/// Rejects `controller: { after: ... }` declarations on non-CRUD (custom) endpoints.
///
/// `before:` controllers are now permitted on custom endpoints — the runtime builds
/// a `Context` with auto-populated `tenant_id`, dispatches the before-hook, and
/// stashes the resulting `Context` into `req.extensions_mut()` so the custom handler
/// can read it.
///
/// `after:` controllers remain rejected on custom endpoints because the custom handler
/// owns the response shape — there is no `data:` envelope for the runtime to merge
/// `ctx.response_extras` into, and no consistent hook point after the handler returns.
fn validate_controller_only_on_crud(
    resource: &str,
    action: &str,
    endpoint: &shaperail_core::EndpointSpec,
) -> Option<ValidationError> {
    const CRUD_ACTIONS: &[&str] = &[
        "list",
        "get",
        "create",
        "update",
        "delete",
        "bulk_create",
        "bulk_delete",
    ];
    if CRUD_ACTIONS.contains(&action) {
        return None;
    }
    // Custom endpoint: allow before-only controller, reject after-controller.
    let has_after = endpoint
        .controller
        .as_ref()
        .and_then(|c| c.after.as_deref())
        .is_some();
    if has_after {
        return Some(err(&format!(
            "resource '{resource}': endpoint '{action}' declares `controller: {{ after: ... }}`, \
             but `after:` controllers are only valid on conventional CRUD endpoints \
             (list / get / create / update / delete / bulk_create / bulk_delete).\n\
             \n\
             Custom endpoints generate their own response via `handler:`, so the runtime \
             has no place to merge `ctx.response_extras` or pass through after-hook \
             mutations. Use a `before:` controller for shared setup (auth augmentation, \
             tenant scoping, request validation), and put response-shaping logic inside \
             the handler itself."
        )));
    }
    None
}

/// Validates a controller name — either a Rust function name or a `wasm:` prefixed path.
fn validate_controller_name(
    res: &str,
    action: &str,
    phase: &str,
    name: &str,
    errors: &mut Vec<ValidationError>,
) {
    if let Some(wasm_path) = name.strip_prefix(WASM_HOOK_PREFIX) {
        if wasm_path.is_empty() {
            errors.push(err(&format!(
                "resource '{res}': endpoint '{action}' controller.{phase} has 'wasm:' prefix but no path"
            )));
        } else if !wasm_path.ends_with(".wasm") {
            errors.push(err(&format!(
                "resource '{res}': endpoint '{action}' controller.{phase} WASM path must end with '.wasm', got '{wasm_path}'"
            )));
        }
    }
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
    fn soft_delete_without_deleted_at() {
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
            .contains("soft_delete but schema has no 'deleted_at'")));
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

    #[test]
    fn wasm_controller_valid_path() {
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
    input: [name]
    controller: { before: "wasm:./plugins/my_validator.wasm" }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(
            errors.is_empty(),
            "Expected no errors for valid WASM controller, got: {errors:?}"
        );
    }

    #[test]
    fn wasm_controller_missing_extension() {
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
    input: [name]
    controller: { before: "wasm:./plugins/my_validator" }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("WASM path must end with '.wasm'")));
    }

    #[test]
    fn wasm_controller_empty_path() {
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
    input: [name]
    controller: { before: "wasm:" }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("'wasm:' prefix but no path")));
    }

    #[test]
    fn upload_endpoint_valid_when_file_field_declared() {
        let yaml = r#"
resource: assets
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  file: { type: file, required: true }
  file_filename: { type: string }
  file_mime_type: { type: string }
  file_size: { type: bigint }
  updated_at: { type: timestamp, generated: true }
endpoints:
  upload:
    method: POST
    path: /assets/upload
    input: [file]
    upload:
      field: file
      storage: local
      max_size: 5mb
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(
            errors.is_empty(),
            "Expected valid upload resource, got {errors:?}"
        );
    }

    #[test]
    fn upload_endpoint_requires_file_field() {
        let yaml = r#"
resource: assets
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  file_path: { type: string, required: true }
endpoints:
  upload:
    method: POST
    path: /assets/upload
    input: [file_path]
    upload:
      field: file_path
      storage: local
      max_size: 5mb
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e
            .message
            .contains("upload field 'file_path' must be type file")));
    }

    #[test]
    fn tenant_key_valid_uuid_field() {
        let yaml = r#"
resource: projects
version: 1
tenant_key: org_id
schema:
  id: { type: uuid, primary: true, generated: true }
  org_id: { type: uuid, ref: organizations.id, required: true }
  name: { type: string, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.is_empty(), "Expected no errors, got: {errors:?}");
    }

    #[test]
    fn tenant_key_missing_field() {
        let yaml = r#"
resource: projects
version: 1
tenant_key: org_id
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e
            .message
            .contains("tenant_key 'org_id' not found in schema")));
    }

    #[test]
    fn transient_field_valid() {
        let yaml = r#"
resource: users
version: 1
schema:
  id:            { type: uuid, primary: true, generated: true }
  password:      { type: string, transient: true, min: 12, required: true }
  password_hash: { type: string, required: true }
endpoints:
  create:
    method: POST
    path: /users
    input: [password]
    controller: { before: hash_password }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.is_empty(), "Expected no errors, got: {errors:?}");
    }

    #[test]
    fn transient_field_dead_when_not_in_input() {
        let yaml = r#"
resource: users
version: 1
schema:
  id:       { type: uuid, primary: true, generated: true }
  password: { type: string, transient: true, min: 12 }
endpoints:
  create:
    method: POST
    path: /users
    input: []
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e
            .message
            .contains("transient field 'password' is not declared in any endpoint's input")));
    }

    #[test]
    fn transient_field_rejects_primary() {
        let yaml = r#"
resource: users
version: 1
schema:
  bad: { type: uuid, transient: true, primary: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("cannot be both transient and primary")));
    }

    #[test]
    fn transient_field_rejects_generated() {
        let yaml = r#"
resource: users
version: 1
schema:
  id:  { type: uuid, primary: true, generated: true }
  bad: { type: timestamp, transient: true, generated: true }
endpoints:
  create:
    method: POST
    path: /users
    input: [bad]
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("cannot be both transient and generated")));
    }

    #[test]
    fn transient_field_rejects_ref() {
        let yaml = r#"
resource: users
version: 1
schema:
  id:  { type: uuid, primary: true, generated: true }
  bad: { type: uuid, transient: true, ref: orgs.id }
endpoints:
  create:
    method: POST
    path: /users
    input: [bad]
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e
            .message
            .contains("cannot be both transient and have a ref")));
    }

    #[test]
    fn transient_field_rejects_unique() {
        let yaml = r#"
resource: users
version: 1
schema:
  id:  { type: uuid, primary: true, generated: true }
  bad: { type: string, transient: true, unique: true }
endpoints:
  create:
    method: POST
    path: /users
    input: [bad]
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors
            .iter()
            .any(|e| e.message.contains("cannot be both transient and unique")));
    }

    #[test]
    fn transient_field_rejects_default() {
        let yaml = r#"
resource: users
version: 1
schema:
  id:  { type: uuid, primary: true, generated: true }
  bad: { type: string, transient: true, default: "x" }
endpoints:
  create:
    method: POST
    path: /users
    input: [bad]
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e
            .message
            .contains("cannot be both transient and have a default")));
    }

    #[test]
    fn tenant_key_wrong_type() {
        let yaml = r#"
resource: projects
version: 1
tenant_key: org_name
schema:
  id: { type: uuid, primary: true, generated: true }
  org_name: { type: string, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e
            .message
            .contains("tenant_key 'org_name' must reference a uuid field")));
    }

    #[test]
    fn reject_after_controller_on_custom_endpoint() {
        let yaml = r#"
resource: agents
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  regenerate_secret:
    method: POST
    path: /agents/:id/regenerate_secret
    auth: [admin]
    controller: { after: my_after }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("declares `controller: { after: ... }`")),
            "expected a CustomEndpointWithAfterController error, got: {errors:?}"
        );
    }

    #[test]
    fn allow_before_controller_on_custom_endpoint() {
        let yaml = r#"
resource: agents
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  regenerate_secret:
    method: POST
    path: /agents/:id/regenerate_secret
    auth: [admin]
    controller: { before: my_before }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(
            errors.is_empty(),
            "before:-only controller on a custom endpoint should validate clean, got: {errors:?}"
        );
    }

    #[test]
    fn allow_controller_on_crud_endpoints() {
        let yaml = r#"
resource: agents
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  create:
    method: POST
    path: /agents
    input: [name]
    controller: { before: my_before }
"#;
        let rd = parse_resource(yaml).unwrap();
        let errors = validate_resource(&rd);
        assert!(
            !errors.iter().any(|e| e
                .message
                .contains("declares `controller:` but is a custom endpoint")),
            "create endpoint with controller should NOT trip the rule, got: {errors:?}"
        );
    }
}
