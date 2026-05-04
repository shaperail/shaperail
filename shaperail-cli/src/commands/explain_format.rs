//! Stable JSON shape for `shaperail explain --format json`. This shape
//! is documented in docs/cli-reference.md and is part of the CLI contract:
//! breaking changes require a major bump.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct ExplainJson {
    pub resource: String,
    pub version: u32,
    pub db: Option<String>,
    pub tenant_key: Option<String>,
    pub routes: Vec<RouteJson>,
    pub table: TableJson,
    pub relations: Vec<RelationJson>,
    pub validations: BTreeMap<String, Vec<String>>,
    pub openapi: BTreeMap<String, OpenApiFragmentJson>,
    pub indexes: Vec<IndexJson>,
}

#[derive(Serialize)]
pub struct RouteJson {
    pub method: String,
    pub path: String,
    pub action: String,
    pub auth: Vec<String>,
    pub filters: Vec<String>,
    pub search: Vec<String>,
    pub sort: Vec<String>,
    pub pagination: Option<String>,
    pub cache_ttl_seconds: Option<u64>,
    pub rate_limit: Option<String>,
    pub soft_delete: bool,
    pub upload: bool,
    pub controller: Option<ControllerJson>,
    pub events: Vec<String>,
    pub jobs: Vec<String>,
}

#[derive(Serialize)]
pub struct ControllerJson {
    pub before: Vec<String>,
    pub after: Vec<String>,
}

#[derive(Serialize)]
pub struct TableJson {
    pub name: String,
    pub columns: Vec<ColumnJson>,
}

#[derive(Serialize)]
pub struct ColumnJson {
    pub name: String,
    pub r#type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub generated: bool,
    pub references: Option<String>,
    pub default: Option<String>,
    pub sensitive: bool,
}

#[derive(Serialize)]
pub struct RelationJson {
    pub name: String,
    pub r#type: String,
    pub resource: String,
    pub key: Option<String>,
    pub foreign_key: Option<String>,
}

#[derive(Serialize)]
pub struct OpenApiFragmentJson {
    pub request: Option<serde_json::Value>,
    pub responses: BTreeMap<u16, String>,
    pub auth: Vec<String>,
}

#[derive(Serialize)]
pub struct IndexJson {
    pub fields: Vec<String>,
    pub order: Option<String>,
    pub unique: bool,
}

/// Build the stable JSON representation of a resource definition.
/// Reuses the same logic as the text printer helpers.
pub fn build(rd: &shaperail_core::ResourceDefinition) -> ExplainJson {
    use crate::commands::explain::{
        auth_rule_strings, compact_validation_summary, describe_request_shape,
        describe_response_codes,
    };

    // Routes
    let routes = rd
        .endpoints
        .as_ref()
        .map(|eps| {
            eps.iter()
                .map(|(action, ep)| {
                    let versioned_path = format!("/v{}{}", rd.version, ep.path());
                    let auth = auth_rule_strings(ep.auth.as_ref());
                    let controller = ep.controller.as_ref().map(|c| ControllerJson {
                        before: c.before_names().to_vec(),
                        after: c.after_names().to_vec(),
                    });
                    RouteJson {
                        method: ep.method().to_string(),
                        path: versioned_path,
                        action: action.clone(),
                        auth,
                        filters: ep.filters.as_deref().unwrap_or(&[]).to_vec(),
                        search: ep.search.as_deref().unwrap_or(&[]).to_vec(),
                        sort: ep.sort.as_deref().unwrap_or(&[]).to_vec(),
                        pagination: ep
                            .pagination
                            .as_ref()
                            .map(|p| format!("{p:?}").to_lowercase()),
                        cache_ttl_seconds: ep.cache.as_ref().map(|c| c.ttl),
                        rate_limit: ep
                            .rate_limit
                            .as_ref()
                            .map(|rl| format!("{} req/{} s", rl.max_requests, rl.window_secs)),
                        soft_delete: ep.soft_delete,
                        upload: ep.upload.is_some(),
                        controller,
                        events: ep.events.as_deref().unwrap_or(&[]).to_vec(),
                        jobs: ep.jobs.as_deref().unwrap_or(&[]).to_vec(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // Table columns
    let has_soft_delete = rd
        .endpoints
        .as_ref()
        .map(|eps| eps.values().any(|ep| ep.soft_delete))
        .unwrap_or(false);

    let mut columns: Vec<ColumnJson> = rd
        .schema
        .iter()
        .map(|(name, field)| ColumnJson {
            name: name.clone(),
            r#type: field.field_type.to_string(),
            nullable: field.nullable,
            primary_key: field.primary,
            unique: field.unique,
            generated: field.generated,
            references: field.reference.clone(),
            default: field.default.as_ref().map(|v| v.to_string()),
            sensitive: field.sensitive,
        })
        .collect();

    if has_soft_delete && !rd.schema.contains_key("deleted_at") {
        columns.push(ColumnJson {
            name: "deleted_at".to_string(),
            r#type: "timestamp".to_string(),
            nullable: true,
            primary_key: false,
            unique: false,
            generated: false,
            references: None,
            default: None,
            sensitive: false,
        });
    }

    let table = TableJson {
        name: rd.resource.clone(),
        columns,
    };

    // Relations
    let relations = rd
        .relations
        .as_ref()
        .map(|rels| {
            rels.iter()
                .map(|(name, rel)| RelationJson {
                    name: name.clone(),
                    r#type: rel.relation_type.to_string(),
                    resource: rel.resource.clone(),
                    key: rel.key.clone(),
                    foreign_key: rel.foreign_key.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    // Validations — reuse compact_validation_summary
    let validations: BTreeMap<String, Vec<String>> = rd
        .schema
        .iter()
        .filter_map(|(name, field)| {
            let parts = compact_validation_summary(field);
            if parts.is_empty() {
                None
            } else {
                Some((name.clone(), parts))
            }
        })
        .collect();

    // OpenAPI fragments — reuse describe_request_shape / describe_response_codes
    let openapi: BTreeMap<String, OpenApiFragmentJson> = rd
        .endpoints
        .as_ref()
        .map(|eps| {
            eps.iter()
                .map(|(action, ep)| {
                    let req_str = describe_request_shape(ep);
                    let request = if req_str.is_empty() {
                        None
                    } else {
                        Some(serde_json::Value::String(req_str))
                    };
                    let mut responses = BTreeMap::new();
                    for (code, body) in describe_response_codes(action, ep) {
                        responses.insert(code, body);
                    }
                    let auth = auth_rule_strings(ep.auth.as_ref());
                    (
                        action.clone(),
                        OpenApiFragmentJson {
                            request,
                            responses,
                            auth,
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // Indexes
    let indexes = rd
        .indexes
        .as_ref()
        .map(|idxs| {
            idxs.iter()
                .map(|idx| IndexJson {
                    fields: idx.fields.clone(),
                    order: idx.order.clone(),
                    unique: idx.unique,
                })
                .collect()
        })
        .unwrap_or_default();

    ExplainJson {
        resource: rd.resource.clone(),
        version: rd.version,
        db: rd.db.clone(),
        tenant_key: rd.tenant_key.clone(),
        routes,
        table,
        relations,
        validations,
        openapi,
        indexes,
    }
}
