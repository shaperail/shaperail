//! API-level integration tests for the shaperail-runtime handler stack.
//!
//! Uses `#[sqlx::test]` macro for auto-rollback and isolated DB per test.
//! Requires a running PostgreSQL instance.
//! Set DATABASE_URL env var or run `docker compose up -d` first.
//!
//! Run with: cargo test -p shaperail-runtime --test api_integration

use std::sync::Arc;

use actix_web::{test as actix_test, web, App};
use indexmap::IndexMap;
use serde_json::json;
use shaperail_core::{
    AuthRule, EndpointSpec, FieldSchema, FieldType, HttpMethod, PaginationStyle, ResourceDefinition,
};
use shaperail_runtime::auth::jwt::JwtConfig;
use shaperail_runtime::handlers::crud::AppState;
use shaperail_runtime::handlers::routes::register_resource;

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
        jwt_config: jwt.map(Arc::new),
        cache: None,
        event_emitter: None,
        job_queue: None,
    })
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
