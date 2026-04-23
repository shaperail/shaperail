use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use serde::Deserialize;

#[path = "../generated/mod.rs"]
mod generated;
mod runtime_extensions;

use runtime_extensions::{
    build_api_key_store, build_controller_map, build_job_registry, incident_channel_definition,
};
use shaperail_runtime::auth::jwt::JwtConfig;
use shaperail_runtime::cache::{create_redis_pool, RedisCache};
use shaperail_runtime::events::{configure_inbound_routes, EventEmitter, WebhookDispatcher};
use shaperail_runtime::handlers::{register_all_resources, AppState};
use shaperail_runtime::jobs::{JobQueue, Worker};
use shaperail_runtime::observability::{
    health_handler, health_ready_handler, metrics_handler, sensitive_fields, HealthState,
    MetricsState, RequestLogger,
};
use shaperail_runtime::ws::{configure_ws_routes, RedisPubSub, RoomManager};

type DevWebhookSink = Arc<tokio::sync::RwLock<Vec<serde_json::Value>>>;

#[derive(Debug, Deserialize)]
struct DevTokenQuery {
    user_id: String,
    role: String,
    tenant_id: Option<String>,
}

fn io_error(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

async fn openapi_json_handler(spec: web::Data<Arc<String>>) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .content_type("application/json")
        .body(spec.get_ref().as_ref().clone())
}

async fn docs_handler() -> HttpResponse {
    HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .content_type("text/plain; charset=utf-8")
        .body(
            "OpenAPI: /openapi.json\nGraphQL Playground: /graphql/playground\nDev token helper: /dev/token?user_id=<uuid>&role=<role>&tenant_id=<uuid>\nOutbound webhook sink: /dev/webhook-sink",
        )
}

async fn dev_token_handler(
    jwt: web::Data<Arc<JwtConfig>>,
    query: web::Query<DevTokenQuery>,
) -> HttpResponse {
    match jwt.encode_access_with_tenant(&query.user_id, &query.role, query.tenant_id.as_deref()) {
        Ok(token) => HttpResponse::Ok()
            .insert_header(("Cache-Control", "no-store"))
            .content_type("text/plain; charset=utf-8")
            .body(token),
        Err(error) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to issue development token: {error}")
        })),
    }
}

async fn dev_webhook_sink_handler(
    request: HttpRequest,
    body: web::Bytes,
    sink: web::Data<DevWebhookSink>,
) -> HttpResponse {
    let payload = serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_else(|_| {
        serde_json::json!({
            "raw_body": String::from_utf8_lossy(&body).to_string(),
        })
    });
    let signature = request
        .headers()
        .get("X-Shaperail-Signature")
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);

    sink.write().await.push(serde_json::json!({
        "received_at": chrono::Utc::now().to_rfc3339(),
        "signature": signature,
        "payload": payload,
    }));

    HttpResponse::Accepted().json(serde_json::json!({
        "received": true,
    }))
}

async fn dev_webhook_sink_list_handler(sink: web::Data<DevWebhookSink>) -> HttpResponse {
    let deliveries = sink.read().await.clone();
    HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(serde_json::json!({
            "count": deliveries.len(),
            "items": deliveries,
        }))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter("info").init();

    let config_path = Path::new("shaperail.config.yaml");
    let config =
        shaperail_codegen::config_parser::parse_config_file(config_path).map_err(|error| {
            io_error(format!(
                "Failed to parse {}: {error}",
                config_path.display()
            ))
        })?;

    let resources_dir = Path::new("resources");
    let mut resources = Vec::new();
    for entry in std::fs::read_dir(resources_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "yaml")
        {
            let resource =
                shaperail_codegen::parser::parse_resource_file(&path).map_err(|error| {
                    io_error(format!("Failed to parse {}: {error}", path.display()))
                })?;
            let validation_errors = shaperail_codegen::validator::validate_resource(&resource);
            if !validation_errors.is_empty() {
                let rendered = validation_errors
                    .into_iter()
                    .map(|error| error.to_string())
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(io_error(format!("{}: {rendered}", path.display())));
            }
            resources.push(resource);
        }
    }

    let openapi_spec = shaperail_codegen::openapi::generate(&config, &resources);
    let openapi_json = Arc::new(
        shaperail_codegen::openapi::to_json(&openapi_spec)
            .map_err(|error| io_error(format!("Failed to serialize OpenAPI spec: {error}")))?,
    );

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| io_error("DATABASE_URL must be set before running the incident example"))?;
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .map_err(|error| io_error(format!("Failed to connect to database: {error}")))?;
    let migrator = sqlx::migrate::Migrator::new(Path::new("./migrations"))
        .await
        .map_err(|error| io_error(format!("Failed to load migrations: {error}")))?;
    migrator
        .run(&pool)
        .await
        .map_err(|error| io_error(format!("Failed to apply migrations: {error}")))?;

    let stores = generated::build_store_registry(pool.clone());

    let redis_pool = config.cache.as_ref().map(|cache_config| {
        create_redis_pool(&cache_config.url)
            .map(Arc::new)
            .map_err(|error| io_error(format!("Failed to create Redis pool: {error}")))
    });
    let redis_pool = match redis_pool {
        Some(result) => Some(result?),
        None => None,
    };

    let cache = redis_pool
        .as_ref()
        .map(|pool| RedisCache::new(pool.clone()));
    let job_queue = redis_pool.as_ref().map(|pool| JobQueue::new(pool.clone()));
    let event_emitter = job_queue
        .clone()
        .map(|queue| EventEmitter::new(queue, config.events.as_ref()));
    let jwt_config = JwtConfig::from_env().map(Arc::new);
    let controllers = build_controller_map();
    let api_keys = web::Data::new(Arc::new(build_api_key_store()));

    let room_manager = redis_pool.as_ref().map(|_| RoomManager::new());
    let ws_pubsub = redis_pool
        .as_ref()
        .map(|pool| RedisPubSub::new(pool.clone()));
    let ws_channel = incident_channel_definition();
    let mut _worker_shutdown_tx = None;

    if let (Some(pubsub), Some(room_manager), Some(cache_config)) = (
        ws_pubsub.clone(),
        room_manager.clone(),
        config.cache.as_ref(),
    ) {
        let _ws_subscriber_handle =
            pubsub.start_subscriber(&ws_channel.channel, room_manager, &cache_config.url);
    }

    let webhook_dispatcher = config
        .events
        .as_ref()
        .and_then(|events| events.webhooks.as_ref())
        .and_then(|webhooks| {
            WebhookDispatcher::from_env(&webhooks.secret_env, webhooks.timeout_secs).ok()
        });

    if let Some(job_queue) = job_queue.clone() {
        let registry = build_job_registry(
            pool.clone(),
            cache.clone(),
            room_manager.clone(),
            ws_pubsub.clone(),
            webhook_dispatcher,
        );
        let worker = Worker::new(job_queue, registry, Duration::from_secs(1));
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        _worker_shutdown_tx = Some(shutdown_tx);
        let _worker_handle = worker.spawn(shutdown_rx);
    }

    let metrics_state = web::Data::new(
        MetricsState::new()
            .map_err(|error| io_error(format!("Failed to initialize metrics: {error}")))?,
    );

    let rate_limiter = redis_pool.as_ref().map(|pool| {
        Arc::new(shaperail_runtime::auth::RateLimiter::new(
            pool.clone(),
            shaperail_runtime::auth::RateLimitConfig::default(),
        ))
    });
    let state = Arc::new(AppState {
        pool: pool.clone(),
        resources: resources.clone(),
        stores: Some(stores),
        controllers: Some(controllers),
        jwt_config: jwt_config.clone(),
        cache,
        event_emitter: event_emitter.clone(),
        job_queue,
        rate_limiter,
        custom_handlers: None,
        metrics: Some(metrics_state.get_ref().clone()),
        saga_executor: None,
        #[cfg(feature = "wasm-plugins")]
        wasm_runtime: None,
        event_bus: tokio::sync::broadcast::channel(256).0,
    });

    let health_state = web::Data::new(HealthState::new(Some(pool), redis_pool));
    let inbound_configs = config
        .events
        .as_ref()
        .map(|events| events.inbound.clone())
        .unwrap_or_default();
    let dev_webhook_sink: DevWebhookSink = Arc::new(tokio::sync::RwLock::new(Vec::new()));

    #[cfg(feature = "graphql")]
    let graphql_schema = if config
        .protocols
        .iter()
        .any(|protocol| protocol == "graphql")
    {
        Some(
            shaperail_runtime::graphql::build_schema(&resources, state.clone())
                .map_err(|error| io_error(error.to_string()))?,
        )
    } else {
        None
    };

    #[cfg(feature = "graphql")]
    let graphql_schema_clone = graphql_schema.clone();

    #[cfg(feature = "grpc")]
    if config.protocols.iter().any(|protocol| protocol == "grpc") {
        let grpc_config = config.grpc.as_ref();
        let _grpc_handle = shaperail_runtime::grpc::build_grpc_server(
            state.clone(),
            resources.clone(),
            jwt_config.clone(),
            grpc_config,
        )
        .await
        .map_err(|error| io_error(error.to_string()))?;
    }

    let state_clone = state.clone();
    let resources_clone = resources.clone();
    let health_state_clone = health_state.clone();
    let metrics_state_clone = metrics_state.clone();
    let jwt_config_clone = jwt_config.clone();
    let openapi_json_clone = openapi_json.clone();
    let event_emitter_clone = event_emitter.clone();
    let room_manager_clone = room_manager.clone();
    let ws_pubsub_clone = ws_pubsub.clone();
    let inbound_configs_clone = inbound_configs.clone();
    let ws_channel_clone = ws_channel.clone();
    let dev_webhook_sink_clone = dev_webhook_sink.clone();

    HttpServer::new(move || {
        let app_state = state_clone.clone();
        let resources = resources_clone.clone();
        let spec = openapi_json_clone.clone();
        let sensitive = sensitive_fields(&resources);
        let mut app = App::new()
            .wrap(RequestLogger::new(sensitive))
            .app_data(web::Data::new(app_state.clone()))
            .app_data(web::Data::new(spec))
            .app_data(api_keys.clone())
            .app_data(web::Data::new(dev_webhook_sink_clone.clone()))
            .app_data(health_state_clone.clone())
            .app_data(metrics_state_clone.clone())
            .route("/health", web::get().to(health_handler))
            .route("/health/ready", web::get().to(health_ready_handler))
            .route("/metrics", web::get().to(metrics_handler))
            .route("/openapi.json", web::get().to(openapi_json_handler))
            .route("/docs", web::get().to(docs_handler))
            .route(
                "/dev/webhook-sink",
                web::post().to(dev_webhook_sink_handler),
            )
            .route(
                "/dev/webhook-sink",
                web::get().to(dev_webhook_sink_list_handler),
            );

        if let Some(ref jwt) = jwt_config_clone {
            app = app
                .app_data(web::Data::new(jwt.clone()))
                .route("/dev/token", web::get().to(dev_token_handler));
        }

        #[cfg(feature = "graphql")]
        if let Some(ref schema) = graphql_schema_clone {
            app = app
                .app_data(web::Data::new(schema.clone()))
                .route(
                    "/graphql",
                    web::post().to(shaperail_runtime::graphql::graphql_handler),
                )
                .route(
                    "/graphql/playground",
                    web::get().to(shaperail_runtime::graphql::playground_handler),
                );
        }

        app.configure(|cfg| {
            if let Some(ref emitter) = event_emitter_clone {
                if !inbound_configs_clone.is_empty() {
                    configure_inbound_routes(cfg, &inbound_configs_clone, emitter);
                }
            }

            if let (Some(room_manager), Some(pubsub), Some(jwt)) = (
                room_manager_clone.clone(),
                ws_pubsub_clone.clone(),
                jwt_config_clone.clone(),
            ) {
                configure_ws_routes(cfg, ws_channel_clone.clone(), room_manager, pubsub, jwt);
            }

            register_all_resources(cfg, &resources, app_state.clone());
        })
    })
    .bind(("0.0.0.0", config.port))?
    .run()
    .await
}
