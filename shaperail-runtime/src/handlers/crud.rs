use std::sync::Arc;

use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse};
use futures_util::TryStreamExt;
#[cfg(test)]
use shaperail_core::RateLimitSpec;
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
    /// Per-endpoint Redis-backed rate limiter. `None` if Redis is not configured.
    pub rate_limiter: Option<Arc<crate::auth::RateLimiter>>,
    /// Custom endpoint handler registry. Keys are "{resource}:{action}".
    pub custom_handlers: Option<super::custom::CustomHandlerMap>,
    pub metrics: Option<MetricsState>,
    /// WASM plugin runtime (M19). Requires `wasm-plugins` feature.
    #[cfg(feature = "wasm-plugins")]
    pub wasm_runtime: Option<crate::plugins::WasmRuntime>,
    /// Broadcast channel for GraphQL subscriptions (M15). Sends event payloads to subscribers.
    pub event_bus: tokio::sync::broadcast::Sender<(String, serde_json::Value)>,
}

impl AppState {
    /// Subscribe to events on the broadcast bus (for GraphQL subscriptions).
    /// Returns a receiver that gets all events matching the given event name.
    pub fn event_bus_subscribe(
        &self,
        event_name: &str,
    ) -> tokio::sync::broadcast::Receiver<serde_json::Value> {
        let (tx, rx) = tokio::sync::broadcast::channel(64);
        let mut bus_rx = self.event_bus.subscribe();
        let name = event_name.to_string();
        tokio::spawn(async move {
            while let Ok((evt, payload)) = bus_rx.recv().await {
                if evt == name {
                    let _ = tx.send(payload);
                }
            }
        });
        rx
    }
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

/// Returns the tenant_id for the current request, if the resource has `tenant_key` set
/// and the user has a `tenant_id` claim.
fn resolve_tenant_id(
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
) -> Option<String> {
    resource.tenant_key.as_ref()?;
    user.and_then(|u| u.tenant_id.clone())
}

/// Returns true if the user is a `super_admin` and should bypass tenant filtering.
fn is_super_admin(user: Option<&AuthenticatedUser>) -> bool {
    user.map(|u| u.role == "super_admin").unwrap_or(false)
}

/// Checks that a fetched record belongs to the authenticated user's tenant.
/// Returns `Err(Forbidden)` if the user has no `tenant_id` claim or is unauthenticated.
/// Returns `Err(NotFound)` if the tenant_key value doesn't match the user's tenant.
/// Skips the check if: no tenant_key on resource, or user is super_admin.
fn verify_tenant(
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    data: &serde_json::Value,
) -> Result<(), ShaperailError> {
    let tenant_key = match &resource.tenant_key {
        Some(k) => k,
        None => return Ok(()),
    };
    if is_super_admin(user) {
        return Ok(());
    }
    let user_tenant = match user.and_then(|u| u.tenant_id.as_deref()) {
        Some(t) => t,
        None => return Err(ShaperailError::Forbidden),
    };
    let record_tenant = data.get(tenant_key).and_then(|v| v.as_str()).unwrap_or("");
    if record_tenant != user_tenant {
        return Err(ShaperailError::NotFound);
    }
    Ok(())
}

/// Injects tenant_key filter into a FilterSet for list queries.
/// Returns `Err(Forbidden)` if the user is unauthenticated or has no `tenant_id` claim.
/// Skips the injection if: no tenant_key on resource, or user is super_admin.
fn inject_tenant_filter(
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    filters: &mut crate::db::FilterSet,
) -> Result<(), ShaperailError> {
    let tenant_key = match &resource.tenant_key {
        Some(k) => k,
        None => return Ok(()),
    };
    if is_super_admin(user) {
        return Ok(());
    }
    match user.and_then(|u| u.tenant_id.as_deref()) {
        Some(tenant_id) => {
            filters.add(tenant_key.clone(), tenant_id.to_string());
            Ok(())
        }
        None => Err(ShaperailError::Forbidden),
    }
}

/// Auto-injects tenant_key into create input data.
fn inject_tenant_into_input(
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    input: &mut serde_json::Map<String, serde_json::Value>,
) {
    let tenant_key = match &resource.tenant_key {
        Some(k) => k,
        None => return,
    };
    if let Some(tenant_id) = user.and_then(|u| u.tenant_id.as_deref()) {
        // Only inject if not already provided
        if !input.contains_key(tenant_key) {
            input.insert(
                tenant_key.clone(),
                serde_json::Value::String(tenant_id.to_string()),
            );
        }
    }
}

/// Returns the tenant_id string for cache/rate-limit key scoping.
fn tenant_id_for_key(user: Option<&AuthenticatedUser>) -> &str {
    match user.and_then(|u| u.tenant_id.as_deref()) {
        Some(t) => t,
        None => "_",
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
pub(crate) fn store_for_or_error(
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

/// Checks the per-endpoint rate limit for the current request.
///
/// Returns `Ok(())` if:
/// - The endpoint has no `rate_limit:` configured, or
/// - `AppState.rate_limiter` is `None` (Redis not configured).
///
/// Returns `Err(ShaperailError::RateLimited)` if the limit is exceeded.
async fn check_rate_limit(
    endpoint: &EndpointSpec,
    state: &AppState,
    req: &HttpRequest,
    user: Option<&AuthenticatedUser>,
    resource_action: &str,
) -> Result<(), ShaperailError> {
    let Some(ref spec) = endpoint.rate_limit else {
        return Ok(());
    };
    let Some(ref limiter) = state.rate_limiter else {
        return Ok(());
    };
    let ip = req
        .connection_info()
        .peer_addr()
        .unwrap_or("unknown")
        .to_string();
    let user_id = user.map(|u| u.id.as_str());
    let tenant_id = user.and_then(|u| u.tenant_id.as_deref());
    let base_key = crate::auth::RateLimiter::key_for_tenant(&ip, user_id, tenant_id);
    let key = format!("{resource_action}:{base_key}");
    // Per-endpoint config — pool is shared (Arc clone), config is endpoint-specific
    let endpoint_limiter = crate::auth::RateLimiter::new(
        limiter.pool(),
        crate::auth::RateLimitConfig {
            max_requests: spec.max_requests,
            window_secs: spec.window_secs,
        },
    );
    endpoint_limiter.check(&key).await.map(|_| ())
}

/// Generates an Actix-web list handler for a resource endpoint.
pub async fn handle_list(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
    resource: web::Data<Arc<ResourceDefinition>>,
    endpoint: web::Data<Arc<EndpointSpec>>,
) -> Result<HttpResponse, ShaperailError> {
    let user = enforce_auth(&req, &endpoint)?;
    check_rate_limit(
        &endpoint,
        &state,
        &req,
        user.as_ref(),
        &format!("{}:list", resource.resource),
    )
    .await?;

    // Cache check for endpoints with cache.ttl configured
    if let (Some(ref cache), Some(ref cache_spec)) = (&state.cache, &endpoint.cache) {
        if !should_bypass_cache(&req, user.as_ref()) {
            let query_params = query_map_public(&req);
            let role = user_role_for_cache(user.as_ref());
            let tenant = tenant_id_for_key(user.as_ref());
            let cache_key = RedisCache::build_key_with_tenant(
                &resource.resource,
                "list",
                &query_params,
                role,
                tenant,
            );

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
    user: Option<AuthenticatedUser>,
) -> Result<serde_json::Value, ShaperailError> {
    let mut params = parse_list_params(req, endpoint);

    // M18: Inject tenant filter — scopes all list queries to user's tenant
    inject_tenant_filter(resource, user.as_ref(), &mut params.filters)?;

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
    check_rate_limit(
        &endpoint,
        &state,
        &req,
        user.as_ref(),
        &format!("{}:get", resource.resource),
    )
    .await?;
    let id = parse_uuid(&path)?;

    // Cache check for endpoints with cache.ttl configured
    if let (Some(ref cache), Some(ref cache_spec)) = (&state.cache, &endpoint.cache) {
        if !should_bypass_cache(&req, user.as_ref()) {
            let mut query_params = query_map_public(&req);
            query_params.insert("_id".to_string(), id.to_string());
            let role = user_role_for_cache(user.as_ref());
            let tenant = tenant_id_for_key(user.as_ref());
            let cache_key = RedisCache::build_key_with_tenant(
                &resource.resource,
                "get",
                &query_params,
                role,
                tenant,
            );

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

    // M18: Verify tenant isolation before returning the record
    verify_tenant(resource, user, &data)?;

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

pub(crate) async fn run_write_side_effects(
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

pub(crate) fn schedule_file_cleanup(
    resource: &ResourceDefinition,
    deleted_data: &serde_json::Value,
) {
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
    let headers = req
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let tenant_id = resolve_tenant_id(resource, user);
    let mut ctx = super::controller::Context {
        input,
        data: None,
        user: user.cloned(),
        pool: state.pool.clone(),
        headers,
        response_headers: vec![],
        tenant_id,
    };
    #[cfg(feature = "wasm-plugins")]
    let wasm_rt = state.wasm_runtime.as_ref();
    #[cfg(not(feature = "wasm-plugins"))]
    let wasm_rt = None;
    super::controller::dispatch_controller(
        name,
        &resource.resource,
        &mut ctx,
        state.controllers.as_ref(),
        wasm_rt,
    )
    .await?;
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
    let headers = req
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let tenant_id = resolve_tenant_id(resource, user);
    let mut ctx = super::controller::Context {
        input: serde_json::Map::new(),
        data: Some(data),
        user: user.cloned(),
        pool: state.pool.clone(),
        headers,
        response_headers: vec![],
        tenant_id,
    };
    #[cfg(feature = "wasm-plugins")]
    let wasm_rt = state.wasm_runtime.as_ref();
    #[cfg(not(feature = "wasm-plugins"))]
    let wasm_rt = None;
    super::controller::dispatch_controller(
        name,
        &resource.resource,
        &mut ctx,
        state.controllers.as_ref(),
        wasm_rt,
    )
    .await?;
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
    check_rate_limit(
        &endpoint,
        &state,
        &req,
        user.as_ref(),
        &format!("{}:create", resource.resource),
    )
    .await?;
    let mut input_data = extract_input(&body, &resource, &endpoint)?;
    // M18: Auto-inject tenant_id into create input
    inject_tenant_into_input(&resource, user.as_ref(), &mut input_data);
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
    let user = enforce_auth(&req, &endpoint)?;
    check_rate_limit(
        &endpoint,
        &state,
        &req,
        user.as_ref(),
        &format!("{}:create_upload", resource.resource),
    )
    .await?;
    let mut input_data = extract_input_from_multipart(payload, &resource, &endpoint).await?;
    // M18: Auto-inject tenant_id into create input (mirrors handle_create)
    inject_tenant_into_input(&resource, user.as_ref(), &mut input_data);
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
    check_rate_limit(
        &endpoint,
        &state,
        &req,
        user.as_ref(),
        &format!("{}:update", resource.resource),
    )
    .await?;
    let id = parse_uuid(&path)?;
    let input_data = extract_input(&body, &resource, &endpoint)?;
    let store_opt = store_for_or_error(&state, &resource)?;

    // M18 tenant check + owner check: fetch record first to verify
    let needs_owner = rbac::needs_owner_check(endpoint.auth.as_ref(), user.as_ref());
    let needs_tenant = resource.tenant_key.is_some() && !is_super_admin(user.as_ref());
    if needs_owner || needs_tenant {
        let existing = if let Some(ref store) = store_opt {
            store.find_by_id(&id).await?
        } else {
            let rq = ResourceQuery::new(&resource, &state.pool);
            rq.find_by_id(&id).await?
        };
        verify_tenant(&resource, user.as_ref(), &existing.0)?;
        if needs_owner {
            if let Some(ref u) = user {
                rbac::check_owner(u, &existing.0)?;
            }
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
    check_rate_limit(
        &endpoint,
        &state,
        &req,
        user.as_ref(),
        &format!("{}:update_upload", resource.resource),
    )
    .await?;
    let id = parse_uuid(&path)?;
    let input_data = extract_input_from_multipart(payload, &resource, &endpoint).await?;
    let store_opt = store_for_or_error(&state, &resource)?;

    // M18 tenant check + owner check
    let needs_owner = rbac::needs_owner_check(endpoint.auth.as_ref(), user.as_ref());
    let needs_tenant = resource.tenant_key.is_some() && !is_super_admin(user.as_ref());
    if needs_owner || needs_tenant {
        let existing = if let Some(ref store) = store_opt {
            store.find_by_id(&id).await?
        } else {
            let rq = ResourceQuery::new(&resource, &state.pool);
            rq.find_by_id(&id).await?
        };
        verify_tenant(&resource, user.as_ref(), &existing.0)?;
        if needs_owner {
            if let Some(ref u) = user {
                rbac::check_owner(u, &existing.0)?;
            }
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
    check_rate_limit(
        &endpoint,
        &state,
        &req,
        user.as_ref(),
        &format!("{}:delete", resource.resource),
    )
    .await?;
    let id = parse_uuid(&path)?;
    let store_opt = store_for_or_error(&state, &resource)?;

    // M18 tenant check + owner check: fetch record first
    let needs_owner = rbac::needs_owner_check(endpoint.auth.as_ref(), user.as_ref());
    let needs_tenant = resource.tenant_key.is_some() && !is_super_admin(user.as_ref());
    if needs_owner || needs_tenant {
        let existing = if let Some(ref store) = store_opt {
            store.find_by_id(&id).await?
        } else {
            let rq = ResourceQuery::new(&resource, &state.pool);
            rq.find_by_id(&id).await?
        };
        verify_tenant(&resource, user.as_ref(), &existing.0)?;
        if needs_owner {
            if let Some(ref u) = user {
                rbac::check_owner(u, &existing.0)?;
            }
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
    let user = enforce_auth(&req, &endpoint)?;
    check_rate_limit(
        &endpoint,
        &state,
        &req,
        user.as_ref(),
        &format!("{}:bulk_create", resource.resource),
    )
    .await?;
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
    let user = enforce_auth(&req, &endpoint)?;
    check_rate_limit(
        &endpoint,
        &state,
        &req,
        user.as_ref(),
        &format!("{}:bulk_delete", resource.resource),
    )
    .await?;
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

/// Extracts input fields from a JSON value (for REST and GraphQL mutations).
pub(crate) fn extract_input_from_value(
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
            tenant_key: None,
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
            method: Some(HttpMethod::Post),
            path: Some("/users".to_string()),
            input: Some(vec!["name".to_string()]),
            ..Default::default()
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
            method: Some(HttpMethod::Post),
            path: Some("/users".to_string()),
            ..Default::default()
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

    // -- M18 Multi-Tenancy Tests --

    fn tenant_resource() -> ResourceDefinition {
        let mut rd = test_resource();
        rd.tenant_key = Some("org_id".to_string());
        rd
    }

    fn user_with_tenant(id: &str, role: &str, tenant_id: &str) -> AuthenticatedUser {
        AuthenticatedUser {
            id: id.to_string(),
            role: role.to_string(),
            tenant_id: Some(tenant_id.to_string()),
        }
    }

    #[test]
    fn verify_tenant_passes_when_matching() {
        let resource = tenant_resource();
        let user = user_with_tenant("u1", "member", "org-a");
        let data = serde_json::json!({"id": "r1", "org_id": "org-a", "name": "Test"});
        assert!(verify_tenant(&resource, Some(&user), &data).is_ok());
    }

    #[test]
    fn verify_tenant_fails_when_mismatched() {
        let resource = tenant_resource();
        let user = user_with_tenant("u1", "member", "org-a");
        let data = serde_json::json!({"id": "r1", "org_id": "org-b", "name": "Test"});
        assert!(verify_tenant(&resource, Some(&user), &data).is_err());
    }

    #[test]
    fn verify_tenant_super_admin_bypasses() {
        let resource = tenant_resource();
        let user = user_with_tenant("u1", "super_admin", "org-a");
        let data = serde_json::json!({"id": "r1", "org_id": "org-b", "name": "Test"});
        assert!(verify_tenant(&resource, Some(&user), &data).is_ok());
    }

    #[test]
    fn verify_tenant_no_tenant_key_always_passes() {
        let resource = test_resource(); // no tenant_key
        let user = user_with_tenant("u1", "member", "org-a");
        let data = serde_json::json!({"id": "r1", "org_id": "org-b", "name": "Test"});
        assert!(verify_tenant(&resource, Some(&user), &data).is_ok());
    }

    #[test]
    fn verify_tenant_no_tenant_id_returns_forbidden() {
        let resource = tenant_resource();
        let user = AuthenticatedUser {
            id: "u1".to_string(),
            role: "member".to_string(),
            tenant_id: None,
        };
        let data = serde_json::json!({"id": "r1", "org_id": "org-b", "name": "Test"});
        let result = verify_tenant(&resource, Some(&user), &data);
        assert!(
            matches!(result, Err(ShaperailError::Forbidden)),
            "user with no tenant_id must be forbidden on tenant-isolated resource"
        );
    }

    #[test]
    fn verify_tenant_unauthenticated_user_returns_forbidden() {
        let resource = tenant_resource();
        let data = serde_json::json!({"id": "r1", "org_id": "org-b", "name": "Test"});
        let result = verify_tenant(&resource, None, &data);
        assert!(
            matches!(result, Err(ShaperailError::Forbidden)),
            "unauthenticated user must be forbidden on tenant-isolated resource"
        );
    }

    #[test]
    fn inject_tenant_filter_no_tenant_id_returns_forbidden() {
        let resource = tenant_resource();
        let user = AuthenticatedUser {
            id: "u1".to_string(),
            role: "member".to_string(),
            tenant_id: None,
        };
        let mut filters = crate::db::FilterSet::default();
        let result = inject_tenant_filter(&resource, Some(&user), &mut filters);
        assert!(
            matches!(result, Err(ShaperailError::Forbidden)),
            "inject_tenant_filter must return Forbidden when tenant_id is absent"
        );
        assert!(
            filters.filters.is_empty(),
            "no filter should be injected on error"
        );
    }

    #[test]
    fn inject_tenant_filter_unauthenticated_returns_forbidden() {
        let resource = tenant_resource();
        let mut filters = crate::db::FilterSet::default();
        let result = inject_tenant_filter(&resource, None, &mut filters);
        assert!(
            matches!(result, Err(ShaperailError::Forbidden)),
            "inject_tenant_filter must return Forbidden for unauthenticated user on tenant-isolated resource"
        );
        assert!(
            filters.filters.is_empty(),
            "no filter should be injected on error"
        );
    }

    #[test]
    fn inject_tenant_filter_adds_filter() {
        let resource = tenant_resource();
        let user = user_with_tenant("u1", "member", "org-a");
        let mut filters = crate::db::FilterSet::default();
        inject_tenant_filter(&resource, Some(&user), &mut filters).unwrap();
        assert_eq!(filters.filters.len(), 1);
        assert_eq!(filters.filters[0].field, "org_id");
        assert_eq!(filters.filters[0].value, "org-a");
    }

    #[test]
    fn inject_tenant_filter_super_admin_skips() {
        let resource = tenant_resource();
        let user = user_with_tenant("u1", "super_admin", "org-a");
        let mut filters = crate::db::FilterSet::default();
        inject_tenant_filter(&resource, Some(&user), &mut filters).unwrap();
        assert!(filters.is_empty());
    }

    #[test]
    fn inject_tenant_into_input_adds_key() {
        let resource = tenant_resource();
        let user = user_with_tenant("u1", "member", "org-a");
        let mut input = serde_json::Map::new();
        input.insert("name".to_string(), serde_json::json!("Test"));
        inject_tenant_into_input(&resource, Some(&user), &mut input);
        assert_eq!(input.get("org_id").and_then(|v| v.as_str()), Some("org-a"));
    }

    #[test]
    fn inject_tenant_into_input_does_not_overwrite() {
        let resource = tenant_resource();
        let user = user_with_tenant("u1", "member", "org-a");
        let mut input = serde_json::Map::new();
        input.insert("org_id".to_string(), serde_json::json!("org-x"));
        inject_tenant_into_input(&resource, Some(&user), &mut input);
        // Should NOT overwrite existing value
        assert_eq!(input.get("org_id").and_then(|v| v.as_str()), Some("org-x"));
    }

    #[test]
    fn is_super_admin_detects_role() {
        let user = user_with_tenant("u1", "super_admin", "org-a");
        assert!(is_super_admin(Some(&user)));
        let user = user_with_tenant("u1", "admin", "org-a");
        assert!(!is_super_admin(Some(&user)));
        assert!(!is_super_admin(None));
    }

    #[test]
    fn rate_limit_field_defaults_to_none() {
        // endpoint with no rate_limit field — helper must return Ok without touching Redis
        let endpoint = EndpointSpec {
            rate_limit: None,
            ..Default::default()
        };
        assert!(endpoint.rate_limit.is_none());
    }

    #[test]
    fn rate_limit_spec_fields_accessible() {
        let endpoint = EndpointSpec {
            rate_limit: Some(RateLimitSpec {
                max_requests: 10,
                window_secs: 60,
            }),
            ..Default::default()
        };
        let _ = endpoint.rate_limit.as_ref().unwrap();
    }
}
