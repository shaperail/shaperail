use std::path::Path;

use super::explain_format;
use crate::ExplainFormat;

/// Dry-run: show what a resource YAML file will produce.
/// Outputs a human/LLM-readable summary of routes, table, relations,
/// validations, and OpenAPI fragments.
pub fn run(path: &Path, format: ExplainFormat) -> i32 {
    let rd = match shaperail_codegen::parser::parse_resource_file(path) {
        Ok(rd) => rd,
        Err(e) => {
            eprintln!("Error parsing {}: {e}", path.display());
            return 1;
        }
    };

    let errors = shaperail_codegen::validator::validate_resource(&rd);
    if !errors.is_empty() {
        eprintln!("Validation errors in {}:", path.display());
        for err in &errors {
            eprintln!("  - {err}");
        }
        return 1;
    }

    match format {
        ExplainFormat::Text => print_text(&rd),
        ExplainFormat::Json => {
            let j = explain_format::build(&rd);
            println!("{}", serde_json::to_string_pretty(&j).expect("serialize"));
        }
    }
    0
}

fn print_text(rd: &shaperail_core::ResourceDefinition) {
    // Header
    println!("Resource: {} (v{})", rd.resource, rd.version);
    if let Some(ref db) = rd.db {
        println!("Database: {db}");
    }
    if let Some(ref tk) = rd.tenant_key {
        println!("Tenant key: {tk}");
    }
    println!();

    // Routes
    if let Some(ref endpoints) = rd.endpoints {
        println!("Routes:");
        for (action, ep) in endpoints {
            let versioned_path = format!("/v{}{}", rd.version, ep.path());
            let auth_str = match &ep.auth {
                Some(rule) => format!("{rule}"),
                None => "none".to_string(),
            };

            let mut annotations = Vec::new();
            annotations.push(format!("auth: {auth_str}"));

            if let Some(ref cache) = ep.cache {
                annotations.push(format!("cached {}s", cache.ttl));
            }
            if let Some(ref pagination) = ep.pagination {
                annotations.push(format!("{pagination:?} pagination").to_lowercase());
            }
            if let Some(ref sort) = ep.sort {
                annotations.push(format!("sort: {}", sort.join(", ")));
            }
            if let Some(ref filters) = ep.filters {
                annotations.push(format!("filters: {}", filters.join(", ")));
            }
            if let Some(ref search) = ep.search {
                annotations.push(format!("search: {}", search.join(", ")));
            }
            if let Some(ref controller) = ep.controller {
                let before_names = controller.before_names();
                if !before_names.is_empty() {
                    annotations.push(format!("before: {}", before_names.join(", ")));
                }
                let after_names = controller.after_names();
                if !after_names.is_empty() {
                    annotations.push(format!("after: {}", after_names.join(", ")));
                }
            }
            if let Some(ref events) = ep.events {
                for event in events {
                    annotations.push(format!("event: {event}"));
                }
            }
            if let Some(ref jobs) = ep.jobs {
                for job in jobs {
                    annotations.push(format!("job: {job}"));
                }
            }
            if ep.soft_delete {
                annotations.push("soft delete".into());
            }
            if ep.upload.is_some() {
                annotations.push("file upload".into());
            }

            println!(
                "  {:<8} {:<30} {} [{}]",
                ep.method(),
                versioned_path,
                action,
                annotations.join(", ")
            );
        }
        println!();
    }

    // Table schema
    let column_count = rd.schema.len();
    let has_soft_delete = rd
        .endpoints
        .as_ref()
        .map(|eps| eps.values().any(|ep| ep.soft_delete))
        .unwrap_or(false);
    let index_count = rd.indexes.as_ref().map(|i| i.len()).unwrap_or(0);

    println!(
        "Table: {} ({} columns{}{})",
        rd.resource,
        column_count + if has_soft_delete { 1 } else { 0 }, // deleted_at added for soft delete
        if index_count > 0 {
            format!(", {index_count} indexes")
        } else {
            String::new()
        },
        if has_soft_delete { ", soft delete" } else { "" }
    );

    println!("  Columns:");
    for (name, field) in &rd.schema {
        let mut attrs = vec![field.field_type.to_string()];
        if field.primary {
            attrs.push("PK".into());
        }
        if field.generated {
            attrs.push("generated".into());
        }
        if field.required {
            attrs.push("NOT NULL".into());
        }
        if field.unique {
            attrs.push("UNIQUE".into());
        }
        if field.nullable {
            attrs.push("nullable".into());
        }
        if let Some(ref reference) = field.reference {
            attrs.push(format!("FK -> {reference}"));
        }
        if let Some(ref format) = field.format {
            attrs.push(format!("format: {format}"));
        }
        if let Some(ref values) = field.values {
            attrs.push(format!("values: [{}]", values.join(", ")));
        }
        if field.sensitive {
            attrs.push("sensitive".into());
        }

        println!("    {:<20} {}", name, attrs.join(", "));
    }

    if let Some(ref indexes) = rd.indexes {
        println!("  Indexes:");
        for idx in indexes {
            let mut desc = format!("({})", idx.fields.join(", "));
            if idx.unique {
                desc.push_str(" UNIQUE");
            }
            if let Some(ref order) = idx.order {
                desc.push_str(&format!(" {}", order.to_uppercase()));
            }
            println!("    {desc}");
        }
    }
    println!();

    // Relations
    if let Some(ref relations) = rd.relations {
        println!("Relations:");
        for (name, rel) in relations {
            let detail = match rel.relation_type {
                shaperail_core::RelationType::BelongsTo => {
                    format!(
                        "belongs_to {} (key: {})",
                        rel.resource,
                        rel.key.as_deref().unwrap_or("?")
                    )
                }
                shaperail_core::RelationType::HasMany => {
                    format!(
                        "has_many {} (foreign_key: {})",
                        rel.resource,
                        rel.foreign_key.as_deref().unwrap_or("?")
                    )
                }
                shaperail_core::RelationType::HasOne => {
                    format!(
                        "has_one {} (foreign_key: {})",
                        rel.resource,
                        rel.foreign_key.as_deref().unwrap_or("?")
                    )
                }
            };
            println!("  {name}: {detail}");
        }
        println!();
    }

    // Validations
    print_validations(rd);

    // OpenAPI fragments
    print_openapi_fragments(rd);
}

// ---------------------------------------------------------------------------
// Task 8 helpers: validation summaries
// ---------------------------------------------------------------------------

fn print_validations(rd: &shaperail_core::ResourceDefinition) {
    println!("Validations:");
    for (name, field) in &rd.schema {
        let parts = compact_validation_summary(field);
        if !parts.is_empty() {
            println!("  {}: {}", name, parts.join(", "));
        }
    }
    println!();
}

/// Returns a compact list of validation descriptors for a field.
/// Order: required, unique, min=N, max=N, format=X, enum [a,b,c], default=X,
///        sensitive, transient.
pub fn compact_validation_summary(field: &shaperail_core::FieldSchema) -> Vec<String> {
    let mut parts = Vec::new();
    if field.required {
        parts.push("required".into());
    }
    if field.unique {
        parts.push("unique".into());
    }
    if let Some(ref min) = field.min {
        parts.push(format!("min={min}"));
    }
    if let Some(ref max) = field.max {
        parts.push(format!("max={max}"));
    }
    if let Some(ref fmt) = field.format {
        parts.push(format!("format={fmt}"));
    }
    if let shaperail_core::FieldType::Enum = field.field_type {
        if let Some(ref values) = field.values {
            parts.push(format!("enum [{}]", values.join(", ")));
        }
    }
    if let Some(ref default) = field.default {
        parts.push(format!("default={default}"));
    }
    if field.sensitive {
        parts.push("sensitive".into());
    }
    if field.transient {
        parts.push("transient".into());
    }
    parts
}

// ---------------------------------------------------------------------------
// Task 9 helpers: OpenAPI fragment printing
// ---------------------------------------------------------------------------

fn print_openapi_fragments(rd: &shaperail_core::ResourceDefinition) {
    println!("OpenAPI fragments:");
    if let Some(ref endpoints) = rd.endpoints {
        for (action, ep) in endpoints {
            println!("  {action}:");
            let req_schema = describe_request_shape(ep);
            if !req_schema.is_empty() {
                println!("    request: {req_schema}");
            }
            println!("    responses:");
            for (status, body) in describe_response_codes(action, ep) {
                println!("      {status}: {body}");
            }
            let auth = auth_rule_strings(ep.auth.as_ref());
            if !auth.is_empty() {
                println!("    auth: [{}]", auth.join(", "));
            }
        }
    }
    println!();
}

/// Return `{ field1, field2, ... }` for endpoints that take a body
/// (create, update, or any endpoint with an `input` list).
/// Returns an empty string for list/get/delete.
pub fn describe_request_shape(ep: &shaperail_core::EndpointSpec) -> String {
    if !endpoint_takes_body(ep) {
        return String::new();
    }
    match &ep.input {
        Some(fields) if !fields.is_empty() => format!("{{ {} }}", fields.join(", ")),
        _ => String::new(),
    }
}

/// Return `(status_code, body_summary)` pairs for every response the endpoint
/// can produce.
pub fn describe_response_codes(
    action: &str,
    ep: &shaperail_core::EndpointSpec,
) -> Vec<(u16, String)> {
    let mut out = Vec::new();
    let success_code: u16 = if action == "create" { 201 } else { 200 };
    out.push((success_code, success_body_for(action).to_string()));

    let auth = auth_rule_strings(ep.auth.as_ref());
    if !auth.is_empty() {
        out.push((401, "Unauthorized".into()));
        out.push((403, "Forbidden".into()));
    }
    if endpoint_takes_body(ep) {
        out.push((422, "Validation error (SR-coded body)".into()));
    }
    if matches!(action, "get" | "update" | "delete") {
        out.push((404, "Not Found".into()));
    }
    out
}

fn success_body_for(action: &str) -> &'static str {
    match action {
        "list" => "{ data: [<resource>], pagination: {...} }",
        "get" | "create" | "update" => "<resource>",
        "delete" => "{ ok: true }",
        _ => "<custom>",
    }
}

/// Returns true if this endpoint accepts a request body (create or update, or
/// any endpoint that declares an `input` list).
fn endpoint_takes_body(ep: &shaperail_core::EndpointSpec) -> bool {
    ep.input.as_ref().is_some_and(|i| !i.is_empty())
}

/// Convert an `AuthRule` to a `Vec<String>` of role names (or special strings
/// like `"public"` / `"owner"`).
pub fn auth_rule_strings(auth: Option<&shaperail_core::AuthRule>) -> Vec<String> {
    match auth {
        None => vec![],
        Some(shaperail_core::AuthRule::Public) => vec!["public".into()],
        Some(shaperail_core::AuthRule::Owner) => vec!["owner".into()],
        Some(shaperail_core::AuthRule::Roles(roles)) => roles.clone(),
    }
}
