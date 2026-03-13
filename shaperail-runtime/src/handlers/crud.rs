use std::sync::Arc;

use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse};
use futures_util::TryStreamExt;
use shaperail_core::{EndpointSpec, FieldError, FieldType, ResourceDefinition, ShaperailError};

use crate::auth::extractor::{try_extract_auth, AuthenticatedUser};
use crate::auth::jwt::JwtConfig;
use crate::auth::rbac;
use crate::cache::RedisCache;
use crate::db::{ResourceQuery, ResourceStore};
use crate::events::EventEmitter;
use crate::jobs::{JobPriority, JobQueue};
use crate::observability::MetricsState;
use crate::storage::{parse_max_size, FileMetadata, StorageBackend, UploadHandler};

use super::params::{parse_item_params, parse_list_params, query_map_public};
use super::relations::load_relations;
use super::response;
use super::validate::validate_input;

/// Shared application state holding the database pool, resource definitions, and auth config.
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub resources: Vec<ResourceDefinition>,
    pub stores: Option<crate::db::StoreRegistry>,
    pub controllers: Option<super::controller::ControllerMap>,
    pub jwt_config: Option<Arc<JwtConfig>>,
    pub cache: Option<RedisCache>,
    pub event_emitter: Option<EventEmitter>,
    pub job_queue: Option<JobQueue>,
    pub metrics: Option<MetricsState>,
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

fn store_for(state: &AppState, resource: &ResourceDefinition) -> Option<Arc<dyn ResourceStore>> {
    state
        .stores
        .as_ref()
        .and_then(|stores| stores.get(&resource.resource).cloned())
}

/// When the app has a store registry, every resource must have a generated store.
/// Returns the store if present, or None if the app has no registry (tests/fallback).
/// Errors if registry exists but this resource has no store.
fn store_for_or_error(
    state: &AppState,
    resource: &ResourceDefinition,
) -> Result<Option<Arc<dyn ResourceStore>>, ShaperailError> {
    if let Some(store) = store_for(state, resource) {
        return Ok(Some(store));
    }
    if state.stores.is_some() {
        return Err(ShaperailError::Internal(format!(
            "Resource '{}' has no generated store; run shaperail generate",
            resource.resource
        )));
    }
    Ok(None)
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
                if let Some(metrics) = &state.metrics {
                    metrics.record_cache(true);
                }
                return Ok(HttpResponse::Ok()
                    .content_type("application/json")
                    .insert_header(("X-Cache", "HIT"))
                    .body(cached));
            }

            // Cache miss — execute query and store result
            if let Some(metrics) = &state.metrics {
                metrics.record_cache(false);
            }
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
    let store_opt = store_for_or_error(state, resource)?;
    let (rows, meta) = if let Some(store) = store_opt {
        store
            .find_all(
                endpoint,
                &params.filters,
                params.search.as_ref(),
                &params.sort,
                &params.page,
            )
            .await?
    } else {
        let rq = ResourceQuery::new(resource, &state.pool);
        rq.find_all(
            &params.filters,
            params.search.as_ref(),
            &params.sort,
            &params.page,
        )
        .await?
    };

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
                if let Some(metrics) = &state.metrics {
                    metrics.record_cache(true);
                }
                return Ok(HttpResponse::Ok()
                    .content_type("application/json")
                    .insert_header(("X-Cache", "HIT"))
                    .body(cached));
            }

            if let Some(metrics) = &state.metrics {
                metrics.record_cache(false);
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
    let store_opt = store_for_or_error(state, resource)?;
    let row = if let Some(store) = store_opt {
        store.find_by_id(id).await?
    } else {
        let rq = ResourceQuery::new(resource, &state.pool);
        rq.find_by_id(id).await?
    };
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

async fn emit_declared_events(
    state: &AppState,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    action: &str,
    data: &serde_json::Value,
) {
    let Some(events) = endpoint.events.as_ref() else {
        return;
    };

    let Some(emitter) = state.event_emitter.as_ref() else {
        tracing::warn!(
            resource = %resource.resource,
            action = action,
            "Endpoint declares custom events but no event emitter is configured"
        );
        return;
    };

    for event_name in events {
        if let Err(e) = emitter
            .emit(event_name, &resource.resource, action, data.clone())
            .await
        {
            tracing::warn!(
                event = %event_name,
                resource = %resource.resource,
                action = action,
                error = %e,
                "Failed to emit declared endpoint event (non-blocking)"
            );
        }
    }
}

async fn enqueue_declared_jobs(
    state: &AppState,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    action: &str,
    data: &serde_json::Value,
) {
    let Some(jobs) = endpoint.jobs.as_ref() else {
        return;
    };

    let Some(job_queue) = state.job_queue.as_ref() else {
        tracing::warn!(
            resource = %resource.resource,
            action = action,
            "Endpoint declares background jobs but no job queue is configured"
        );
        return;
    };

    let payload = serde_json::json!({
        "resource": resource.resource.as_str(),
        "action": action,
        "data": data,
    });

    for job_name in jobs {
        if let Err(e) = job_queue
            .enqueue(job_name, payload.clone(), JobPriority::Normal)
            .await
        {
            tracing::warn!(
                job = %job_name,
                resource = %resource.resource,
                action = action,
                error = %e,
                "Failed to enqueue declared endpoint job (non-blocking)"
            );
        }
    }
}

async fn run_write_side_effects(
    state: &AppState,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    action: &str,
    data: &serde_json::Value,
) {
    invalidate_cache(state, resource, action).await;
    auto_emit_event(state, resource, action, data).await;
    emit_declared_events(state, resource, endpoint, action, data).await;
    enqueue_declared_jobs(state, resource, endpoint, action, data).await;
}

fn schedule_file_cleanup(resource: &ResourceDefinition, deleted_data: &serde_json::Value) {
    let file_paths: Vec<String> = resource
        .schema
        .iter()
        .filter(|(_, field)| field.field_type == FieldType::File)
        .filter_map(|(name, _)| {
            deleted_data
                .get(name)
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
        })
        .collect();

    if file_paths.is_empty() {
        return;
    }

    let backend = match StorageBackend::from_env() {
        Ok(backend) => Arc::new(backend),
        Err(error) => {
            tracing::warn!(error = %error, "Skipping file cleanup: storage backend unavailable");
            return;
        }
    };

    tokio::spawn(async move {
        let handler = UploadHandler::new(backend);
        for path in file_paths {
            if let Err(error) = handler.delete(&path).await {
                tracing::warn!(path = %path, error = %error, "Failed to clean up uploaded file");
            }
        }
    });
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

/// Runs a before-controller if declared on the endpoint.
///
/// Builds a `ControllerContext`, calls the named function, and returns the
/// (potentially modified) input map. Returns the input unchanged if no
/// before-controller is declared.
async fn run_before_controller(
    state: &AppState,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    input: serde_json::Map<String, serde_json::Value>,
    user: Option<&AuthenticatedUser>,
    req: &HttpRequest,
) -> Result<serde_json::Map<String, serde_json::Value>, ShaperailError> {
    let name = match endpoint
        .controller
        .as_ref()
        .and_then(|c| c.before.as_deref())
    {
        Some(n) => n,
        None => return Ok(input),
    };
    let controllers = state.controllers.as_ref().ok_or_else(|| {
        ShaperailError::Internal(format!(
            "Endpoint declares controller.before '{name}' but no controller registry is configured"
        ))
    })?;
    let headers = req
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let mut ctx = super::controller::Context {
        input,
        data: None,
        user: user.cloned(),
        pool: state.pool.clone(),
        headers,
        response_headers: vec![],
    };
    controllers.call(&resource.resource, name, &mut ctx).await?;
    Ok(ctx.input)
}

/// Runs an after-controller if declared on the endpoint.
///
/// Passes the DB result data to the controller, which can modify it.
/// Returns the (potentially modified) data.
async fn run_after_controller(
    state: &AppState,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    data: serde_json::Value,
    user: Option<&AuthenticatedUser>,
    req: &HttpRequest,
) -> Result<serde_json::Value, ShaperailError> {
    let name = match endpoint
        .controller
        .as_ref()
        .and_then(|c| c.after.as_deref())
    {
        Some(n) => n,
        None => return Ok(data),
    };
    let controllers = state.controllers.as_ref().ok_or_else(|| {
        ShaperailError::Internal(format!(
            "Endpoint declares controller.after '{name}' but no controller registry is configured"
        ))
    })?;
    let headers = req
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let mut ctx = super::controller::Context {
        input: serde_json::Map::new(),
        data: Some(data),
        user: user.cloned(),
        pool: state.pool.clone(),
        headers,
        response_headers: vec![],
    };
    controllers.call(&resource.resource, name, &mut ctx).await?;
    Ok(ctx.data.unwrap_or(serde_json::Value::Null))
}

/// Generates an Actix-web create handler.
pub async fn handle_create(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    resource: web::Data<Arc<ResourceDefinition>>,
    endpoint: web::Data<Arc<EndpointSpec>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, ShaperailError> {
    let user = enforce_auth(&req, &endpoint)?;
    let input_data = extract_input(&body, &resource, &endpoint)?;
    validate_input(&input_data, &resource)?;

    // Before-controller: can modify input
    let input_data = run_before_controller(
        &state,
        &resource,
        &endpoint,
        input_data,
        user.as_ref(),
        &req,
    )
    .await?;

    let store_opt = store_for_or_error(&state, &resource)?;
    let row = if let Some(store) = store_opt {
        store.insert(&input_data).await?
    } else {
        let rq = ResourceQuery::new(&resource, &state.pool);
        rq.insert(&input_data).await?
    };
    let params = parse_item_params(&req);
    let side_effect_data = row.0.clone();
    let mut data = row.0;

    // After-controller: can modify response data
    data = run_after_controller(&state, &resource, &endpoint, data, user.as_ref(), &req).await?;

    if !params.fields.is_empty() {
        data = response::select_fields(&data, &params.fields);
    }

    run_write_side_effects(&state, &resource, &endpoint, "created", &side_effect_data).await;
    Ok(response::created(data))
}

/// Create handler for endpoints that declare `upload`.
pub async fn handle_create_upload(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    resource: web::Data<Arc<ResourceDefinition>>,
    endpoint: web::Data<Arc<EndpointSpec>>,
    payload: Multipart,
) -> Result<HttpResponse, ShaperailError> {
    enforce_auth(&req, &endpoint)?;
    let input_data = extract_input_from_multipart(payload, &resource, &endpoint).await?;
    validate_input(&input_data, &resource)?;

    let store_opt = store_for_or_error(&state, &resource)?;
    let row = if let Some(store) = store_opt {
        store.insert(&input_data).await?
    } else {
        let rq = ResourceQuery::new(&resource, &state.pool);
        rq.insert(&input_data).await?
    };
    let params = parse_item_params(&req);
    let side_effect_data = row.0.clone();
    let mut data = row.0;

    if !params.fields.is_empty() {
        data = response::select_fields(&data, &params.fields);
    }

    run_write_side_effects(&state, &resource, &endpoint, "created", &side_effect_data).await;
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
    let store_opt = store_for_or_error(&state, &resource)?;

    // Owner check: fetch record first to verify ownership
    if rbac::needs_owner_check(endpoint.auth.as_ref(), user.as_ref()) {
        let existing = if let Some(ref store) = store_opt {
            store.find_by_id(&id).await?
        } else {
            let rq = ResourceQuery::new(&resource, &state.pool);
            rq.find_by_id(&id).await?
        };
        if let Some(ref u) = user {
            rbac::check_owner(u, &existing.0)?;
        }
    }

    // Before-controller: can modify input
    let input_data = run_before_controller(
        &state,
        &resource,
        &endpoint,
        input_data,
        user.as_ref(),
        &req,
    )
    .await?;

    let row = if let Some(store) = store_opt {
        store.update_by_id(&id, &input_data).await?
    } else {
        let rq = ResourceQuery::new(&resource, &state.pool);
        rq.update_by_id(&id, &input_data).await?
    };
    let params = parse_item_params(&req);
    let side_effect_data = row.0.clone();
    let mut data = row.0;

    // After-controller: can modify response data
    data = run_after_controller(&state, &resource, &endpoint, data, user.as_ref(), &req).await?;

    if !params.fields.is_empty() {
        data = response::select_fields(&data, &params.fields);
    }

    run_write_side_effects(&state, &resource, &endpoint, "updated", &side_effect_data).await;
    Ok(response::single(data))
}

/// Update handler for endpoints that declare `upload`.
pub async fn handle_update_upload(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    resource: web::Data<Arc<ResourceDefinition>>,
    endpoint: web::Data<Arc<EndpointSpec>>,
    path: web::Path<String>,
    payload: Multipart,
) -> Result<HttpResponse, ShaperailError> {
    let user = enforce_auth(&req, &endpoint)?;
    let id = parse_uuid(&path)?;
    let input_data = extract_input_from_multipart(payload, &resource, &endpoint).await?;
    let store_opt = store_for_or_error(&state, &resource)?;

    if rbac::needs_owner_check(endpoint.auth.as_ref(), user.as_ref()) {
        let existing = if let Some(ref store) = store_opt {
            store.find_by_id(&id).await?
        } else {
            let rq = ResourceQuery::new(&resource, &state.pool);
            rq.find_by_id(&id).await?
        };
        if let Some(ref u) = user {
            rbac::check_owner(u, &existing.0)?;
        }
    }

    validate_input(&input_data, &resource)?;

    let row = if let Some(store) = store_opt {
        store.update_by_id(&id, &input_data).await?
    } else {
        let rq = ResourceQuery::new(&resource, &state.pool);
        rq.update_by_id(&id, &input_data).await?
    };
    let params = parse_item_params(&req);
    let side_effect_data = row.0.clone();
    let mut data = row.0;

    if !params.fields.is_empty() {
        data = response::select_fields(&data, &params.fields);
    }

    run_write_side_effects(&state, &resource, &endpoint, "updated", &side_effect_data).await;
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
    let store_opt = store_for_or_error(&state, &resource)?;

    // Owner check: fetch record first
    if rbac::needs_owner_check(endpoint.auth.as_ref(), user.as_ref()) {
        let existing = if let Some(ref store) = store_opt {
            store.find_by_id(&id).await?
        } else {
            let rq = ResourceQuery::new(&resource, &state.pool);
            rq.find_by_id(&id).await?
        };
        if let Some(ref u) = user {
            rbac::check_owner(u, &existing.0)?;
        }
    }

    // Before-controller: can halt deletion
    let input = serde_json::Map::new();
    let _ = run_before_controller(&state, &resource, &endpoint, input, user.as_ref(), &req).await?;

    let (result, deleted_data) = if endpoint.soft_delete {
        let row = if let Some(ref store) = store_opt {
            store.soft_delete_by_id(&id).await?
        } else {
            let rq = ResourceQuery::new(&resource, &state.pool);
            rq.soft_delete_by_id(&id).await?
        };
        let data = row.0.clone();
        (response::no_content(), data)
    } else {
        let row = if let Some(ref store) = store_opt {
            store.hard_delete_by_id(&id).await?
        } else {
            let rq = ResourceQuery::new(&resource, &state.pool);
            rq.hard_delete_by_id(&id).await?
        };
        (response::no_content(), row.0)
    };

    if !endpoint.soft_delete {
        schedule_file_cleanup(&resource, &deleted_data);
    }
    run_write_side_effects(&state, &resource, &endpoint, "deleted", &deleted_data).await;
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

    let store_opt = store_for_or_error(&state, &resource)?;
    let mut results = Vec::with_capacity(items.len());

    for item in items {
        let input_data = extract_input_from_value(item, &resource, &endpoint)?;
        validate_input(&input_data, &resource)?;
        let row = if let Some(ref store) = store_opt {
            store.insert(&input_data).await?
        } else {
            let rq = ResourceQuery::new(&resource, &state.pool);
            rq.insert(&input_data).await?
        };
        results.push(row.0);
    }

    invalidate_cache(&state, &resource, "create").await;
    for item in &results {
        auto_emit_event(&state, &resource, "created", item).await;
        emit_declared_events(&state, &resource, &endpoint, "created", item).await;
        enqueue_declared_jobs(&state, &resource, &endpoint, "created", item).await;
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

    let store_opt = store_for_or_error(&state, &resource)?;
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
            let row = if let Some(ref store) = store_opt {
                store.soft_delete_by_id(&id).await?
            } else {
                let rq = ResourceQuery::new(&resource, &state.pool);
                rq.soft_delete_by_id(&id).await?
            };
            results.push(row.0);
        } else {
            let row = if let Some(ref store) = store_opt {
                store.hard_delete_by_id(&id).await?
            } else {
                let rq = ResourceQuery::new(&resource, &state.pool);
                rq.hard_delete_by_id(&id).await?
            };
            results.push(row.0);
        }
    }

    if !endpoint.soft_delete {
        for item in &results {
            schedule_file_cleanup(&resource, item);
        }
    }
    invalidate_cache(&state, &resource, "delete").await;
    for item in &results {
        auto_emit_event(&state, &resource, "deleted", item).await;
        emit_declared_events(&state, &resource, &endpoint, "deleted", item).await;
        enqueue_declared_jobs(&state, &resource, &endpoint, "deleted", item).await;
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

async fn extract_input_from_multipart(
    mut payload: Multipart,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
) -> Result<serde_json::Map<String, serde_json::Value>, ShaperailError> {
    let upload = endpoint.upload.as_ref().ok_or_else(|| {
        ShaperailError::Internal("multipart handler invoked without upload spec".to_string())
    })?;

    let backend = Arc::new(StorageBackend::from_name(&upload.storage)?);
    let handler = UploadHandler::new(backend);
    let max_size = parse_max_size(&upload.max_size)?;
    let storage_prefix = format!("{}/{}", resource.resource, upload.field);

    let mut body = serde_json::Map::new();
    let mut uploaded_metadata: Option<FileMetadata> = None;

    while let Some(mut field) = payload
        .try_next()
        .await
        .map_err(|e| ShaperailError::Internal(format!("Failed to read multipart body: {e}")))?
    {
        let field_name = field.name().unwrap_or_default().to_string();
        let mut bytes = Vec::new();

        while let Some(chunk) = field
            .try_next()
            .await
            .map_err(|e| ShaperailError::Internal(format!("Failed to read multipart field: {e}")))?
        {
            bytes.extend_from_slice(&chunk);
        }

        if field_name == upload.field {
            let filename = field
                .content_disposition()
                .and_then(|cd| cd.get_filename())
                .ok_or_else(|| {
                    ShaperailError::Validation(vec![FieldError {
                        field: field_name.clone(),
                        message: "Uploaded file must include a filename".to_string(),
                        code: "missing_filename".to_string(),
                    }])
                })?;
            let mime_type = field
                .content_type()
                .map(|mime| mime.essence_str().to_string())
                .unwrap_or_else(|| "application/octet-stream".to_string());

            let metadata = handler
                .process_upload(
                    filename,
                    &bytes,
                    &mime_type,
                    Some(max_size),
                    upload.types.as_deref(),
                    &storage_prefix,
                )
                .await?;

            body.insert(
                field_name.clone(),
                serde_json::Value::String(metadata.path.clone()),
            );
            uploaded_metadata = Some(metadata);
            continue;
        }

        let raw = String::from_utf8(bytes).map_err(|_| {
            ShaperailError::Validation(vec![FieldError {
                field: field_name.clone(),
                message: "Multipart text fields must be valid UTF-8".to_string(),
                code: "invalid_utf8".to_string(),
            }])
        })?;
        let value = coerce_form_value(&field_name, &raw, resource)?;
        body.insert(field_name, value);
    }

    let body_value = serde_json::Value::Object(body);
    let mut input = extract_input_from_value(&body_value, resource, endpoint)?;

    if let Some(metadata) = uploaded_metadata.as_ref() {
        inject_upload_metadata(&mut input, resource, &upload.field, metadata);
    }

    Ok(input)
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

fn coerce_form_value(
    field_name: &str,
    raw: &str,
    resource: &ResourceDefinition,
) -> Result<serde_json::Value, ShaperailError> {
    let Some(schema) = resource.schema.get(field_name) else {
        return Ok(serde_json::Value::String(raw.to_string()));
    };

    match schema.field_type {
        shaperail_core::FieldType::String
        | shaperail_core::FieldType::Enum
        | shaperail_core::FieldType::File
        | shaperail_core::FieldType::Uuid
        | shaperail_core::FieldType::Timestamp
        | shaperail_core::FieldType::Date => Ok(serde_json::Value::String(raw.to_string())),
        shaperail_core::FieldType::Integer => raw
            .parse::<i32>()
            .map(serde_json::Value::from)
            .map_err(|_| {
                multipart_field_error(field_name, "must be a valid integer", "invalid_integer")
            }),
        shaperail_core::FieldType::Bigint => raw
            .parse::<i64>()
            .map(serde_json::Value::from)
            .map_err(|_| {
                multipart_field_error(field_name, "must be a valid integer", "invalid_bigint")
            }),
        shaperail_core::FieldType::Number => raw
            .parse::<f64>()
            .map(serde_json::Value::from)
            .map_err(|_| {
                multipart_field_error(field_name, "must be a valid number", "invalid_number")
            }),
        shaperail_core::FieldType::Boolean => raw
            .parse::<bool>()
            .map(serde_json::Value::from)
            .map_err(|_| {
                multipart_field_error(field_name, "must be true or false", "invalid_boolean")
            }),
        shaperail_core::FieldType::Json | shaperail_core::FieldType::Array => {
            serde_json::from_str(raw).map_err(|_| {
                multipart_field_error(field_name, "must be valid JSON", "invalid_json")
            })
        }
    }
}

fn inject_upload_metadata(
    input: &mut serde_json::Map<String, serde_json::Value>,
    resource: &ResourceDefinition,
    field_name: &str,
    metadata: &FileMetadata,
) {
    let filename_field = format!("{field_name}_filename");
    if resource.schema.contains_key(&filename_field) {
        input.insert(
            filename_field,
            serde_json::Value::String(metadata.filename.clone()),
        );
    }

    let mime_field = format!("{field_name}_mime_type");
    if resource.schema.contains_key(&mime_field) {
        input.insert(
            mime_field,
            serde_json::Value::String(metadata.mime_type.clone()),
        );
    }

    let size_field = format!("{field_name}_size");
    if resource.schema.contains_key(&size_field) {
        input.insert(size_field, serde_json::Value::from(metadata.size));
    }
}

fn multipart_field_error(field: &str, message: &str, code: &str) -> ShaperailError {
    ShaperailError::Validation(vec![FieldError {
        field: field.to_string(),
        message: format!("{field} {message}"),
        code: code.to_string(),
    }])
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
            db: None,
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
            controller: None,
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
            controller: None,
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
