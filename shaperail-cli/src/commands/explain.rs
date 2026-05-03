use std::path::Path;

/// Dry-run: show what a resource YAML file will produce.
/// Outputs a human/LLM-readable summary of routes, table, and relations.
pub fn run(path: &Path) -> i32 {
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

    0
}
