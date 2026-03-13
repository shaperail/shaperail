//! API-level integration tests for the shaperail-runtime handler stack.
//!
//! Uses `#[sqlx::test]` macro for auto-rollback and isolated DB per test.
//! Requires a running PostgreSQL instance.
//! Set DATABASE_URL env var or run `docker compose up -d` first.
//!
//! Run with: cargo test -p shaperail-runtime --test api_integration

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use actix_web::{test as actix_test, web, App};
use async_trait::async_trait;
use deadpool_redis::Pool;
use indexmap::IndexMap;
use redis::AsyncCommands;
use serde_json::json;
use shaperail_core::{
    AuthRule, CacheSpec, EndpointSpec, FieldSchema, FieldType, HttpMethod, PaginationStyle,
    RelationSpec, RelationType, ResourceDefinition,
};
use shaperail_runtime::auth::jwt::JwtConfig;
use shaperail_runtime::cache::{create_redis_pool, RedisCache};
use shaperail_runtime::db::{
    FilterSet, PageRequest, ResourceQuery, ResourceRow, ResourceStore, SortParam, StoreRegistry,
};
use shaperail_runtime::handlers::crud::AppState;
use shaperail_runtime::handlers::routes::register_resource;
use shaperail_runtime::observability::{metrics_handler, MetricsState, RequestLogger};
use sqlx::Row;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Shared JWT config for auth tests.
fn test_jwt() -> JwtConfig {
    JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400)
}

/// Builds a minimal `AppState` with the given pool and optional JWT config.
fn make_state(pool: sqlx::PgPool, jwt: Option<JwtConfig>) -> Arc<AppState> {
    Arc::new(AppState {
        pool,
        resources: vec![],
        stores: None,
        jwt_config: jwt.map(Arc::new),
        cache: None,
        event_emitter: None,
        job_queue: None,
        metrics: Some(MetricsState::new().expect("metrics state")),
    })
}

fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

fn redis_test_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn storage_test_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn clear_resource_cache(pool: &Pool, resource: &str) {
    let mut conn = pool.get().await.expect("redis connection");
    let keys: Vec<String> = redis::cmd("KEYS")
        .arg(format!("shaperail:{resource}:*"))
        .query_async(&mut conn)
        .await
        .unwrap_or_default();

    if !keys.is_empty() {
        let _: usize = conn.del(keys).await.expect("delete cache keys");
    }
}

fn test_asset_resource() -> ResourceDefinition {
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
        "title".to_string(),
        FieldSchema {
            field_type: FieldType::String,
            primary: false,
            generated: false,
            required: true,
            unique: false,
            nullable: false,
            reference: None,
            min: Some(json!(1)),
            max: Some(json!(200)),
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: false,
            items: None,
        },
    );
    schema.insert(
        "attachment".to_string(),
        FieldSchema {
            field_type: FieldType::File,
            primary: false,
            generated: false,
            required: true,
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
        "attachment_filename".to_string(),
        FieldSchema {
            field_type: FieldType::String,
            primary: false,
            generated: false,
            required: false,
            unique: false,
            nullable: true,
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
        "attachment_mime_type".to_string(),
        FieldSchema {
            field_type: FieldType::String,
            primary: false,
            generated: false,
            required: false,
            unique: false,
            nullable: true,
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
        "attachment_size".to_string(),
        FieldSchema {
            field_type: FieldType::Bigint,
            primary: false,
            generated: false,
            required: false,
            unique: false,
            nullable: true,
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
        "created_at".to_string(),
        FieldSchema {
            field_type: FieldType::Timestamp,
            primary: false,
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
        "updated_at".to_string(),
        FieldSchema {
            field_type: FieldType::Timestamp,
            primary: false,
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

    let mut endpoints = IndexMap::new();
    endpoints.insert(
        "create".to_string(),
        EndpointSpec {
            method: HttpMethod::Post,
            path: "/test_assets".to_string(),
            auth: None,
            input: Some(vec!["title".to_string(), "attachment".to_string()]),
            filters: None,
            search: None,
            pagination: None,
            sort: None,
            cache: None,
            hooks: None,
            events: None,
            jobs: None,
            upload: Some(shaperail_core::UploadSpec {
                field: "attachment".to_string(),
                storage: "local".to_string(),
                max_size: "1mb".to_string(),
                types: Some(vec!["text/plain".to_string()]),
            }),
            soft_delete: false,
        },
    );
    endpoints.insert(
        "delete".to_string(),
        EndpointSpec {
            method: HttpMethod::Delete,
            path: "/test_assets/:id".to_string(),
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
        },
    );

    ResourceDefinition {
        resource: "test_assets".to_string(),
        version: 1,
        schema,
        endpoints: Some(endpoints),
        relations: None,
        indexes: None,
    }
}

fn multipart_body(
    fields: &[(&str, &str)],
    file_field: &str,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
) -> (String, Vec<u8>) {
    let boundary = "shaperail-boundary";
    let mut body = Vec::new();

    for (name, value) in fields {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }

    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{file_field}\"; filename=\"{filename}\"\r\nContent-Type: {mime_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    (boundary.to_string(), body)
}

/// Returns a full `ResourceDefinition` matching the test_users migration.
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
            max: Some(json!(255)),
            format: Some("email".to_string()),
            values: None,
            default: None,
            sensitive: false,
            search: true,
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
            min: Some(json!(1)),
            max: Some(json!(200)),
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: true,
            items: None,
        },
    );
    schema.insert(
        "role".to_string(),
        FieldSchema {
            field_type: FieldType::Enum,
            primary: false,
            generated: false,
            required: true,
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
            default: Some(json!("member")),
            sensitive: false,
            search: false,
            items: None,
        },
    );
    schema.insert(
        "org_id".to_string(),
        FieldSchema {
            field_type: FieldType::Uuid,
            primary: false,
            generated: false,
            required: true,
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
        "created_at".to_string(),
        FieldSchema {
            field_type: FieldType::Timestamp,
            primary: false,
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
        "updated_at".to_string(),
        FieldSchema {
            field_type: FieldType::Timestamp,
            primary: false,
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
        "deleted_at".to_string(),
        FieldSchema {
            field_type: FieldType::Timestamp,
            primary: false,
            generated: false,
            required: false,
            unique: false,
            nullable: true,
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

    ResourceDefinition {
        resource: "test_users".to_string(),
        version: 1,
        schema,
        endpoints: None,
        relations: None,
        indexes: None,
    }
}

/// Builds the standard CRUD endpoints for the test resource.
fn full_crud_endpoints() -> IndexMap<String, EndpointSpec> {
    let mut eps = IndexMap::new();

    eps.insert(
        "list".to_string(),
        EndpointSpec {
            method: HttpMethod::Get,
            path: "/test_users".to_string(),
            auth: None,
            input: None,
            filters: Some(vec!["role".to_string(), "org_id".to_string()]),
            search: Some(vec!["name".to_string(), "email".to_string()]),
            pagination: Some(PaginationStyle::Cursor),
            sort: None,
            cache: None,
            hooks: None,
            events: None,
            jobs: None,
            upload: None,
            soft_delete: false,
        },
    );

    eps.insert(
        "get".to_string(),
        EndpointSpec {
            method: HttpMethod::Get,
            path: "/test_users/:id".to_string(),
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
        },
    );

    eps.insert(
        "create".to_string(),
        EndpointSpec {
            method: HttpMethod::Post,
            path: "/test_users".to_string(),
            auth: None,
            input: Some(vec![
                "email".to_string(),
                "name".to_string(),
                "role".to_string(),
                "org_id".to_string(),
            ]),
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
        },
    );

    eps.insert(
        "update".to_string(),
        EndpointSpec {
            method: HttpMethod::Patch,
            path: "/test_users/:id".to_string(),
            auth: None,
            input: Some(vec!["name".to_string(), "role".to_string()]),
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
        },
    );

    eps.insert(
        "delete".to_string(),
        EndpointSpec {
            method: HttpMethod::Delete,
            path: "/test_users/:id".to_string(),
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
            soft_delete: true,
        },
    );

    eps
}

/// Builds a user payload with given email, name, role, and org_id.
fn user_payload(email: &str, name: &str, role: &str, org_id: &str) -> serde_json::Value {
    json!({
        "email": email,
        "name": name,
        "role": role,
        "org_id": org_id,
    })
}

// ---------------------------------------------------------------------------
// 1. Full CRUD cycle
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_full_crud_cycle(pool: sqlx::PgPool) {
    let mut resource = test_resource();
    resource.endpoints = Some(full_crud_endpoints());
    let state = make_state(pool, None);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();

    // CREATE
    let req = actix_test::TestRequest::post()
        .uri("/test_users")
        .set_json(user_payload(
            "crud@example.com",
            "CRUD User",
            "admin",
            &org_id,
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201, "Create should return 201");
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let id = body["data"]["id"]
        .as_str()
        .expect("Created record should have id");
    assert_eq!(body["data"]["name"], "CRUD User");

    // GET by ID
    let req = actix_test::TestRequest::get()
        .uri(&format!("/test_users/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["data"]["email"], "crud@example.com");

    // UPDATE name
    let req = actix_test::TestRequest::patch()
        .uri(&format!("/test_users/{id}"))
        .set_json(json!({"name": "Updated Name"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Updated Name");

    // SOFT DELETE
    let req = actix_test::TestRequest::delete()
        .uri(&format!("/test_users/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    // GET after soft delete should return 404
    let req = actix_test::TestRequest::get()
        .uri(&format!("/test_users/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404, "Soft-deleted record should return 404");
}

// ---------------------------------------------------------------------------
// 2. List with filters
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_list_with_filters(pool: sqlx::PgPool) {
    let mut resource = test_resource();
    resource.endpoints = Some(full_crud_endpoints());
    let state = make_state(pool, None);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();

    // Insert 3 users with different roles
    for (email, role) in [
        ("admin@example.com", "admin"),
        ("member@example.com", "member"),
        ("viewer@example.com", "viewer"),
    ] {
        let req = actix_test::TestRequest::post()
            .uri("/test_users")
            .set_json(user_payload(email, "Test User", role, &org_id))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    // Filter by role=admin
    let req = actix_test::TestRequest::get()
        .uri("/test_users?filter%5Brole%5D=admin")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let data = body["data"].as_array().expect("data should be an array");
    assert!(
        !data.is_empty(),
        "Should have at least one admin in results"
    );
    for item in data {
        assert_eq!(
            item["role"], "admin",
            "All filtered results should be admin"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. List with cursor pagination
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_list_with_pagination(pool: sqlx::PgPool) {
    let mut resource = test_resource();
    resource.endpoints = Some(full_crud_endpoints());
    let state = make_state(pool, None);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();

    // Insert 5 users
    for i in 0..5 {
        let req = actix_test::TestRequest::post()
            .uri("/test_users")
            .set_json(user_payload(
                &format!("page{i}@example.com"),
                &format!("User {i}"),
                "member",
                &org_id,
            ))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    // First page: limit=2
    let req = actix_test::TestRequest::get()
        .uri("/test_users?limit=2")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let page1 = body["data"].as_array().expect("data array");
    assert_eq!(page1.len(), 2, "First page should have 2 items");
    assert_eq!(body["meta"]["has_more"], true, "Should have more pages");
    let cursor = body["meta"]["cursor"]
        .as_str()
        .expect("Should have a cursor");

    // Second page: use cursor
    let req = actix_test::TestRequest::get()
        .uri(&format!("/test_users?limit=2&after={cursor}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body2: serde_json::Value = actix_test::read_body_json(resp).await;
    let page2 = body2["data"].as_array().expect("data array");
    assert_eq!(page2.len(), 2, "Second page should have 2 items");

    // No overlap between pages
    let ids1: Vec<&str> = page1.iter().filter_map(|r| r["id"].as_str()).collect();
    let ids2: Vec<&str> = page2.iter().filter_map(|r| r["id"].as_str()).collect();
    for id in &ids2 {
        assert!(!ids1.contains(id), "Pages should not overlap: {id}");
    }
}

// ---------------------------------------------------------------------------
// 4. Validation rejects missing required field
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_validation_rejects_missing_required_field(pool: sqlx::PgPool) {
    let mut resource = test_resource();
    resource.endpoints = Some(full_crud_endpoints());
    let state = make_state(pool, None);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    // Missing "name" (required, no default)
    let req = actix_test::TestRequest::post()
        .uri("/test_users")
        .set_json(json!({
            "email": "noname@example.com",
            "role": "member",
            "org_id": uuid::Uuid::new_v4().to_string(),
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        422,
        "Missing required field should return 422"
    );
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    let details = body["error"]["details"]
        .as_array()
        .expect("details should be an array");
    assert!(
        details.iter().any(|e| e["field"] == "name"),
        "Error details should mention 'name' field"
    );
}

// ---------------------------------------------------------------------------
// 5. Validation rejects invalid enum value
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_validation_rejects_invalid_enum(pool: sqlx::PgPool) {
    let mut resource = test_resource();
    resource.endpoints = Some(full_crud_endpoints());
    let state = make_state(pool, None);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let req = actix_test::TestRequest::post()
        .uri("/test_users")
        .set_json(json!({
            "email": "invalid-role@example.com",
            "name": "Bad Role",
            "role": "superuser",
            "org_id": uuid::Uuid::new_v4().to_string(),
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 422, "Invalid enum should return 422");
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    let details = body["error"]["details"]
        .as_array()
        .expect("details should be an array");
    assert!(
        details.iter().any(|e| e["code"] == "invalid_enum"),
        "Should have invalid_enum error code"
    );
}

// ---------------------------------------------------------------------------
// 6. Auth enforcement — admin-only endpoint rejects member
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_auth_enforcement_rejects_wrong_role(pool: sqlx::PgPool) {
    let jwt = test_jwt();
    let mut resource = test_resource();

    // Build endpoints where create requires admin
    let mut eps = full_crud_endpoints();
    eps.get_mut("create").unwrap().auth = Some(AuthRule::Roles(vec!["admin".to_string()]));
    resource.endpoints = Some(eps);

    let state = make_state(pool, Some(jwt.clone()));

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .app_data(web::Data::new(Arc::new(jwt.clone())))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    // Generate a member token
    let member_token = jwt.encode_access("user-2", "member").unwrap();

    let req = actix_test::TestRequest::post()
        .uri("/test_users")
        .insert_header(("Authorization", format!("Bearer {member_token}")))
        .set_json(user_payload(
            "forbidden@example.com",
            "Forbidden",
            "member",
            &uuid::Uuid::new_v4().to_string(),
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "Member should be forbidden from admin-only endpoint"
    );
}

// ---------------------------------------------------------------------------
// 7. Auth public endpoint — no token required
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_auth_public_endpoint(pool: sqlx::PgPool) {
    let jwt = test_jwt();
    let mut resource = test_resource();

    // list endpoint explicitly marked public
    let mut eps = full_crud_endpoints();
    eps.get_mut("list").unwrap().auth = Some(AuthRule::Public);
    // create stays unauthenticated (auth: None = public) so we can seed data
    resource.endpoints = Some(eps);

    let state = make_state(pool, Some(jwt.clone()));

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .app_data(web::Data::new(Arc::new(jwt.clone())))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    // Seed one record (create has no auth)
    let org_id = uuid::Uuid::new_v4().to_string();
    let req = actix_test::TestRequest::post()
        .uri("/test_users")
        .set_json(user_payload(
            "public@example.com",
            "Public User",
            "member",
            &org_id,
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // List without any token — should succeed
    let req = actix_test::TestRequest::get()
        .uri("/test_users")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "Public endpoint should allow unauthenticated access"
    );
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert!(!body["data"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// 8. Bulk create
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_bulk_create(pool: sqlx::PgPool) {
    let mut resource = test_resource();
    let mut eps = full_crud_endpoints();
    eps.insert(
        "bulk_create".to_string(),
        EndpointSpec {
            method: HttpMethod::Post,
            path: "/test_users/bulk".to_string(),
            auth: None,
            input: Some(vec![
                "email".to_string(),
                "name".to_string(),
                "role".to_string(),
                "org_id".to_string(),
            ]),
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
        },
    );
    resource.endpoints = Some(eps);
    let state = make_state(pool, None);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();
    let payload = json!([
        {"email": "bulk1@example.com", "name": "Bulk 1", "role": "admin", "org_id": org_id},
        {"email": "bulk2@example.com", "name": "Bulk 2", "role": "member", "org_id": org_id},
        {"email": "bulk3@example.com", "name": "Bulk 3", "role": "viewer", "org_id": org_id},
    ]);

    let req = actix_test::TestRequest::post()
        .uri("/test_users/bulk")
        .set_json(payload)
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "Bulk create should return 200");
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let data = body["data"].as_array().expect("data should be array");
    assert_eq!(data.len(), 3, "Should have created 3 records");
    assert_eq!(body["meta"]["total"], 3);
}

// ---------------------------------------------------------------------------
// 9. Soft delete excludes from list
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_soft_delete_excludes_from_list(pool: sqlx::PgPool) {
    let mut resource = test_resource();
    resource.endpoints = Some(full_crud_endpoints());
    let state = make_state(pool, None);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();

    // Create two users
    let mut created_ids = Vec::new();
    for (email, name) in [
        ("keep@example.com", "Keep"),
        ("delete@example.com", "Delete"),
    ] {
        let req = actix_test::TestRequest::post()
            .uri("/test_users")
            .set_json(user_payload(email, name, "member", &org_id))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: serde_json::Value = actix_test::read_body_json(resp).await;
        created_ids.push(body["data"]["id"].as_str().unwrap().to_string());
    }

    // Soft-delete the second user
    let delete_id = &created_ids[1];
    let req = actix_test::TestRequest::delete()
        .uri(&format!("/test_users/{delete_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    // List should not include the deleted user
    let req = actix_test::TestRequest::get()
        .uri(&format!("/test_users?filter%5Borg_id%5D={org_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let data = body["data"].as_array().expect("data array");

    let listed_ids: Vec<&str> = data.iter().filter_map(|r| r["id"].as_str()).collect();
    assert!(
        !listed_ids.contains(&delete_id.as_str()),
        "Soft-deleted record should not appear in list"
    );
    assert!(
        listed_ids.contains(&created_ids[0].as_str()),
        "Non-deleted record should appear in list"
    );
}

// ---------------------------------------------------------------------------
// 10. Field selection
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_field_selection(pool: sqlx::PgPool) {
    let mut resource = test_resource();
    resource.endpoints = Some(full_crud_endpoints());
    let state = make_state(pool, None);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();

    // Create a user
    let req = actix_test::TestRequest::post()
        .uri("/test_users")
        .set_json(user_payload(
            "fields@example.com",
            "Fields User",
            "admin",
            &org_id,
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // List with field selection
    let req = actix_test::TestRequest::get()
        .uri("/test_users?fields=name,email")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let data = body["data"].as_array().expect("data array");
    assert!(!data.is_empty(), "Should return at least one record");

    for item in data {
        let obj = item.as_object().expect("each item should be an object");
        assert!(obj.contains_key("name"), "Should include 'name'");
        assert!(obj.contains_key("email"), "Should include 'email'");
        assert!(
            !obj.contains_key("id"),
            "Should not include non-selected field 'id'"
        );
        assert!(
            !obj.contains_key("role"),
            "Should not include non-selected field 'role'"
        );
    }
}

// ---------------------------------------------------------------------------
// 11. Metrics wiring
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_metrics_capture_requests_errors_and_cache(pool: sqlx::PgPool) {
    let _redis_guard = redis_test_lock().lock().await;
    let redis_pool = Arc::new(create_redis_pool(&redis_url()).expect("redis pool"));
    clear_resource_cache(&redis_pool, "test_users").await;

    let mut resource = test_resource();
    let mut endpoints = full_crud_endpoints();
    endpoints.get_mut("list").unwrap().cache = Some(CacheSpec {
        ttl: 60,
        invalidate_on: None,
    });
    resource.endpoints = Some(endpoints);

    let metrics_state = web::Data::new(MetricsState::new().expect("metrics state"));
    let state = Arc::new(AppState {
        pool: pool.clone(),
        resources: vec![],
        stores: None,
        jwt_config: None,
        cache: Some(RedisCache::new(redis_pool.clone())),
        event_emitter: None,
        job_queue: None,
        metrics: Some(metrics_state.get_ref().clone()),
    });

    let app = actix_test::init_service(
        App::new()
            .wrap(RequestLogger::new(HashSet::new()))
            .app_data(web::Data::new(state.clone()))
            .app_data(metrics_state.clone())
            .route("/metrics", web::get().to(metrics_handler))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();
    let req = actix_test::TestRequest::post()
        .uri("/test_users")
        .set_json(user_payload(
            "metrics@example.com",
            "Metrics User",
            "admin",
            &org_id,
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = actix_test::TestRequest::get()
        .uri(&format!("/test_users?filter%5Borg_id%5D={org_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("X-Cache").unwrap(), "MISS");

    let req = actix_test::TestRequest::get()
        .uri(&format!("/test_users?filter%5Borg_id%5D={org_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("X-Cache").unwrap(), "HIT");

    let missing_id = uuid::Uuid::new_v4();
    let req = actix_test::TestRequest::get()
        .uri(&format!("/test_users/{missing_id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    let req = actix_test::TestRequest::get().uri("/metrics").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = actix_test::read_body(resp).await;
    let output = String::from_utf8(body.to_vec()).expect("utf8 metrics");

    assert!(output.contains("shaperail_http_requests_total"));
    assert!(output.contains("shaperail_http_request_duration_seconds"));
    assert!(output.contains(r#"shaperail_cache_total{result="hit"} 1"#));
    assert!(output.contains(r#"shaperail_cache_total{result="miss"} 1"#));
    assert!(output.contains(r#"shaperail_errors_total{error_type="http_404"} 1"#));
    assert!(output.contains("shaperail_db_pool_size"));
    assert!(output.contains("shaperail_job_queue_depth"));
}

// ---------------------------------------------------------------------------
// 12. Redis cache integration
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_list_cache_hit_serves_stale_data_after_db_delete(pool: sqlx::PgPool) {
    let _redis_guard = redis_test_lock().lock().await;
    let redis_pool = Arc::new(create_redis_pool(&redis_url()).expect("redis pool"));
    clear_resource_cache(&redis_pool, "test_users").await;

    let mut resource = test_resource();
    let mut endpoints = full_crud_endpoints();
    endpoints.get_mut("list").unwrap().cache = Some(CacheSpec {
        ttl: 60,
        invalidate_on: None,
    });
    resource.endpoints = Some(endpoints);

    let metrics_state = web::Data::new(MetricsState::new().expect("metrics state"));
    let state = Arc::new(AppState {
        pool: pool.clone(),
        resources: vec![],
        stores: None,
        jwt_config: None,
        cache: Some(RedisCache::new(redis_pool.clone())),
        event_emitter: None,
        job_queue: None,
        metrics: Some(metrics_state.get_ref().clone()),
    });

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .app_data(metrics_state.clone())
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();
    let req = actix_test::TestRequest::post()
        .uri("/test_users")
        .set_json(user_payload(
            "cache-hit@example.com",
            "Cached User",
            "admin",
            &org_id,
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let list_uri = format!("/test_users?filter%5Borg_id%5D={org_id}");

    let req = actix_test::TestRequest::get().uri(&list_uri).to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("X-Cache").unwrap(), "MISS");
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    sqlx::query("DELETE FROM test_users WHERE org_id = $1")
        .bind(uuid::Uuid::parse_str(&org_id).expect("org id uuid"))
        .execute(&pool)
        .await
        .expect("delete rows");

    let req = actix_test::TestRequest::get().uri(&list_uri).to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("X-Cache").unwrap(), "HIT");
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let data = body["data"].as_array().expect("data array");
    assert_eq!(
        data.len(),
        1,
        "cached list should still contain deleted row"
    );
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_write_invalidates_cached_list(pool: sqlx::PgPool) {
    let _redis_guard = redis_test_lock().lock().await;
    let redis_pool = Arc::new(create_redis_pool(&redis_url()).expect("redis pool"));
    clear_resource_cache(&redis_pool, "test_users").await;

    let mut resource = test_resource();
    let mut endpoints = full_crud_endpoints();
    endpoints.get_mut("list").unwrap().cache = Some(CacheSpec {
        ttl: 60,
        invalidate_on: None,
    });
    resource.endpoints = Some(endpoints);

    let state = Arc::new(AppState {
        pool: pool.clone(),
        resources: vec![],
        stores: None,
        jwt_config: None,
        cache: Some(RedisCache::new(redis_pool.clone())),
        event_emitter: None,
        job_queue: None,
        metrics: Some(MetricsState::new().expect("metrics state")),
    });

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();

    for email in ["invalidate-1@example.com", "invalidate-2@example.com"] {
        let req = actix_test::TestRequest::post()
            .uri("/test_users")
            .set_json(user_payload(email, "Invalidate User", "admin", &org_id))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let list_uri = format!("/test_users?filter%5Borg_id%5D={org_id}");

    let req = actix_test::TestRequest::get().uri(&list_uri).to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("X-Cache").unwrap(), "MISS");

    let req = actix_test::TestRequest::get().uri(&list_uri).to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("X-Cache").unwrap(), "HIT");

    let req = actix_test::TestRequest::post()
        .uri("/test_users")
        .set_json(user_payload(
            "invalidate-3@example.com",
            "Invalidate User",
            "admin",
            &org_id,
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = actix_test::TestRequest::get().uri(&list_uri).to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("X-Cache").unwrap(), "MISS");
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 3);
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_nocache_bypasses_cached_response(pool: sqlx::PgPool) {
    let _redis_guard = redis_test_lock().lock().await;
    let redis_pool = Arc::new(create_redis_pool(&redis_url()).expect("redis pool"));
    clear_resource_cache(&redis_pool, "test_users").await;

    let mut resource = test_resource();
    let mut endpoints = full_crud_endpoints();
    endpoints.get_mut("list").unwrap().cache = Some(CacheSpec {
        ttl: 60,
        invalidate_on: None,
    });
    resource.endpoints = Some(endpoints);

    let state = Arc::new(AppState {
        pool: pool.clone(),
        resources: vec![],
        stores: None,
        jwt_config: None,
        cache: Some(RedisCache::new(redis_pool.clone())),
        event_emitter: None,
        job_queue: None,
        metrics: Some(MetricsState::new().expect("metrics state")),
    });

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();
    let req = actix_test::TestRequest::post()
        .uri("/test_users")
        .set_json(user_payload(
            "bypass@example.com",
            "Bypass User",
            "admin",
            &org_id,
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let list_uri = format!("/test_users?filter%5Borg_id%5D={org_id}");
    let req = actix_test::TestRequest::get().uri(&list_uri).to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("X-Cache").unwrap(), "MISS");

    sqlx::query("DELETE FROM test_users WHERE org_id = $1")
        .bind(uuid::Uuid::parse_str(&org_id).expect("org id uuid"))
        .execute(&pool)
        .await
        .expect("delete rows");

    let req = actix_test::TestRequest::get()
        .uri(&format!("{list_uri}&nocache=1"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(
        !resp.headers().contains_key("X-Cache"),
        "bypass should skip cache headers"
    );
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert!(body["data"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// 13. File upload endpoints
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_upload_endpoint_persists_file_path_and_metadata(pool: sqlx::PgPool) {
    let _storage_guard = storage_test_lock().lock().await;
    let storage_dir = TempDir::new().expect("temp storage dir");
    std::env::set_var(
        "SHAPERAIL_STORAGE_LOCAL_DIR",
        storage_dir.path().display().to_string(),
    );

    let resource = test_asset_resource();
    let state = make_state(pool.clone(), None);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let (boundary, body) = multipart_body(
        &[("title", "Quarterly report")],
        "attachment",
        "report.txt",
        "text/plain",
        b"hello world",
    );

    let req = actix_test::TestRequest::post()
        .uri("/test_assets")
        .insert_header((
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(body)
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let data = body["data"].as_object().expect("created asset");

    let path = data["attachment"].as_str().expect("attachment path");
    assert!(path.starts_with("test_assets/attachment/"));
    assert_eq!(data["attachment_filename"], "report.txt");
    assert_eq!(data["attachment_mime_type"], "text/plain");
    assert_eq!(data["attachment_size"], 11);

    let stored_path = storage_dir.path().join(path);
    assert!(stored_path.exists(), "uploaded file should exist on disk");

    let row = sqlx::query(
        r#"
        SELECT attachment, attachment_filename, attachment_mime_type, attachment_size
        FROM test_assets
        LIMIT 1
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("fetch stored asset");

    assert_eq!(row.get::<String, _>("attachment"), path);
    assert_eq!(row.get::<String, _>("attachment_filename"), "report.txt");
    assert_eq!(row.get::<String, _>("attachment_mime_type"), "text/plain");
    assert_eq!(row.get::<i64, _>("attachment_size"), 11);

    std::env::remove_var("SHAPERAIL_STORAGE_LOCAL_DIR");
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_delete_endpoint_cleans_up_uploaded_file(pool: sqlx::PgPool) {
    let _storage_guard = storage_test_lock().lock().await;
    let storage_dir = TempDir::new().expect("temp storage dir");
    std::env::set_var(
        "SHAPERAIL_STORAGE_LOCAL_DIR",
        storage_dir.path().display().to_string(),
    );

    let resource = test_asset_resource();
    let state = make_state(pool.clone(), None);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let (boundary, body) = multipart_body(
        &[("title", "Delete me")],
        "attachment",
        "delete-me.txt",
        "text/plain",
        b"cleanup",
    );

    let req = actix_test::TestRequest::post()
        .uri("/test_assets")
        .insert_header((
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(body)
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let data = body["data"].as_object().expect("created asset");
    let id = data["id"].as_str().expect("asset id").to_string();
    let path = data["attachment"]
        .as_str()
        .expect("attachment path")
        .to_string();
    let stored_path = storage_dir.path().join(&path);
    assert!(
        stored_path.exists(),
        "uploaded file should exist before delete"
    );

    let req = actix_test::TestRequest::delete()
        .uri(&format!("/test_assets/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if !stored_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("file cleanup should finish");

    std::env::remove_var("SHAPERAIL_STORAGE_LOCAL_DIR");
}

// ---------------------------------------------------------------------------
// Relation loading with store (?include=)
// ---------------------------------------------------------------------------

/// Store implementation that delegates to ResourceQuery (used to test store path in relation loading).
struct QueryBackedStore {
    resource: ResourceDefinition,
    pool: sqlx::PgPool,
}

#[async_trait]
impl ResourceStore for QueryBackedStore {
    fn resource_name(&self) -> &str {
        &self.resource.resource
    }

    async fn find_by_id(
        &self,
        id: &uuid::Uuid,
    ) -> Result<ResourceRow, shaperail_core::ShaperailError> {
        ResourceQuery::new(&self.resource, &self.pool)
            .find_by_id(id)
            .await
    }

    async fn find_all(
        &self,
        _endpoint: &EndpointSpec,
        filters: &FilterSet,
        search: Option<&shaperail_runtime::db::SearchParam>,
        sort: &SortParam,
        page: &PageRequest,
    ) -> Result<(Vec<ResourceRow>, serde_json::Value), shaperail_core::ShaperailError> {
        ResourceQuery::new(&self.resource, &self.pool)
            .find_all(filters, search, sort, page)
            .await
    }

    async fn insert(
        &self,
        data: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<ResourceRow, shaperail_core::ShaperailError> {
        ResourceQuery::new(&self.resource, &self.pool)
            .insert(data)
            .await
    }

    async fn update_by_id(
        &self,
        id: &uuid::Uuid,
        data: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<ResourceRow, shaperail_core::ShaperailError> {
        ResourceQuery::new(&self.resource, &self.pool)
            .update_by_id(id, data)
            .await
    }

    async fn soft_delete_by_id(
        &self,
        id: &uuid::Uuid,
    ) -> Result<ResourceRow, shaperail_core::ShaperailError> {
        ResourceQuery::new(&self.resource, &self.pool)
            .soft_delete_by_id(id)
            .await
    }

    async fn hard_delete_by_id(
        &self,
        id: &uuid::Uuid,
    ) -> Result<ResourceRow, shaperail_core::ShaperailError> {
        ResourceQuery::new(&self.resource, &self.pool)
            .hard_delete_by_id(id)
            .await
    }
}

fn org_resource() -> ResourceDefinition {
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

    let mut endpoints = IndexMap::new();
    endpoints.insert(
        "list".to_string(),
        EndpointSpec {
            method: HttpMethod::Get,
            path: "/test_orgs".to_string(),
            auth: None,
            input: None,
            filters: None,
            search: None,
            pagination: Some(PaginationStyle::Cursor),
            sort: None,
            cache: None,
            hooks: None,
            events: None,
            jobs: None,
            upload: None,
            soft_delete: false,
        },
    );
    endpoints.insert(
        "get".to_string(),
        EndpointSpec {
            method: HttpMethod::Get,
            path: "/test_orgs/:id".to_string(),
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
        },
    );
    endpoints.insert(
        "create".to_string(),
        EndpointSpec {
            method: HttpMethod::Post,
            path: "/test_orgs".to_string(),
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
        },
    );

    ResourceDefinition {
        resource: "test_orgs".to_string(),
        version: 1,
        schema,
        endpoints: Some(endpoints),
        relations: None,
        indexes: None,
    }
}

fn users_resource_with_organization_relation() -> ResourceDefinition {
    let mut resource = test_resource();
    resource.endpoints = Some(full_crud_endpoints());
    let mut relations = IndexMap::new();
    relations.insert(
        "organization".to_string(),
        RelationSpec {
            resource: "test_orgs".to_string(),
            relation_type: RelationType::BelongsTo,
            key: Some("org_id".to_string()),
            foreign_key: None,
        },
    );
    resource.relations = Some(relations);
    resource
}

fn build_test_store_registry(
    pool: sqlx::PgPool,
    resources: &[ResourceDefinition],
) -> StoreRegistry {
    let mut map: HashMap<String, Arc<dyn ResourceStore>> = HashMap::new();
    for resource in resources {
        let store = Arc::new(QueryBackedStore {
            resource: resource.clone(),
            pool: pool.clone(),
        });
        map.insert(resource.resource.clone(), store);
    }
    Arc::new(map)
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_list_with_include_uses_store(pool: sqlx::PgPool) {
    let org_res = org_resource();
    let users_res = users_resource_with_organization_relation();
    let resources = vec![users_res.clone(), org_res.clone()];
    let stores = build_test_store_registry(pool.clone(), &resources);

    let state = Arc::new(AppState {
        pool: pool.clone(),
        resources: resources.clone(),
        stores: Some(stores),
        jwt_config: None,
        cache: None,
        event_emitter: None,
        job_queue: None,
        metrics: Some(MetricsState::new().expect("metrics state")),
    });

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| {
                for res in &resources {
                    register_resource(cfg, res, state.clone());
                }
            }),
    )
    .await;

    // Create one org
    let create_org = actix_test::TestRequest::post()
        .uri("/test_orgs")
        .set_json(json!({ "name": "Acme Corp" }))
        .to_request();
    let resp = actix_test::call_service(&app, create_org).await;
    assert_eq!(resp.status(), 201, "Create org should return 201");
    let org_body: serde_json::Value = actix_test::read_body_json(resp).await;
    let org_id = org_body["data"]["id"].as_str().expect("org has id");

    // Create user with that org_id
    let create_user = actix_test::TestRequest::post()
        .uri("/test_users")
        .set_json(user_payload(
            "include@example.com",
            "Include User",
            "member",
            org_id,
        ))
        .to_request();
    let resp = actix_test::call_service(&app, create_user).await;
    assert_eq!(resp.status(), 201, "Create user should return 201");

    // List users with include=organization; store path is used for relation loading
    let req = actix_test::TestRequest::get()
        .uri("/test_users?include=organization")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "List with include should return 200");
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let data = body["data"].as_array().expect("data is array");
    assert!(!data.is_empty(), "At least one user");
    let user = &data[0];
    assert!(
        user.get("organization").is_some(),
        "User should have embedded organization from ?include=organization"
    );
    assert_eq!(user["organization"]["name"], "Acme Corp");
}
