# Shaperail Testing Strategy

## Layer-by-Layer Rules

### shaperail-core (unit tests only)
- Test every FieldType variant parses correctly from YAML
- Test validation logic for every field constraint
- Test error formatting and conversion
- No DB, no HTTP, no async — pure functions only
- Location: `shaperail-core/src/` inline `#[cfg(test)]` modules

### shaperail-codegen (unit + snapshot tests)
- Test YAML → ResourceDefinition parsing for valid and invalid inputs
- Snapshot test generated Rust code using `insta` crate
  - If generated code changes, snapshot must be explicitly approved
- Test that invalid resource files produce correct error messages
- Do NOT test that generated code compiles here — that's shaperail-runtime's job
- Location: `shaperail-codegen/tests/`

### shaperail-runtime (integration tests — require running Postgres + Redis)
- Use `sqlx::test` macro — spins up isolated DB per test, auto-rollback
- Test every generated endpoint: happy path, auth failure, validation failure, not found
- Test cache invalidation: verify Redis key is deleted after write
- Test soft delete: verify deleted records don't appear in list
- Test pagination: cursor and offset, edge cases (empty page, last page)
- Location: `shaperail-runtime/tests/`
- Test files:
  - `tests/db_integration.rs` — DB layer: CRUD, pagination, filters, sort, soft/hard delete
  - `tests/api_integration.rs` — Full HTTP stack: Actix handlers with real DB, auth, validation
  - `tests/handler_tests.rs` — Handler unit tests: response envelopes, validation, auth, cache keys

### shaperail-runtime (benchmarks — no DB or Redis required)
- Use Criterion for CPU benchmarks of hot paths
- Location: `shaperail-runtime/benches/`
- Benchmark files:
  - `benches/health_response.rs` — health handler + response serialization throughput
  - `benches/throughput.rs` — JSON serialization, validation, query building, cache keys, parsing
- Run with: `cargo bench -p shaperail-runtime`
- PRD targets: 150K+ req/s JSON response, 80K+ cached reads, 20K+ writes

### shaperail-cli (end-to-end tests)
- Test `shaperail init` produces correct file structure
- Test `shaperail generate` produces files that compile (`cargo check`)
- Test `shaperail validate` catches invalid resource files
- Use `assert_cmd` crate for CLI testing
- Location: `shaperail-cli/tests/`

## Test Naming Convention
```rust
#[test]
fn test_<thing>_<condition>_<expected_outcome>() { ... }

// Examples:
fn test_field_type_uuid_parses_correctly() { ... }
fn test_list_endpoint_without_auth_returns_401() { ... }
fn test_soft_delete_hides_record_from_list() { ... }
fn test_codegen_emits_correct_handler_for_crud_resource() { ... }
```

## Test Data Pattern
```rust
// Always use builder pattern for test fixtures
fn user_fixture() -> CreateUserInput {
    CreateUserInput {
        email: "test@example.com".into(),
        name: "Test User".into(),
        role: None,
        org_id: Uuid::new_v4(),
    }
}
```

## What Must Always Be Tested Before Commit
1. The specific function/module you changed
2. Any hook that touches the changed resource
3. The endpoint that calls the changed function
4. Run: `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`

## Integration tests (`tests/integration.rs`) — `test_support` pattern

`shaperail_runtime::test_support` (behind the `test-support` cargo feature) ships an in-process server-spawn helper modeled on the zero2prod `TestApp` pattern. To use it, expose your project's bootstrap as a `build_server(listener) -> std::io::Result<Server>` from `src/lib.rs`, and call it from `tests/integration.rs`:

**`Cargo.toml`** — add an explicit `[lib]` target alongside the binary, plus dev-deps:

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

**`src/lib.rs`** — extract the existing bootstrap into an async function that takes a `TcpListener` and returns the unawaited `actix_web::dev::Server`. The function is async because realistic bootstrap code connects a sqlx pool, generates OpenAPI docs, builds resource registries, etc.:

```rust
use std::net::TcpListener;
use actix_web::dev::Server;

pub async fn build_server(listener: TcpListener) -> std::io::Result<Server> {
    // ... your existing bootstrap (config, pool, registry, channels, etc.) ...
    let server = HttpServer::new(move || { /* ... */ })
        .listen(listener)?
        .run();
    Ok(server)
}
```

**`src/main.rs`** — collapse to a thin caller:

```rust
use std::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = std::env::var("PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(3000);
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    my_app::build_server(listener).await?.await
}
```

**`tests/integration.rs`**:

```rust
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

`spawn_with_listener` returns a `TestServer` whose `Drop` aborts the spawned task. For database-backed tests, run migrations once per process via `shaperail_runtime::test_support::ensure_migrations_run(&pool, migrations_dir).await?` — the helper uses a `tokio::sync::OnceCell` so parallel tests share a single sweep instead of contending on the migration advisory lock. Pass the consumer's own migrations directory (e.g. `Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"))`); the runtime `Migrator::new` API resolves the path at runtime rather than at macro-expansion time, so the path always points at the consumer's migrations even when called through the helper crate.

Future versions of `shaperail init` will generate this lib/bin split for you. Until then, the manual lift above is a one-time edit per project.

## Current Test Counts (as of v0.2.2)
| Crate | Tests | Notes |
|-------|-------|-------|
| shaperail-core | 59 | All enum variants, struct fields, error shapes |
| shaperail-codegen | 60 | 45 unit + 15 insta snapshots |
| shaperail-runtime | 220 | 164 unit + 43 handler + 12 DB integration + 1 doc-test |
| shaperail-cli | 36 | 29 assert_cmd + 7 seed unit tests |
| **Total** | **385** | 0 failures, 0 ignored |
