use std::sync::Arc;

use actix_web::{web, HttpRequest, HttpResponse};
use shaperail_core::{EndpointSpec, FieldError, ResourceDefinition, ShaperailError};

use crate::auth::extractor::{try_extract_auth, AuthenticatedUser};
use crate::auth::jwt::JwtConfig;
use crate::auth::rbac;
use crate::cache::RedisCache;
use crate::db::ResourceQuery;
use crate::events::EventEmitter;

use super::params::{parse_item_params, parse_list_params, query_map_public};
use super::relations::load_relations;
use super::response;
use super::validate::validate_input;

/// Shared application state holding the database pool, resource definitions, and auth config.
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub resources: Vec<ResourceDefinition>,
    pub jwt_config: Option<Arc<JwtConfig>>,
    pub cache: Option<RedisCache>,
    pub event_emitter: Option<EventEmitter>,
}

/// Enforces auth rules for an endpoint, returning the authenticated user if present.
fn enforce_auth(
    req: &HttpRequest,
    endpoint: &EndpointSpec,
) -> Result<Option<AuthenticatedUser>, ShaperailError> {
    let user = try_extract_auth(req);
    rbac::enforce(endpoint.auth.as_ref(), user.as_ref())?;
    Ok(user)
}

/// Checks if cache should be bypassed via `?nocache=1` or admin role.
fn should_bypass_cache(req: &HttpRequest, user: Option<&AuthenticatedUser>) -> bool {
    let query = req.query_string();
    if query.contains("nocache=1") {
        return true;
    }
    if let Some(u) = user {
        if u.role == "admin" {
            return true;
        }
    }
    false
}

/// Extracts the user's primary role for cache key partitioning.
fn user_role_for_cache(user: Option<&AuthenticatedUser>) -> &str {
    match user {
        Some(u) => u.role.as_str(),
        None => "anonymous",
    }
}

/// Generates an Actix-web list handler for a resource endpoint.
pub async fn handle_list(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    resource: web::Data<Arc<ResourceDefinition>>,
    endpoint: web::Data<Arc<EndpointSpec>>,
) -> Result<HttpResponse, ShaperailError> {
    let user = enforce_auth(&req, &endpoint)?;

    // Cache check for endpoints with cache.ttl configured
    if let (Some(ref cache), Some(ref cache_spec)) = (&state.cache, &endpoint.cache) {
        if !should_bypass_cache(&req, user.as_ref()) {
            let query_params = query_map_public(&req);
            let role = user_role_for_cache(user.as_ref());
            let cache_key = RedisCache::build_key(&resource.resource, "list", &query_params, role);

            if let Some(cached) = cache.get(&cache_key).await {
                return Ok(HttpResponse::Ok()
                    .content_type("application/json")
                    .insert_header(("X-Cache", "HIT"))
                    .body(cached));
            }

            // Cache miss — execute query and store result
            let result = execute_list(&req, &state, &resource, &endpoint, user).await?;
            let body = result.to_string();
            cache.set(&cache_key, &body, cache_spec.ttl).await;
            return Ok(HttpResponse::Ok()
                .content_type("application/json")
                .insert_header(("X-Cache", "MISS"))
                .json(result));
        }
    }

    let result = execute_list(&req, &state, &resource, &endpoint, user).await?;
    Ok(HttpResponse::Ok().json(result))
}

/// Executes the list query and returns the response as a JSON value.
async fn execute_list(
    req: &HttpRequest,
    state: &web::Data<Arc<AppState>>,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    _user: Option<AuthenticatedUser>,
) -> Result<serde_json::Value, ShaperailError> {
    let params = parse_list_params(req, endpoint);
    let rq = ResourceQuery::new(resource, &state.pool);

    let (rows, meta) = rq
        .find_all(
            &params.filters,
            params.search.as_ref(),
            &params.sort,
            &params.page,
        )
        .await?;

    let mut data: Vec<serde_json::Value> = rows.into_iter().map(|r| r.0).collect();

    if !params.include.is_empty() {
        load_relations(&mut data, resource, &params.include, state).await?;
    }

    if !params.fields.is_empty() {
        data = data
            .iter()
            .map(|v| response::select_fields(v, &params.fields))
            .collect();
    }

    Ok(serde_json::json!({
        "data": data,
        "meta": meta
    }))
}

/// Generates an Actix-web get (single record) handler.
pub async fn handle_get(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    resource: web::Data<Arc<ResourceDefinition>>,
    endpoint: web::Data<Arc<EndpointSpec>>,
    path: web::Path<String>,
) -> Result<HttpResponse, ShaperailError> {
    let user = enforce_auth(&req, &endpoint)?;
    let id = parse_uuid(&path)?;

    // Cache check for endpoints with cache.ttl configured
    if let (Some(ref cache), Some(ref cache_spec)) = (&state.cache, &endpoint.cache) {
        if !should_bypass_cache(&req, user.as_ref()) {
            let mut query_params = query_map_public(&req);
            query_params.insert("_id".to_string(), id.to_string());
            let role = user_role_for_cache(user.as_ref());
            let cache_key = RedisCache::build_key(&resource.resource, "get", &query_params, role);

            if let Some(cached) = cache.get(&cache_key).await {
                return Ok(HttpResponse::Ok()
                    .content_type("application/json")
                    .insert_header(("X-Cache", "HIT"))
                    .body(cached));
            }

            let result =
                execute_get(&state, &resource, &endpoint, &id, user.as_ref(), &req).await?;
            let body = serde_json::to_string(&result)
                .map_err(|e| ShaperailError::Internal(format!("JSON serialization error: {e}")))?;
            cache.set(&cache_key, &body, cache_spec.ttl).await;
            return Ok(HttpResponse::Ok()
                .content_type("application/json")
                .insert_header(("X-Cache", "MISS"))
                .json(result));
        }
    }

    let result = execute_get(&state, &resource, &endpoint, &id, user.as_ref(), &req).await?;
    Ok(response::single_value(result))
}

/// Executes get-by-id and returns the data value.
async fn execute_get(
    state: &web::Data<Arc<AppState>>,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    id: &uuid::Uuid,
    user: Option<&AuthenticatedUser>,
    req: &HttpRequest,
) -> Result<serde_json::Value, ShaperailError> {
    let rq = ResourceQuery::new(resource, &state.pool);
    let row = rq.find_by_id(id).await?;
    let params = parse_item_params(req);
    let mut data = row.0;

    if rbac::needs_owner_check(endpoint.auth.as_ref(), user) {
        if let Some(u) = user {
            rbac::check_owner(u, &data)?;
        }
    }

    if !params.include.is_empty() {
        let mut items = vec![data];
        load_relations(&mut items, resource, &params.include, state).await?;
        data = items.into_iter().next().unwrap_or(serde_json::Value::Null);
    }

    if !params.fields.is_empty() {
        data = response::select_fields(&data, &params.fields);
    }

    Ok(data)
}

/// Auto-emits an event for a resource action (non-blocking via job queue).
///
/// Emits `<resource>.<action>` (e.g., "users.created") automatically after
/// every create/update/delete operation. Events never block the HTTP response.
async fn auto_emit_event(
    state: &AppState,
    resource: &ResourceDefinition,
    action: &str,
    data: &serde_json::Value,
) {
    if let Some(ref emitter) = state.event_emitter {
        let event_name = format!("{}.{}", resource.resource, action);
        if let Err(e) = emitter
            .emit(&event_name, &resource.resource, action, data.clone())
            .await
        {
            tracing::warn!(
                event = %event_name,
                error = %e,
                "Failed to emit event (non-blocking)"
            );
        }
    }
}

/// Invalidates cache for a resource after a write operation.
async fn invalidate_cache(state: &AppState, resource: &ResourceDefinition, action: &str) {
    if let Some(ref cache) = state.cache {
        // Check if any endpoint has invalidate_on configured
        let invalidate_on = resource
            .endpoints
            .as_ref()
            .and_then(|eps| {
                eps.values()
                    .find_map(|ep| ep.cache.as_ref().and_then(|c| c.invalidate_on.as_ref()))
            })
            .map(|v| v.as_slice());
        cache
            .invalidate_if_needed(&resource.resource, action, invalidate_on)
            .await;
    }
}

/// Generates an Actix-web create handler.
pub async fn handle_create(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    resource: web::Data<Arc<ResourceDefinition>>,
    endpoint: web::Data<Arc<EndpointSpec>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, ShaperailError> {
    enforce_auth(&req, &endpoint)?;
    let input_data = extract_input(&body, &resource, &endpoint)?;
    validate_input(&input_data, &resource)?;

    let rq = ResourceQuery::new(&resource, &state.pool);
    let row = rq.insert(&input_data).await?;
    let params = parse_item_params(&req);
    let mut data = row.0;

    if !params.fields.is_empty() {
        data = response::select_fields(&data, &params.fields);
    }

    invalidate_cache(&state, &resource, "create").await;
    auto_emit_event(&state, &resource, "created", &data).await;
    Ok(response::created(data))
}

/// Generates an Actix-web update handler.
pub async fn handle_update(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    resource: web::Data<Arc<ResourceDefinition>>,
    endpoint: web::Data<Arc<EndpointSpec>>,
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, ShaperailError> {
    let user = enforce_auth(&req, &endpoint)?;
    let id = parse_uuid(&path)?;
    let input_data = extract_input(&body, &resource, &endpoint)?;

    let rq = ResourceQuery::new(&resource, &state.pool);

    // Owner check: fetch record first to verify ownership
    if rbac::needs_owner_check(endpoint.auth.as_ref(), user.as_ref()) {
        let existing = rq.find_by_id(&id).await?;
        if let Some(ref u) = user {
            rbac::check_owner(u, &existing.0)?;
        }
    }

    let row = rq.update_by_id(&id, &input_data).await?;
    let params = parse_item_params(&req);
    let mut data = row.0;

    if !params.fields.is_empty() {
        data = response::select_fields(&data, &params.fields);
    }

    invalidate_cache(&state, &resource, "update").await;
    auto_emit_event(&state, &resource, "updated", &data).await;
    Ok(response::single(data))
}

/// Generates an Actix-web delete handler (soft or hard).
pub async fn handle_delete(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    resource: web::Data<Arc<ResourceDefinition>>,
    endpoint: web::Data<Arc<EndpointSpec>>,
    path: web::Path<String>,
) -> Result<HttpResponse, ShaperailError> {
    let user = enforce_auth(&req, &endpoint)?;
    let id = parse_uuid(&path)?;
    let rq = ResourceQuery::new(&resource, &state.pool);

    // Owner check: fetch record first
    if rbac::needs_owner_check(endpoint.auth.as_ref(), user.as_ref()) {
        let existing = rq.find_by_id(&id).await?;
        if let Some(ref u) = user {
            rbac::check_owner(u, &existing.0)?;
        }
    }

    let (result, deleted_data) = if endpoint.soft_delete {
        let row = rq.soft_delete_by_id(&id).await?;
        let data = row.0.clone();
        (response::single(row.0), data)
    } else {
        let row = rq.hard_delete_by_id(&id).await?;
        (response::no_content(), row.0)
    };

    invalidate_cache(&state, &resource, "delete").await;
    auto_emit_event(&state, &resource, "deleted", &deleted_data).await;
    Ok(result)
}

/// Bulk create handler — accepts an array of up to 500 items.
pub async fn handle_bulk_create(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    resource: web::Data<Arc<ResourceDefinition>>,
    endpoint: web::Data<Arc<EndpointSpec>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, ShaperailError> {
    enforce_auth(&req, &endpoint)?;
    let items = body.as_array().ok_or_else(|| {
        ShaperailError::Validation(vec![FieldError {
            field: "body".to_string(),
            message: "Expected an array of items".to_string(),
            code: "invalid_body".to_string(),
        }])
    })?;

    if items.len() > 500 {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "body".to_string(),
            message: "Bulk create accepts at most 500 items".to_string(),
            code: "too_many_items".to_string(),
        }]));
    }

    if items.is_empty() {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "body".to_string(),
            message: "Expected at least one item".to_string(),
            code: "empty_body".to_string(),
        }]));
    }

    let rq = ResourceQuery::new(&resource, &state.pool);
    let mut results = Vec::with_capacity(items.len());

    for item in items {
        let input_data = extract_input_from_value(item, &resource, &endpoint)?;
        validate_input(&input_data, &resource)?;
        let row = rq.insert(&input_data).await?;
        results.push(row.0);
    }

    invalidate_cache(&state, &resource, "create").await;
    for item in &results {
        auto_emit_event(&state, &resource, "created", item).await;
    }
    Ok(response::bulk(results))
}

/// Bulk delete handler — accepts an array of UUIDs.
pub async fn handle_bulk_delete(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    resource: web::Data<Arc<ResourceDefinition>>,
    endpoint: web::Data<Arc<EndpointSpec>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, ShaperailError> {
    enforce_auth(&req, &endpoint)?;
    let ids = body.as_array().ok_or_else(|| {
        ShaperailError::Validation(vec![FieldError {
            field: "body".to_string(),
            message: "Expected an array of IDs".to_string(),
            code: "invalid_body".to_string(),
        }])
    })?;

    let rq = ResourceQuery::new(&resource, &state.pool);
    let mut results = Vec::with_capacity(ids.len());

    for id_value in ids {
        let id_str = id_value.as_str().ok_or_else(|| {
            ShaperailError::Validation(vec![FieldError {
                field: "id".to_string(),
                message: "Each ID must be a string UUID".to_string(),
                code: "invalid_id".to_string(),
            }])
        })?;
        let id = uuid::Uuid::parse_str(id_str).map_err(|_| {
            ShaperailError::Validation(vec![FieldError {
                field: "id".to_string(),
                message: format!("Invalid UUID: {id_str}"),
                code: "invalid_uuid".to_string(),
            }])
        })?;

        if endpoint.soft_delete {
            let row = rq.soft_delete_by_id(&id).await?;
            results.push(row.0);
        } else {
            let row = rq.hard_delete_by_id(&id).await?;
            results.push(row.0);
        }
    }

    invalidate_cache(&state, &resource, "delete").await;
    for item in &results {
        auto_emit_event(&state, &resource, "deleted", item).await;
    }
    Ok(response::bulk(results))
}

/// Parses a UUID from a path string.
fn parse_uuid(s: &str) -> Result<uuid::Uuid, ShaperailError> {
    uuid::Uuid::parse_str(s).map_err(|_| {
        ShaperailError::Validation(vec![FieldError {
            field: "id".to_string(),
            message: format!("Invalid UUID: {s}"),
            code: "invalid_uuid".to_string(),
        }])
    })
}

/// Extracts input fields from a JSON body based on the endpoint's `input` list.
///
/// If the endpoint has an `input` list, only those fields are accepted.
/// Otherwise, all non-generated, non-primary fields are accepted.
fn extract_input(
    body: &serde_json::Value,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
) -> Result<serde_json::Map<String, serde_json::Value>, ShaperailError> {
    extract_input_from_value(body, resource, endpoint)
}

fn extract_input_from_value(
    value: &serde_json::Value,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
) -> Result<serde_json::Map<String, serde_json::Value>, ShaperailError> {
    let obj = value.as_object().ok_or_else(|| {
        ShaperailError::Validation(vec![FieldError {
            field: "body".to_string(),
            message: "Expected a JSON object".to_string(),
            code: "invalid_body".to_string(),
        }])
    })?;

    let allowed: Vec<&str> = if let Some(input_fields) = &endpoint.input {
        input_fields.iter().map(|s| s.as_str()).collect()
    } else {
        // Accept all non-generated, non-primary fields
        resource
            .schema
            .iter()
            .filter(|(_, fs)| !fs.generated && !fs.primary)
            .map(|(name, _)| name.as_str())
            .collect()
    };

    let mut result = serde_json::Map::new();
    for (key, value) in obj {
        if allowed.contains(&key.as_str()) {
            result.insert(key.clone(), value.clone());
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use shaperail_core::{FieldSchema, FieldType, HttpMethod};

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

        ResourceDefinition {
            resource: "users".to_string(),
            version: 1,
            schema,
            endpoints: None,
            relations: None,
            indexes: None,
        }
    }

    #[test]
    fn extract_input_with_explicit_fields() {
        let resource = test_resource();
        let endpoint = EndpointSpec {
            method: HttpMethod::Post,
            path: "/users".to_string(),
            auth: None,
            input: Some(vec!["name".to_string()]),
            filters: None,
            search: None,
            pagination: None,
            sort: None,
            cache: None,
            hooks: None,
            events: None,
            jobs: None,
            upload: None,
            soft_delete: false,
        };

        let body =
            serde_json::json!({"name": "Alice", "email": "alice@test.com", "id": "should-ignore"});
        let result = extract_input(&body, &resource, &endpoint).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert!(result.get("email").is_none());
        assert!(result.get("id").is_none());
    }

    #[test]
    fn extract_input_without_explicit_fields() {
        let resource = test_resource();
        let endpoint = EndpointSpec {
            method: HttpMethod::Post,
            path: "/users".to_string(),
            auth: None,
            input: None,
            filters: None,
            search: None,
            pagination: None,
            sort: None,
            cache: None,
            hooks: None,
            events: None,
            jobs: None,
            upload: None,
            soft_delete: false,
        };

        let body =
            serde_json::json!({"name": "Alice", "email": "alice@test.com", "id": "should-ignore"});
        let result = extract_input(&body, &resource, &endpoint).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains_key("name"));
        assert!(result.contains_key("email"));
        assert!(!result.contains_key("id"));
    }

    #[test]
    fn parse_uuid_valid() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let result = parse_uuid(uuid_str);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_uuid_invalid() {
        let result = parse_uuid("not-a-uuid");
        assert!(result.is_err());
    }
}
