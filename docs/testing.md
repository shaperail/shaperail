---
title: Testing
parent: Guides
nav_order: 6
---

# Testing

Shaperail projects use standard Rust testing tools. The framework generates
testable code and provides patterns for unit tests, integration tests, and
end-to-end tests against a real database.

---

## Running tests

### The `shaperail test` command

The simplest way to run your test suite:

```bash
shaperail test
```

This wraps `cargo test` and passes through any additional arguments:

```bash
# Run a specific test by name
shaperail test -- test_create_user

# Run tests in a specific module
shaperail test -- --test api_integration

# Show output from passing tests
shaperail test -- --nocapture

# Run only tests matching a pattern
shaperail test -- "test_validation"
```

### Using `cargo test` directly

You can also call `cargo test` with full control:

```bash
# Run all tests in the workspace
cargo test --workspace

# Run tests for a specific crate
cargo test -p my-app

# Run a single test file
cargo test --test api_integration

# Run tests with release optimizations
cargo test --release
```

### Pre-commit checklist

Always run these before committing:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

---

## Unit testing controllers

Controllers are async functions that take `&mut Context`. To unit test them,
construct a `Context` with the fields your controller reads, call the function,
and assert the result.

### Testing a before-controller

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use shaperail_runtime::handlers::controller::Context;

    fn mock_context(input: serde_json::Value) -> Context {
        Context {
            input: input.as_object().unwrap().clone(),
            data: None,
            user: None,
            pool: test_pool(),  // see "Testing with a real database" below
            headers: std::collections::HashMap::new(),
            response_headers: vec![],
            tenant_id: None,
        }
    }

    #[tokio::test]
    async fn test_normalize_name_trims_whitespace() {
        let mut ctx = mock_context(json!({
            "name": "  Alice  ",
            "role": "member"
        }));

        let result = normalize_name(&mut ctx).await;

        assert!(result.is_ok());
        assert_eq!(ctx.input["name"], "Alice");
    }

    #[tokio::test]
    async fn test_normalize_name_does_nothing_when_name_missing() {
        let mut ctx = mock_context(json!({
            "role": "member"
        }));

        let result = normalize_name(&mut ctx).await;

        assert!(result.is_ok());
        assert!(ctx.input.get("name").is_none());
    }
}
```

### Testing an after-controller

After-controllers receive `ctx.data` containing the database result:

```rust
#[tokio::test]
async fn test_enrich_response_adds_display_name() {
    let mut ctx = mock_context(json!({}));
    ctx.data = Some(json!({
        "id": "abc-123",
        "name": "Alice",
        "role": "admin"
    }));

    let result = enrich_response(&mut ctx).await;

    assert!(result.is_ok());
    let data = ctx.data.unwrap();
    assert_eq!(data["display_name"], "Alice (admin)");
}
```

### Testing controller error paths

Controllers return `Err(ShaperailError::...)` to halt the request:

```rust
use shaperail_core::ShaperailError;

#[tokio::test]
async fn test_set_created_by_rejects_unauthenticated() {
    let mut ctx = mock_context(json!({"title": "hello"}));
    ctx.user = None;

    let result = set_created_by(&mut ctx).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ShaperailError::Unauthorized => {}
        other => panic!("Expected Auth error, got: {:?}", other),
    }
}
```

### Testing with an authenticated user

```rust
use shaperail_runtime::auth::AuthenticatedUser;

#[tokio::test]
async fn test_admin_only_fields_strips_role_for_non_admin() {
    let mut ctx = mock_context(json!({
        "name": "Bob",
        "role": "admin",
        "org_id": "org-1"
    }));
    ctx.user = Some(AuthenticatedUser {
        id: "user-1".into(),
        role: "member".into(),
        tenant_id: None,
    });

    let result = admin_only_fields(&mut ctx).await;

    assert!(result.is_ok());
    assert!(ctx.input.get("role").is_none(), "role should be removed");
    assert!(ctx.input.get("org_id").is_none(), "org_id should be removed");
    assert_eq!(ctx.input["name"], "Bob", "name should be preserved");
}
```

---

## Integration testing endpoints

Use `actix_web::test` to spin up a test server with real handlers and make HTTP
requests against it. This is the same pattern used by Shaperail's own test
suite.

### Basic setup

```rust
use actix_web::{test as actix_test, web, App};
use serde_json::json;
use shaperail_runtime::handlers::crud::AppState;
use shaperail_runtime::handlers::routes::register_resource;
use std::sync::Arc;

/// Build an AppState for testing (no auth, no cache, no jobs).
fn make_test_state(pool: sqlx::PgPool) -> Arc<AppState> {
    Arc::new(AppState {
        pool,
        resources: vec![],
        stores: None,
        controllers: None,
        jwt_config: None,
        cache: None,
        event_emitter: None,
        job_queue: None,
        metrics: None,
        wasm_runtime: None,
        event_bus: tokio::sync::broadcast::channel(16).0,
    })
}
```

### Full CRUD test

```rust
#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_full_crud_cycle(pool: sqlx::PgPool) {
    let resource = test_resource();  // your ResourceDefinition
    let state = make_test_state(pool);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();

    // CREATE
    let req = actix_test::TestRequest::post()
        .uri("/v1/users")
        .set_json(json!({
            "email": "alice@example.com",
            "name": "Alice",
            "role": "admin",
            "org_id": org_id
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let id = body["data"]["id"].as_str().expect("id in response");
    assert_eq!(body["data"]["name"], "Alice");

    // READ
    let req = actix_test::TestRequest::get()
        .uri(&format!("/v1/users/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // UPDATE
    let req = actix_test::TestRequest::patch()
        .uri(&format!("/v1/users/{id}"))
        .set_json(json!({"name": "Alice Updated"}))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Alice Updated");

    // DELETE
    let req = actix_test::TestRequest::delete()
        .uri(&format!("/v1/users/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);

    // Verify deleted
    let req = actix_test::TestRequest::get()
        .uri(&format!("/v1/users/{id}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}
```

### Testing with authentication

Add a JWT config to `AppState` and include tokens in requests:

```rust
use shaperail_runtime::auth::jwt::JwtConfig;

fn make_auth_state(pool: sqlx::PgPool) -> Arc<AppState> {
    let jwt = JwtConfig::new(
        "test-secret-key-at-least-32-bytes-long!",
        3600,   // access token TTL
        86400,  // refresh token TTL
    );
    Arc::new(AppState {
        pool,
        jwt_config: Some(Arc::new(jwt)),
        // ... other fields same as make_test_state
        resources: vec![],
        stores: None,
        controllers: None,
        cache: None,
        event_emitter: None,
        job_queue: None,
        metrics: None,
        wasm_runtime: None,
        event_bus: tokio::sync::broadcast::channel(16).0,
    })
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_auth_rejects_wrong_role(pool: sqlx::PgPool) {
    let resource = test_resource();  // endpoint requires auth: [admin]
    let state = make_auth_state(pool);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    // Generate a token with "viewer" role
    let jwt = JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400);
    let token = jwt
        .encode_access("user-1", "viewer")
        .expect("generate token");

    let req = actix_test::TestRequest::post()
        .uri("/v1/users")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(json!({
            "email": "bob@example.com",
            "name": "Bob",
            "role": "member",
            "org_id": uuid::Uuid::new_v4().to_string()
        }))
        .to_request();

    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403, "Viewer should not access admin endpoint");
}
```

### Testing validation errors

```rust
#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_validation_rejects_missing_required_field(pool: sqlx::PgPool) {
    let resource = test_resource();
    let state = make_test_state(pool);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    // Missing required "name" field
    let req = actix_test::TestRequest::post()
        .uri("/v1/users")
        .set_json(json!({
            "email": "alice@example.com",
            "role": "member",
            "org_id": uuid::Uuid::new_v4().to_string()
        }))
        .to_request();

    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 422, "Missing field should return 422");

    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let errors = body["errors"].as_array().expect("errors array");
    assert!(
        errors.iter().any(|e| e["field"] == "name"),
        "Error should mention the missing field"
    );
}
```

### Testing filters and pagination

```rust
#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_list_with_filters(pool: sqlx::PgPool) {
    let resource = test_resource();
    let state = make_test_state(pool);

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let org_id = uuid::Uuid::new_v4().to_string();

    // Insert users with different roles
    for (email, role) in [
        ("admin@test.com", "admin"),
        ("member@test.com", "member"),
        ("viewer@test.com", "viewer"),
    ] {
        let req = actix_test::TestRequest::post()
            .uri("/v1/users")
            .set_json(json!({
                "email": email,
                "name": "Test User",
                "role": role,
                "org_id": org_id
            }))
            .to_request();
        actix_test::call_service(&app, req).await;
    }

    // Filter by role=admin
    let req = actix_test::TestRequest::get()
        .uri("/v1/users?filter%5Brole%5D=admin")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let data = body["data"].as_array().expect("data array");
    assert!(!data.is_empty());
    for item in data {
        assert_eq!(item["role"], "admin");
    }
}
```

---

## Testing with a real database

Shaperail integration tests run against a real PostgreSQL instance. The
`#[sqlx::test]` macro provides automatic transaction rollback and test
isolation.

### Docker setup

Start the dev database and Redis:

```bash
docker compose up -d
```

This starts PostgreSQL on port 5433 and Redis on port 6379. Set the environment:

```bash
export DATABASE_URL=postgresql://shaperail:shaperail@localhost:5433/shaperail_dev
export REDIS_URL=redis://localhost:6379
```

Or add these to your `.env` file (created by `shaperail init`).

### Test database isolation

The `#[sqlx::test]` macro creates an isolated database for each test function.
Each test runs in a transaction that is rolled back when the test completes,
so tests never interfere with each other.

```rust
#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_something(pool: sqlx::PgPool) {
    // `pool` is connected to an isolated DB with migrations applied.
    // Anything written here is rolled back after the test.
}
```

### Test migrations

Place test-specific migrations in `tests/fixtures/migrations/`. These create the
tables your tests need without requiring your full app schema:

```text
tests/
  fixtures/
    migrations/
      01_create_test_users.sql
      02_create_test_orders.sql
```

Example migration:

```sql
-- tests/fixtures/migrations/01_create_test_users.sql
CREATE TABLE IF NOT EXISTS test_users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(200) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'member',
    org_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);
```

### Seed data with `shaperail seed`

Load fixture YAML files into the database:

```bash
shaperail seed
```

This reads YAML files from the `seeds/` directory and inserts them in a
transaction. Seed files follow this format:

```yaml
# seeds/users.yaml
resource: users
records:
  - email: alice@example.com
    name: Alice Admin
    role: admin
    org_id: "11111111-1111-1111-1111-111111111111"
  - email: bob@example.com
    name: Bob Member
    role: member
    org_id: "11111111-1111-1111-1111-111111111111"
```

Load a specific seed file:

```bash
shaperail seed seeds/users.yaml
```

For tests, insert data programmatically using the test helpers shown in the
integration test examples above.

### Test data builder pattern

Use a builder function to create consistent test fixtures:

```rust
fn user_payload(email: &str, name: &str, role: &str, org_id: &str) -> serde_json::Value {
    serde_json::json!({
        "email": email,
        "name": name,
        "role": role,
        "org_id": org_id
    })
}

// Usage in tests:
let org_id = uuid::Uuid::new_v4().to_string();
let payload = user_payload("test@example.com", "Test User", "member", &org_id);
```

---

## Testing background jobs

Background jobs are enqueued to a Redis-backed queue. Test them in two layers:
verify that endpoints enqueue the correct jobs, and test job handler functions
in isolation.

### Checking that jobs are enqueued

After calling an endpoint that declares `jobs: [send_welcome_email]`, verify
the job appears in the Redis queue:

```rust
use redis::AsyncCommands;
use shaperail_runtime::cache::create_redis_pool;

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_create_enqueues_welcome_email_job(pool: sqlx::PgPool) {
    let redis_pool = create_redis_pool(&redis_url()).expect("redis pool");
    let state = make_state_with_jobs(pool, redis_pool.clone());

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    // Create a user (triggers jobs: [send_welcome_email])
    let req = actix_test::TestRequest::post()
        .uri("/v1/users")
        .set_json(json!({
            "email": "new@example.com",
            "name": "New User",
            "role": "member",
            "org_id": uuid::Uuid::new_v4().to_string()
        }))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // Check Redis queue for the enqueued job
    let mut conn = redis_pool.get().await.expect("redis connection");
    let queue_len: i64 = conn
        .llen("shaperail:jobs:queue:normal")
        .await
        .expect("queue length");
    assert!(queue_len > 0, "Job should be enqueued");

    // Peek at the job payload
    let raw: String = conn
        .lindex("shaperail:jobs:queue:normal", 0)
        .await
        .expect("peek job");
    let job: serde_json::Value = serde_json::from_str(&raw).expect("parse job");
    assert_eq!(job["name"], "send_welcome_email");
}
```

### Testing job handler functions

Job handlers are regular async functions. Test them the same way you test any
async Rust code:

```rust
#[tokio::test]
async fn test_send_welcome_email_job_handler() {
    let payload = json!({
        "email": "alice@example.com",
        "name": "Alice"
    });

    // Call your job handler directly
    let result = send_welcome_email(payload).await;

    assert!(result.is_ok());
    // Assert side effects: email sent, external API called, etc.
}
```

### Testing retry behavior

To test that a job retries on failure, simulate a transient error and verify the
job re-enters the queue:

```rust
#[tokio::test]
async fn test_job_retries_on_transient_error() {
    let payload = json!({"email": "fail@example.com"});

    // First call fails
    let result = flaky_job_handler(payload.clone()).await;
    assert!(result.is_err());

    // The job queue worker will re-enqueue automatically.
    // In a test, verify retry count via the job metadata in Redis:
    let mut conn = redis_pool.get().await.unwrap();
    let attempts: i64 = conn
        .hget("shaperail:jobs:meta:job-123", "attempts")
        .await
        .unwrap_or(0);
    assert!(attempts <= 3, "Should not exceed max_retries");
}
```

---

## Testing events and webhooks

Events are emitted after mutations and processed asynchronously. Test them by
checking the event log or by verifying that subscriber targets are triggered.

### Verifying events are emitted

After a write operation, check the `shaperail_event_log` table:

```rust
#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_create_emits_event(pool: sqlx::PgPool) {
    let state = make_state_with_events(pool.clone());

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let req = actix_test::TestRequest::post()
        .uri("/v1/users")
        .set_json(json!({
            "email": "event@example.com",
            "name": "Event User",
            "role": "member",
            "org_id": uuid::Uuid::new_v4().to_string()
        }))
        .to_request();
    actix_test::call_service(&app, req).await;

    // Allow async event processing to complete
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Check event log
    let row = sqlx::query("SELECT event, resource FROM shaperail_event_log ORDER BY timestamp DESC LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("event logged");

    let event: String = row.get("event");
    let resource: String = row.get("resource");
    assert_eq!(event, "users.created");
    assert_eq!(resource, "users");
}
```

### Testing outbound webhook delivery

Use a mock HTTP server (e.g., `wiremock`) to verify webhook delivery:

```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header_exists};

#[tokio::test]
async fn test_webhook_delivery_with_signature() {
    // Start a mock server
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/hooks/user-created"))
        .and(header_exists("X-Shaperail-Signature"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Configure the webhook subscriber to point at the mock server
    let webhook_url = format!("{}/hooks/user-created", mock_server.uri());

    // Emit an event that triggers the webhook
    emit_event("users.created", &json!({
        "id": "abc-123",
        "email": "alice@example.com"
    }), &webhook_url).await;

    // wiremock automatically verifies the expected call count on drop
}
```

### Testing inbound webhooks

Send a signed request to an inbound webhook endpoint and verify it produces an
internal event:

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_inbound_stripe_webhook(pool: sqlx::PgPool) {
    let state = make_state_with_events(pool.clone());
    let secret = "whsec_test_secret";

    // Build signed payload
    let body = json!({"type": "payment.completed", "data": {"amount": 1000}});
    let body_str = serde_json::to_string(&body).unwrap();
    let timestamp = chrono::Utc::now().timestamp();

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{timestamp}.{body_str}").as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let app = actix_test::init_service(/* ... */).await;

    let req = actix_test::TestRequest::post()
        .uri("/webhooks/stripe")
        .insert_header((
            "Stripe-Signature",
            format!("t={timestamp},v1={signature}"),
        ))
        .set_json(body)
        .to_request();

    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}
```

---

## Testing with fixtures and seed data

### YAML seed files

Place seed files in `seeds/` for repeatable test data:

```yaml
# seeds/organizations.yaml
resource: organizations
records:
  - id: "11111111-1111-1111-1111-111111111111"
    name: Acme Corp
    plan: enterprise

# seeds/users.yaml
resource: users
records:
  - email: alice@acme.com
    name: Alice Admin
    role: admin
    org_id: "11111111-1111-1111-1111-111111111111"
  - email: bob@acme.com
    name: Bob Viewer
    role: viewer
    org_id: "11111111-1111-1111-1111-111111111111"
```

Load them before tests:

```bash
shaperail seed seeds/organizations.yaml
shaperail seed seeds/users.yaml
```

Or load the entire directory:

```bash
shaperail seed
```

### Programmatic fixtures in Rust

For integration tests, insert data directly through the test app or the pool:

```rust
/// Insert a test user and return its ID.
async fn insert_test_user(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    email: &str,
    role: &str,
    org_id: &str,
) -> String {
    let req = actix_test::TestRequest::post()
        .uri("/v1/users")
        .set_json(json!({
            "email": email,
            "name": "Test User",
            "role": role,
            "org_id": org_id
        }))
        .to_request();
    let resp = actix_test::call_service(app, req).await;
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    body["data"]["id"].as_str().unwrap().to_string()
}
```

### Fixture files for SQL-level setup

For data that must exist before the test handler is configured, use SQL
fixture files alongside your test migrations:

```sql
-- tests/fixtures/migrations/02_seed_test_data.sql
INSERT INTO organizations (id, name)
VALUES ('11111111-1111-1111-1111-111111111111', 'Test Org')
ON CONFLICT DO NOTHING;
```

---

## Integration tests with `test_support`

`shaperail-runtime` ships a `test-support` cargo feature that provides
`TestServer`, `spawn_with_listener`, and `ensure_migrations_run`. These let you
spin up the full Actix server in-process on an ephemeral port and make real HTTP
requests against it — without mocking any layer.

> **Note:** Future versions of `shaperail init` will generate the lib/bin split
> described here automatically. Until then, the steps below are a one-time edit
> per project.

### Step 1 — split `src/main.rs` into a library + binary

Add an explicit `[lib]` target to `Cargo.toml` alongside the binary, and pull
in the dev-dependencies:

```toml
[lib]
path = "src/lib.rs"

[[bin]]
name = "my-app"
path = "src/main.rs"

[dev-dependencies]
shaperail-runtime = { workspace = true, features = ["test-support"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

### Step 2 — expose `build_server` from `src/lib.rs`

Move the existing bootstrap logic (config, pool, registry, route registration)
into a public async function that accepts a `TcpListener` and returns the
unawaited `actix_web::dev::Server`. The function is async because realistic
bootstrap code connects a sqlx pool, generates OpenAPI docs, builds resource
registries, etc., all of which are async operations:

```rust
// src/lib.rs
use std::net::TcpListener;
use actix_web::dev::Server;

pub async fn build_server(listener: TcpListener) -> std::io::Result<Server> {
    // ... async config, pool setup, resource registry, middleware ...
    let server = actix_web::HttpServer::new(move || {
        // ... App::new().route(...) ...
    })
    .listen(listener)?
    .run();
    Ok(server)
}
```

### Step 3 — collapse `src/main.rs` to a thin caller

```rust
// src/main.rs
use std::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    my_app::build_server(listener)?.await
}
```

### Step 4 — write `tests/integration.rs`

```rust
// tests/integration.rs
use std::net::TcpListener;

#[tokio::test]
async fn health_responds_200() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = shaperail_runtime::test_support::spawn_with_listener(
        listener,
        |l| async move { my_app::build_server(l).await },
    )
    .await
    .unwrap();

    let resp = reqwest::get(server.url("/health")).await.unwrap();
    assert_eq!(resp.status(), 200);
}
```

If your `build_server` is synchronous (no async work in bootstrap), wrap it:

```rust
    let server = shaperail_runtime::test_support::spawn_with_listener(
        listener,
        |l| async move { my_app::build_server(l) },
    )
    .await
    .unwrap();
```

`spawn_with_listener` binds to port 0 (OS assigns an ephemeral port) and
returns a `TestServer` whose `Drop` aborts the spawned task. Tests that start
multiple server instances will each get a unique port with no conflicts.

### Running migrations once per test process

For database-backed integration tests, call `ensure_migrations_run` before your
first query. The helper is gated on a `tokio::sync::OnceCell`, so parallel
tests share a single migration sweep instead of contending on the Postgres
advisory lock.

Pass the path to your project's own `migrations/` directory. Use
`std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"))` for
an absolute path that works regardless of where `cargo test` is invoked from:

```rust
use std::path::Path;
use shaperail_runtime::test_support::ensure_migrations_run;

#[tokio::test]
async fn test_create_user(pool: sqlx::PgPool) {
    let migrations = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"));
    ensure_migrations_run(&pool, migrations).await.expect("migrations");
    // ... test body ...
}
```

---

## Reaching controller helpers from integration tests

The codegen exposes each `<resource>_controller` module under
`crate::resources::*` via a `#[doc(hidden)] pub mod resources` aggregator
in `generated/mod.rs`. Library projects publish this with one line in
`src/lib.rs`:

```rust
mod generated;
pub use generated::resources;
```

After this, integration tests in `tests/` import controller helpers
directly:

```rust
// tests/users.rs
use my_app::resources::users_controller::{create_user, NewUser};

#[tokio::test]
async fn create_user_normalizes_email() {
    let input = NewUser { email: "Alice@Example.COM".into(), /* ... */ };
    // ... call into the helper exactly as the runtime does ...
}
```

The `#[doc(hidden)]` attribute keeps the aggregator off the docs.rs surface
— it exists for test wiring, not as a public API. Binary-only projects
with no `tests/` crate do not need to add the `pub use` line; the
aggregator is still emitted but stays unreachable from outside
`src/lib.rs`.

---

## CI/CD testing patterns

### Docker-based CI

Run the full test suite without installing Rust locally using the project's
Docker Compose setup:

```bash
docker compose up -d
export DATABASE_URL=postgresql://shaperail:shaperail@localhost:5433/shaperail_dev
export REDIS_URL=redis://localhost:6379
cargo test --workspace
```

### GitHub Actions example

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

env:
  DATABASE_URL: postgresql://shaperail:shaperail@localhost:5433/shaperail_dev
  REDIS_URL: redis://localhost:6379

jobs:
  test:
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:16-alpine
        env:
          POSTGRES_DB: shaperail_dev
          POSTGRES_USER: shaperail
          POSTGRES_PASSWORD: shaperail
        ports:
          - 5433:5432
        options: >-
          --health-cmd "pg_isready -U shaperail -d shaperail_dev"
          --health-interval 5s
          --health-timeout 3s
          --health-retries 10

      redis:
        image: redis:7-alpine
        ports:
          - 6379:6379
        options: >-
          --health-cmd "redis-cli ping"
          --health-interval 5s
          --health-timeout 3s
          --health-retries 10

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt

      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Check formatting
        run: cargo fmt --check

      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: Run tests
        run: cargo test --workspace

      - name: Run benchmarks (compile only)
        run: cargo bench -p shaperail-runtime --no-run
```

### Parallelizing tests

By default, `cargo test` runs tests in parallel. The `#[sqlx::test]` macro
handles database isolation per test, so parallel execution is safe.

If you have tests that share external state (such as Redis keys), use a
`tokio::sync::Mutex` to serialize them:

```rust
use std::sync::OnceLock;

fn redis_test_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_cache_behavior(pool: sqlx::PgPool) {
    let _guard = redis_test_lock().lock().await;
    // This test now has exclusive Redis access
}
```

---

## Common pitfalls and solutions

### 1. `DATABASE_URL` not set

**Symptom:** Tests fail with "database URL not set" or connection refused.

**Solution:** Start the dev services and export the URL:

```bash
docker compose up -d
export DATABASE_URL=postgresql://shaperail:shaperail@localhost:5433/shaperail_dev
```

### 2. Port conflict on 5433

**Symptom:** `docker compose up` fails because port 5433 is already in use.

**Solution:** Stop any other PostgreSQL instances or change the port in
`docker-compose.yml`. Shaperail uses port 5433 (not the default 5432) to avoid
conflicts with a local PostgreSQL installation.

### 3. Tests interfere with each other

**Symptom:** Tests pass individually but fail when run together.

**Solution:** Use `#[sqlx::test]` for database isolation. For Redis-dependent
tests, use the mutex lock pattern shown above, and clear relevant cache keys
before each test:

```rust
async fn clear_resource_cache(pool: &deadpool_redis::Pool, resource: &str) {
    let mut conn = pool.get().await.expect("redis connection");
    let keys: Vec<String> = redis::cmd("KEYS")
        .arg(format!("shaperail:{resource}:*"))
        .query_async(&mut conn)
        .await
        .unwrap_or_default();
    if !keys.is_empty() {
        let _: usize = redis::AsyncCommands::del(&mut conn, keys)
            .await
            .expect("clear cache");
    }
}
```

### 4. Stale test migrations

**Symptom:** Tests fail after changing a resource schema because the test
migration still creates the old table structure.

**Solution:** Update `tests/fixtures/migrations/` to match the current schema.
Test migrations are separate from app migrations and must be kept in sync
manually.

### 5. Async test runtime errors

**Symptom:** `#[tokio::test]` panics with "cannot start a runtime from within a runtime."

**Solution:** Use `#[sqlx::test]` for tests that need a database pool. It
manages the Tokio runtime internally. Do not nest `#[tokio::test]` inside
`#[sqlx::test]`.

### 6. Controller tests fail without a database

**Symptom:** Unit tests for controllers that use `ctx.pool` fail because there
is no database connection.

**Solution:** If your controller queries the database, it needs an integration
test with `#[sqlx::test]`. For controllers that only manipulate `ctx.input` or
`ctx.data`, you can create a mock context without a real pool -- but if the
function touches `ctx.pool`, use a real database.

### 7. Webhook tests are flaky

**Symptom:** Webhook delivery tests fail intermittently because the async
event processing has not completed.

**Solution:** Add a short delay after the triggering request to allow background
processing to finish:

```rust
tokio::time::sleep(std::time::Duration::from_millis(200)).await;
```

For more reliable assertions, poll the expected state with a timeout instead of
a fixed sleep.

### 8. Test naming conventions

Follow the project naming convention for discoverability:

```rust
#[test]
fn test_<thing>_<condition>_<expected_outcome>() { ... }

// Examples:
fn test_field_type_uuid_parses_correctly() { ... }
fn test_list_endpoint_without_auth_returns_401() { ... }
fn test_soft_delete_hides_record_from_list() { ... }
```

---

## Summary

| Test type | Tool | Database required | Location |
| --- | --- | --- | --- |
| Unit tests (controllers, pure logic) | `#[tokio::test]` | No (unless controller queries DB) | `src/` inline `#[cfg(test)]` modules |
| Integration tests (HTTP endpoints) | `#[sqlx::test]` + `actix_web::test` | Yes | `tests/` directory |
| Job handler tests | `#[tokio::test]` | No | `src/` or `tests/` |
| Event and webhook tests | `#[sqlx::test]` + mock server | Yes | `tests/` directory |
| CLI smoke tests | `assert_cmd` | No | `tests/` directory |
| Benchmarks | Criterion | No | `benches/` directory |
