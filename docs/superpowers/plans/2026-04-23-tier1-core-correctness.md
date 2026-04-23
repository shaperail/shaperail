# Tier 1 — Core Correctness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the four gaps that block real-world usage: gRPC Update RPC, event subscriber auto-wiring from resource YAML, saga state machine auto-wiring, and custom endpoint auto-wiring.

**Architecture:** Four independent features implemented in dependency order (gRPC Update → Event Subscribers → Custom Endpoints → Sagas). gRPC Update is a single function addition. Event subscribers add a new YAML field that gets collected at startup. Custom endpoints add a handler registry to AppState with codegen-generated stubs. Sagas are the most complex: new module, DB table, state machine, HTTP routes.

**Tech Stack:** Rust, Actix-web 4, sqlx (Postgres), prost (protobuf), serde_yaml, tokio, reqwest

---

## File Map

### gRPC Update RPC
- Modify: `shaperail-runtime/src/grpc/service.rs` — add `pub async fn handle_update`
- Modify: `shaperail-runtime/src/grpc/server.rs:89-90` — wire handle_update in dispatch match arm
- Create: `shaperail-runtime/tests/grpc_service_tests.rs` — integration tests for gRPC handlers

### Event Subscriber Auto-Wiring
- Modify: `shaperail-core/src/endpoint.rs` — add `SubscriberSpec` struct, add `subscribers` field to `EndpointSpec`
- Modify: `shaperail-core/src/lib.rs` — re-export `SubscriberSpec`
- Modify: `shaperail-cli/src/commands/init.rs` (line ~1462) — collect resource subscribers, merge into EventEmitter
- Modify: `shaperail-codegen/src/diagnostics.rs` — add SR073 (empty subscriber event) and SR074 (empty subscriber handler)

### Custom Endpoint Auto-Wiring
- Modify: `shaperail-core/src/endpoint.rs` — add `handler: Option<String>` field to `EndpointSpec`
- Create: `shaperail-runtime/src/handlers/custom.rs` — `CustomHandlerFn` type, `handle_custom` generic route handler
- Modify: `shaperail-runtime/src/handlers/mod.rs` — expose `custom` module
- Modify: `shaperail-runtime/src/handlers/routes.rs` — add custom endpoint catch-all after known actions
- Modify: `shaperail-runtime/src/handlers/crud.rs` — add `custom_handlers: Option<CustomHandlerMap>` to `AppState`
- Modify: `shaperail-codegen/src/rust.rs` — generate `resources/<name>.handlers.rs` with handler stubs
- Modify: `shaperail-cli/src/commands/init.rs` — auto-register custom handlers from generated map at startup
- Modify: `shaperail-codegen/src/diagnostics.rs` — add SR075 (non-convention endpoint missing handler)

### Saga Auto-Wiring
- Create: `shaperail-runtime/src/sagas/mod.rs` — module exports
- Create: `shaperail-runtime/src/sagas/executor.rs` — `SagaExecutor`: start, advance, compensate, get_status
- Create: `shaperail-runtime/src/sagas/handler.rs` — HTTP handlers for `POST /v1/sagas/{name}` and `GET /v1/sagas/{id}`
- Modify: `shaperail-runtime/src/lib.rs` — expose `pub mod sagas`
- Modify: `shaperail-runtime/src/handlers/crud.rs` — add `saga_executor: Option<Arc<SagaExecutor>>` to `AppState`
- Modify: `shaperail-cli/src/commands/init.rs` — wire SagaExecutor from workspace.yaml at startup; register saga routes

---

## Task 1: gRPC Update — Failing Integration Test

**Files:**
- Create: `shaperail-runtime/tests/grpc_service_tests.rs`

- [ ] **Step 1: Create the test file with a failing test**

```rust
//! Integration tests for gRPC service handlers.
//! Run with: cargo test -p shaperail-runtime --test grpc_service_tests

use std::sync::Arc;

use indexmap::IndexMap;
use prost::bytes::BytesMut;
use shaperail_core::{EndpointSpec, FieldSchema, FieldType, HttpMethod, ResourceDefinition};
use shaperail_runtime::grpc::service;
use shaperail_runtime::handlers::crud::AppState;
use shaperail_runtime::observability::MetricsState;

fn test_resource() -> ResourceDefinition {
    let mut schema = IndexMap::new();
    schema.insert(
        "id".to_string(),
        FieldSchema {
            field_type: FieldType::String,
            primary: true,
            generated: true,
            required: true,
            unique: true,
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
    let mut endpoints = std::collections::HashMap::new();
    endpoints.insert(
        "update".to_string(),
        EndpointSpec {
            method: Some(HttpMethod::Patch),
            path: Some("/grpc_test_items/:id".to_string()),
            auth: None,
            input: Some(vec!["name".to_string()]),
            ..Default::default()
        },
    );
    ResourceDefinition {
        resource: "grpc_test_items".to_string(),
        version: 1,
        schema,
        endpoints: Some(endpoints),
        relations: None,
        indexes: None,
    }
}

fn make_state(pool: sqlx::PgPool) -> Arc<AppState> {
    Arc::new(AppState {
        pool,
        resources: vec![],
        stores: None,
        controllers: None,
        jwt_config: None,
        cache: None,
        event_emitter: None,
        job_queue: None,
        rate_limiter: None,
        custom_handlers: None,
        saga_executor: None,
        metrics: Some(MetricsState::new().expect("metrics")),
        wasm_runtime: None,
        event_bus: tokio::sync::broadcast::channel(16).0,
    })
}

/// Encode a gRPC update request: field 1 = id (string), field 2 = name (string).
fn encode_update_request(id: &str, name: &str) -> prost::bytes::Bytes {
    let mut buf = BytesMut::new();
    // Field 1: id
    prost::encoding::encode_key(1, prost::encoding::WireType::LengthDelimited, &mut buf);
    prost::encoding::encode_varint(id.len() as u64, &mut buf);
    buf.extend_from_slice(id.as_bytes());
    // Field 2: name
    prost::encoding::encode_key(2, prost::encoding::WireType::LengthDelimited, &mut buf);
    prost::encoding::encode_varint(name.len() as u64, &mut buf);
    buf.extend_from_slice(name.as_bytes());
    buf.freeze()
}

#[sqlx::test]
async fn grpc_handle_update_changes_record(pool: sqlx::PgPool) {
    sqlx::query("CREATE TABLE grpc_test_items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO grpc_test_items (id, name) VALUES ('item-1', 'original')")
        .execute(&pool)
        .await
        .unwrap();

    let state = make_state(pool.clone());
    let resource = test_resource();
    let request_bytes = encode_update_request("item-1", "updated");

    let result = service::handle_update(state, &resource, None, &request_bytes).await;
    assert!(result.is_ok(), "handle_update failed: {:?}", result.err());

    let row: (String,) =
        sqlx::query_as("SELECT name FROM grpc_test_items WHERE id = 'item-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, "updated");
}

#[sqlx::test]
async fn grpc_handle_update_missing_id_returns_error(pool: sqlx::PgPool) {
    let state = make_state(pool);
    let resource = test_resource();

    let result = service::handle_update(state, &resource, None, &[]).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}
```

- [ ] **Step 2: Run the test — confirm it fails with compile error (handle_update not found)**

```bash
cargo test -p shaperail-runtime --test grpc_service_tests 2>&1 | head -30
```

Expected: compile error mentioning `handle_update` not found in `service`.

---

## Task 2: gRPC Update — Implementation

**Files:**
- Modify: `shaperail-runtime/src/grpc/service.rs`

- [ ] **Step 1: Add `handle_update` to `shaperail-runtime/src/grpc/service.rs` before the `#[cfg(test)]` block**

```rust
/// Handle an Update RPC: updates a resource record by ID.
pub async fn handle_update(
    state: Arc<AppState>,
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    request_data: &[u8],
) -> Result<Bytes, Status> {
    let endpoint = resource.endpoints.as_ref().and_then(|e| e.get("update"));
    let auth_rule = endpoint.and_then(|e| e.auth.as_ref());
    enforce_auth(auth_rule, user)?;

    let input_fields = endpoint
        .and_then(|e| e.input.as_ref())
        .cloned()
        .unwrap_or_default();

    // Build combined decode schema: id first, then input fields
    let mut update_schema = indexmap::IndexMap::new();
    update_schema.insert(
        "id".to_string(),
        shaperail_core::FieldSchema {
            field_type: shaperail_core::FieldType::String,
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
    for field_name in &input_fields {
        if let Some(field) = resource.schema.get(field_name) {
            update_schema.insert(field_name.clone(), field.clone());
        }
    }

    let req_json = decode_resource_message(&update_schema, request_data);
    let id = req_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Status::invalid_argument("Missing 'id' field"))?;

    if input_fields.is_empty() {
        return Err(Status::invalid_argument(
            "Update endpoint has no input fields declared",
        ));
    }

    // BUILD: UPDATE {table} SET field1 = $1, field2 = $2, ... WHERE id = $N
    let table = &resource.resource;
    let set_clauses: Vec<String> = input_fields
        .iter()
        .enumerate()
        .map(|(i, f)| format!("{f} = ${}", i + 1))
        .collect();
    let set_clause = set_clauses.join(", ");
    let id_param = input_fields.len() + 1;
    let query = format!(
        "UPDATE {table} SET {set_clause} WHERE id = ${id_param} RETURNING row_to_json({table}.*)"
    );

    let mut q = sqlx::query_as::<_, (serde_json::Value,)>(&query);
    for field_name in &input_fields {
        let val = req_json
            .get(field_name)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        match val {
            serde_json::Value::String(s) => q = q.bind(s),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    q = q.bind(i.to_string());
                } else if let Some(f) = n.as_f64() {
                    q = q.bind(f.to_string());
                } else {
                    q = q.bind(n.to_string());
                }
            }
            serde_json::Value::Bool(b) => q = q.bind(b.to_string()),
            _ => q = q.bind(Option::<String>::None),
        }
    }
    q = q.bind(id);

    let row = q
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    let record = row.map(|(v,)| v).ok_or_else(|| Status::not_found("Not found"))?;

    // Encode response: field 1 = updated record
    let data_bytes = encode_resource_message(&resource.schema, &record);
    let mut response_buf = BytesMut::new();
    prost::encoding::encode_key(
        1,
        prost::encoding::WireType::LengthDelimited,
        &mut response_buf,
    );
    prost::encoding::encode_varint(data_bytes.len() as u64, &mut response_buf);
    response_buf.extend_from_slice(&data_bytes);

    Ok(response_buf.freeze())
}
```

- [ ] **Step 2: Wire `handle_update` in `shaperail-runtime/src/grpc/server.rs:89-90`**

Replace:
```rust
        } else if method_name.starts_with("Update") {
            Err(Status::unimplemented("Update not yet implemented"))
```

With:
```rust
        } else if method_name.starts_with("Update") {
            let data = service::handle_update(self.state.clone(), resource, user, body).await?;
            Ok(GrpcResponse::Unary(data))
```

- [ ] **Step 3: Run the integration tests**

```bash
cargo test -p shaperail-runtime --test grpc_service_tests 2>&1
```

Expected:
```
test grpc_handle_update_changes_record ... ok
test grpc_handle_update_missing_id_returns_error ... ok
```

- [ ] **Step 4: Run clippy**

```bash
cargo clippy -p shaperail-runtime -- -D warnings 2>&1
```

Expected: no warnings or errors.

- [ ] **Step 5: Commit**

```bash
git add shaperail-runtime/src/grpc/service.rs shaperail-runtime/src/grpc/server.rs shaperail-runtime/tests/grpc_service_tests.rs
git commit -m "feat(runtime): implement gRPC Update RPC"
```

---

## Task 3: Event Subscriber Schema

**Files:**
- Modify: `shaperail-core/src/endpoint.rs`
- Modify: `shaperail-core/src/lib.rs`

- [ ] **Step 1: Write a failing test in `shaperail-core/src/endpoint.rs`**

Add to the `#[cfg(test)]` block at the bottom of `endpoint.rs`:

```rust
#[test]
fn endpoint_spec_subscribers_parse() {
    let yaml = r#"
auth: [admin]
events: [user.created]
subscribers:
  - event: user.created
    handler: send_welcome_email
  - event: "*.deleted"
    handler: cleanup_resources
"#;
    let spec: EndpointSpec = serde_yaml::from_str(yaml).unwrap();
    let subs = spec.subscribers.as_ref().unwrap();
    assert_eq!(subs.len(), 2);
    assert_eq!(subs[0].event, "user.created");
    assert_eq!(subs[0].handler, "send_welcome_email");
    assert_eq!(subs[1].event, "*.deleted");
}

#[test]
fn subscriber_spec_unknown_field_rejected() {
    let yaml = r#"
subscribers:
  - event: user.created
    handler: send_welcome_email
    extra: bad_field
"#;
    let result = serde_yaml::from_str::<EndpointSpec>(yaml);
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run — confirm fails (SubscriberSpec not defined, subscribers field not on EndpointSpec)**

```bash
cargo test -p shaperail-core 2>&1 | grep -E "error|FAILED"
```

Expected: compile errors about unknown type `SubscriberSpec` and unknown field `subscribers`.

- [ ] **Step 3: Add `SubscriberSpec` struct and `subscribers` field**

In `shaperail-core/src/endpoint.rs`, add before the `EndpointSpec` struct:

```rust
/// A subscriber declaration within an endpoint — auto-registered at startup.
///
/// ```yaml
/// subscribers:
///   - event: user.created
///     handler: send_welcome_email
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriberSpec {
    /// Event name pattern (e.g., "user.created", "*.deleted").
    pub event: String,
    /// Handler function name in `resources/<resource>.controller.rs`.
    pub handler: String,
}
```

Then add the `subscribers` field to `EndpointSpec` (after the `rate_limit` field, before `soft_delete`):

```rust
    /// Event subscribers auto-registered at startup from this endpoint's events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscribers: Option<Vec<SubscriberSpec>>,
```

- [ ] **Step 4: Re-export `SubscriberSpec` from `shaperail-core/src/lib.rs`**

Find the existing re-exports in `lib.rs` (the `pub use endpoint::` line) and add `SubscriberSpec` to it:

```rust
pub use endpoint::{
    apply_endpoint_defaults, endpoint_convention, AuthRule, CacheSpec, ControllerSpec,
    EndpointSpec, HttpMethod, PaginationStyle, RateLimitSpec, SubscriberSpec, UploadSpec,
    WASM_HOOK_PREFIX,
};
```

- [ ] **Step 5: Run the tests**

```bash
cargo test -p shaperail-core 2>&1
```

Expected: all tests pass including the two new subscriber tests.

- [ ] **Step 6: Commit**

```bash
git add shaperail-core/src/endpoint.rs shaperail-core/src/lib.rs
git commit -m "feat(core): add SubscriberSpec to EndpointSpec for event subscriber auto-wiring"
```

---

## Task 4: Event Subscriber Runtime Wiring

**Files:**
- Modify: `shaperail-cli/src/commands/init.rs`

- [ ] **Step 1: Write a failing unit test in `init.rs` (or a new test file) for the subscriber collection helper**

Add a test for the helper function we're about to write. Add near the bottom of `init.rs`:

```rust
#[cfg(test)]
mod subscriber_tests {
    use super::*;
    use shaperail_core::{EndpointSpec, ResourceDefinition, SubscriberSpec};
    use std::collections::HashMap;

    fn resource_with_subscribers() -> ResourceDefinition {
        let mut endpoints = HashMap::new();
        endpoints.insert(
            "create".to_string(),
            EndpointSpec {
                events: Some(vec!["user.created".to_string()]),
                subscribers: Some(vec![
                    SubscriberSpec {
                        event: "user.created".to_string(),
                        handler: "send_welcome_email".to_string(),
                    },
                ]),
                ..Default::default()
            },
        );
        ResourceDefinition {
            resource: "users".to_string(),
            version: 1,
            schema: indexmap::IndexMap::new(),
            endpoints: Some(endpoints),
            relations: None,
            indexes: None,
        }
    }

    #[test]
    fn collect_resource_subscribers_extracts_all() {
        let resources = vec![resource_with_subscribers()];
        let subs = collect_resource_subscribers(&resources);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].event, "user.created");
        assert!(matches!(
            &subs[0].targets[0],
            shaperail_core::EventTarget::Hook { name } if name == "send_welcome_email"
        ));
    }

    #[test]
    fn collect_resource_subscribers_empty_when_none() {
        let resources: Vec<ResourceDefinition> = vec![];
        let subs = collect_resource_subscribers(&resources);
        assert!(subs.is_empty());
    }
}
```

- [ ] **Step 2: Run — confirm fails (collect_resource_subscribers not defined)**

```bash
cargo test -p shaperail-cli -- subscriber_tests 2>&1 | grep -E "error|FAILED"
```

Expected: compile error `collect_resource_subscribers` not found.

- [ ] **Step 3: Add `collect_resource_subscribers` to `init.rs`**

Add this function before the `run_serve` function in `init.rs`:

```rust
/// Collects event subscribers declared in resource endpoint YAML and converts
/// them to `EventSubscriber` entries pointing to hook targets.
///
/// Called at startup to merge resource-level subscribers into the EventEmitter
/// alongside any subscribers declared in `shaperail.config.yaml`.
fn collect_resource_subscribers(
    resources: &[shaperail_core::ResourceDefinition],
) -> Vec<shaperail_core::EventSubscriber> {
    let mut result = Vec::new();
    for resource in resources {
        let Some(endpoints) = &resource.endpoints else {
            continue;
        };
        for endpoint in endpoints.values() {
            let Some(subs) = &endpoint.subscribers else {
                continue;
            };
            for sub in subs {
                result.push(shaperail_core::EventSubscriber {
                    event: sub.event.clone(),
                    targets: vec![shaperail_core::EventTarget::Hook {
                        name: sub.handler.clone(),
                    }],
                });
            }
        }
    }
    result
}
```

- [ ] **Step 4: Merge resource subscribers into EventEmitter construction**

Find the line in `init.rs` (around line 1462) that reads:
```rust
    let event_emitter = job_queue
        .clone()
        .map(|queue| EventEmitter::new(queue, config.events.as_ref()));
```

Replace it with:
```rust
    // Merge config-level subscribers with resource-level subscribers.
    let merged_events: Option<shaperail_core::EventsConfig> =
        if resources.iter().any(|r| {
            r.endpoints.as_ref().map_or(false, |eps| {
                eps.values().any(|ep| ep.subscribers.is_some())
            })
        }) {
            let mut base = config
                .events
                .clone()
                .unwrap_or_else(|| shaperail_core::EventsConfig {
                    subscribers: vec![],
                    webhooks: None,
                    inbound: vec![],
                });
            base.subscribers
                .extend(collect_resource_subscribers(&resources));
            Some(base)
        } else {
            config.events.clone()
        };
    let event_emitter = job_queue
        .clone()
        .map(|queue| EventEmitter::new(queue, merged_events.as_ref()));
```

- [ ] **Step 5: Run the tests**

```bash
cargo test -p shaperail-cli -- subscriber_tests 2>&1
```

Expected: both subscriber tests pass.

- [ ] **Step 6: Build the workspace to verify no regressions**

```bash
cargo build --workspace 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add shaperail-cli/src/commands/init.rs
git commit -m "feat(cli): auto-wire resource-level event subscribers into EventEmitter at startup"
```

---

## Task 5: Event Subscriber Diagnostics

**Files:**
- Modify: `shaperail-codegen/src/diagnostics.rs`

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)]` block in `diagnostics.rs`:

```rust
#[test]
fn subscriber_with_empty_event_has_fix_suggestion() {
    let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    auth: [admin]
    subscribers:
      - event: ""
        handler: my_handler
"#;
    let rd = parse_resource(yaml).unwrap();
    let diags = diagnose_resource(&rd);
    let d = diags.iter().find(|d| d.code == "SR073");
    assert!(d.is_some(), "Expected SR073 diagnostic for empty subscriber event");
    assert!(d.unwrap().fix.contains("event"));
}

#[test]
fn subscriber_with_empty_handler_has_fix_suggestion() {
    let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    auth: [admin]
    subscribers:
      - event: items.created
        handler: ""
"#;
    let rd = parse_resource(yaml).unwrap();
    let diags = diagnose_resource(&rd);
    let d = diags.iter().find(|d| d.code == "SR074");
    assert!(d.is_some(), "Expected SR074 diagnostic for empty subscriber handler");
    assert!(d.unwrap().fix.contains("handler"));
}
```

- [ ] **Step 2: Run — confirm fails (SR073/SR074 codes not produced)**

```bash
cargo test -p shaperail-codegen -- subscriber_with_empty 2>&1
```

Expected: tests fail (assertions on `d.is_some()` fail).

- [ ] **Step 3: Add subscriber validation to `diagnose_resource` in `diagnostics.rs`**

In the `diagnose_resource` function, add after the existing endpoint checks (after the WASM path checks, before the closing `diags` return):

```rust
    // SR073 / SR074: subscriber event and handler must not be empty
    if let Some(endpoints) = &rd.endpoints {
        for (action, ep) in endpoints {
            if let Some(subs) = &ep.subscribers {
                for (i, sub) in subs.iter().enumerate() {
                    if sub.event.is_empty() {
                        diags.push(Diagnostic {
                            code: "SR073",
                            error: format!(
                                "resource '{}': endpoint '{}' subscriber[{}] has an empty event pattern",
                                rd.resource, action, i
                            ),
                            fix: "provide a non-empty event pattern (e.g., \"user.created\" or \"*.deleted\")".into(),
                            example: format!(
                                "subscribers:\n  - event: {}.created\n    handler: my_handler",
                                rd.resource
                            ),
                        });
                    }
                    if sub.handler.is_empty() {
                        diags.push(Diagnostic {
                            code: "SR074",
                            error: format!(
                                "resource '{}': endpoint '{}' subscriber[{}] has an empty handler name",
                                rd.resource, action, i
                            ),
                            fix: "provide the name of a Rust function in resources/<resource>.controller.rs".into(),
                            example: "subscribers:\n  - event: user.created\n    handler: send_welcome_email".into(),
                        });
                    }
                }
            }
        }
    }
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p shaperail-codegen -- subscriber_with_empty 2>&1
```

Expected: both tests pass.

- [ ] **Step 5: Run the full codegen test suite**

```bash
cargo test -p shaperail-codegen 2>&1
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add shaperail-codegen/src/diagnostics.rs
git commit -m "feat(codegen): add SR073/SR074 diagnostics for invalid subscriber declarations"
```

---

## Task 6: Custom Endpoint — Handler Field in EndpointSpec

**Files:**
- Modify: `shaperail-core/src/endpoint.rs`

- [ ] **Step 1: Write a failing test**

Add to the test module in `endpoint.rs`:

```rust
#[test]
fn custom_endpoint_handler_field_parses() {
    let yaml = r#"
method: POST
path: /invite
auth: [admin]
input: [email, role]
handler: invite_user
"#;
    let ep: EndpointSpec = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(ep.handler.as_deref(), Some("invite_user"));
    assert_eq!(ep.input.as_ref().unwrap().len(), 2);
}

#[test]
fn endpoint_without_handler_has_none() {
    let yaml = "auth: [member]\n";
    let ep: EndpointSpec = serde_yaml::from_str(yaml).unwrap();
    assert!(ep.handler.is_none());
}
```

- [ ] **Step 2: Run — confirm fails**

```bash
cargo test -p shaperail-core -- custom_endpoint_handler 2>&1
```

Expected: compile error — unknown field `handler` on `EndpointSpec`.

- [ ] **Step 3: Add `handler` field to `EndpointSpec`**

In `endpoint.rs`, add to `EndpointSpec` after the `subscribers` field:

```rust
    /// Handler function name for non-convention endpoints.
    /// Required when the endpoint action name is not list/get/create/update/delete.
    /// The function must be defined in `resources/<resource>.controller.rs`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p shaperail-core 2>&1
```

Expected: all tests pass including two new custom endpoint tests.

- [ ] **Step 5: Commit**

```bash
git add shaperail-core/src/endpoint.rs
git commit -m "feat(core): add handler field to EndpointSpec for custom endpoint auto-wiring"
```

---

## Task 7: Custom Handler Registry in AppState

**Files:**
- Create: `shaperail-runtime/src/handlers/custom.rs`
- Modify: `shaperail-runtime/src/handlers/mod.rs`
- Modify: `shaperail-runtime/src/handlers/crud.rs`

- [ ] **Step 1: Write a failing test in `custom.rs`**

Create `shaperail-runtime/src/handlers/custom.rs`:

```rust
//! Custom endpoint handler dispatch.
//!
//! Users declare non-CRUD endpoints in resource YAML with a `handler:` field.
//! The framework enforces auth, validation, and rate limiting; user code provides
//! only the business logic function.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse};
use shaperail_core::{EndpointSpec, ResourceDefinition};

use super::crud::AppState;

/// A custom handler function: receives request context, returns HTTP response.
pub type CustomHandlerFn = Arc<
    dyn Fn(
            HttpRequest,
            Arc<AppState>,
            Arc<ResourceDefinition>,
            Arc<EndpointSpec>,
        ) -> Pin<Box<dyn Future<Output = HttpResponse> + Send>>
        + Send
        + Sync,
>;

/// Registry mapping "{resource}:{action}" to a custom handler function.
pub type CustomHandlerMap = HashMap<String, CustomHandlerFn>;

/// Build the registry key for a custom handler.
pub fn handler_key(resource: &str, action: &str) -> String {
    format!("{resource}:{action}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_key_format() {
        assert_eq!(handler_key("users", "invite"), "users:invite");
        assert_eq!(handler_key("orders", "cancel"), "orders:cancel");
    }

    #[test]
    fn custom_handler_map_lookup() {
        let mut map: CustomHandlerMap = HashMap::new();
        let key = handler_key("users", "invite");
        let handler: CustomHandlerFn = Arc::new(|_req, _state, _res, _ep| {
            Box::pin(async { HttpResponse::Ok().finish() })
        });
        map.insert(key.clone(), handler);
        assert!(map.contains_key(&key));
        assert!(!map.contains_key("users:ban"));
    }
}
```

- [ ] **Step 2: Run — confirm it compiles and tests pass**

```bash
cargo test -p shaperail-runtime 2>&1 | grep -E "custom::tests|error"
```

Note: This test will pass because `custom.rs` is self-contained. The next step wires it into `AppState`, which may fail to compile until done.

- [ ] **Step 3: Expose `custom` module in `shaperail-runtime/src/handlers/mod.rs`**

Find `mod.rs` in `handlers/`. Add:

```rust
pub mod custom;
```

- [ ] **Step 4: Add `custom_handlers` field to `AppState` in `crud.rs`**

In `shaperail-runtime/src/handlers/crud.rs`, add to the `AppState` struct after the `rate_limiter` field:

```rust
    /// Custom endpoint handler registry. Keys are "{resource}:{action}".
    pub custom_handlers: Option<super::custom::CustomHandlerMap>,
```

- [ ] **Step 5: Fix all `AppState { ... }` constructor sites**

All places constructing `AppState` must add `custom_handlers: None`. Search:

```bash
grep -rn "AppState {" /Users/Mahin/Desktop/shaperail/ --include="*.rs" | grep -v "target/"
```

For each location found (expect: `api_integration.rs` × ~5, `db_integration.rs` × some, `handler_tests.rs` × some, `cli/commands/init.rs` × 1), add `custom_handlers: None,` to the struct literal.

- [ ] **Step 6: Build to confirm no regressions**

```bash
cargo build --workspace 2>&1 | grep "^error"
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add shaperail-runtime/src/handlers/custom.rs shaperail-runtime/src/handlers/mod.rs shaperail-runtime/src/handlers/crud.rs shaperail-runtime/tests/
git commit -m "feat(runtime): add CustomHandlerMap type and custom_handlers field to AppState"
```

---

## Task 8: Custom Endpoint Route Registration

**Files:**
- Modify: `shaperail-runtime/src/handlers/routes.rs`
- Create: `shaperail-runtime/src/handlers/custom_handler.rs`

- [ ] **Step 1: Write a failing integration test in `api_integration.rs`**

Add to `shaperail-runtime/tests/api_integration.rs`:

```rust
#[sqlx::test]
async fn custom_endpoint_dispatches_to_registered_handler(pool: sqlx::PgPool) {
    use shaperail_runtime::handlers::custom::{handler_key, CustomHandlerFn, CustomHandlerMap};
    use std::sync::Arc;

    let resource = ResourceDefinition {
        resource: "items".to_string(),
        version: 1,
        schema: {
            let mut s = IndexMap::new();
            s.insert(
                "id".to_string(),
                FieldSchema {
                    field_type: FieldType::Uuid,
                    primary: true,
                    generated: true,
                    required: true,
                    unique: true,
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
            s
        },
        endpoints: Some({
            let mut eps = std::collections::HashMap::new();
            eps.insert(
                "archive".to_string(),
                EndpointSpec {
                    method: Some(HttpMethod::Post),
                    path: Some("/items/:id/archive".to_string()),
                    auth: None,
                    handler: Some("archive_item".to_string()),
                    ..Default::default()
                },
            );
            eps
        }),
        relations: None,
        indexes: None,
    };

    let mut custom_handlers: CustomHandlerMap = std::collections::HashMap::new();
    custom_handlers.insert(
        handler_key("items", "archive"),
        Arc::new(|_req, _state, _res, _ep| {
            Box::pin(async {
                actix_web::HttpResponse::Ok()
                    .json(serde_json::json!({"status": "archived"}))
            })
        }),
    );

    let state = Arc::new(AppState {
        pool,
        resources: vec![resource.clone()],
        stores: None,
        controllers: None,
        jwt_config: None,
        cache: None,
        event_emitter: None,
        job_queue: None,
        rate_limiter: None,
        custom_handlers: Some(custom_handlers),
        saga_executor: None,
        metrics: Some(MetricsState::new().expect("metrics")),
        wasm_runtime: None,
        event_bus: tokio::sync::broadcast::channel(16).0,
    });

    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone())),
    )
    .await;

    let req = actix_test::TestRequest::post()
        .uri("/v1/items/some-id/archive")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    assert_eq!(body["status"], "archived");
}
```

- [ ] **Step 2: Run — confirm fails (custom endpoint not registered)**

```bash
cargo test -p shaperail-runtime --test api_integration custom_endpoint_dispatches 2>&1
```

Expected: test fails with 404 (route not registered).

- [ ] **Step 3: Add custom endpoint dispatch to `routes.rs`**

In `shaperail-runtime/src/handlers/routes.rs`, at the end of the `for (action, endpoint) in endpoints` loop, add a catch-all after the existing `"delete"` arm (after all the known action arms):

```rust
                action_name => {
                    // Non-convention endpoint: dispatch to registered custom handler.
                    let ep = ep_arc.clone();
                    let r = res.clone();
                    let action_owned = action_name.to_string();
                    let method = endpoint.method().clone();
                    let route = match method {
                        HttpMethod::Get => web::get(),
                        HttpMethod::Post => web::post(),
                        HttpMethod::Patch => web::patch(),
                        HttpMethod::Put => web::put(),
                        HttpMethod::Delete => web::delete(),
                    };
                    cfg.route(
                        &actix_path,
                        route.to(move |req: HttpRequest, state: web::Data<Arc<AppState>>| {
                            let ep = ep.clone();
                            let r = r.clone();
                            let action = action_owned.clone();
                            async move {
                                let resource_name = r.resource.clone();
                                let key = super::custom::handler_key(&resource_name, &action);
                                let handler = state
                                    .custom_handlers
                                    .as_ref()
                                    .and_then(|m| m.get(&key))
                                    .cloned();
                                match handler {
                                    Some(f) => f(req, state.get_ref().clone(), r, ep).await,
                                    None => actix_web::HttpResponse::NotImplemented()
                                        .json(serde_json::json!({
                                            "error": format!(
                                                "Custom handler '{}' not registered for {resource_name}:{action}",
                                                ep.handler.as_deref().unwrap_or("(none)")
                                            )
                                        })),
                                }
                            }
                        }),
                    );
                }
```

Note: this catch-all must come after the existing named arms (`"list"`, `"get"`, `"create"`, `"update"`, `"delete"`) to avoid overriding them. Place it as the final arm in the `match action.as_str()` block.

- [ ] **Step 4: Run the failing test**

```bash
cargo test -p shaperail-runtime --test api_integration custom_endpoint_dispatches 2>&1
```

Expected: test passes.

- [ ] **Step 5: Run the full integration test suite**

```bash
cargo test -p shaperail-runtime 2>&1
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add shaperail-runtime/src/handlers/routes.rs shaperail-runtime/tests/api_integration.rs
git commit -m "feat(runtime): register custom endpoint routes and dispatch to CustomHandlerMap"
```

---

## Task 9: Custom Endpoint Codegen and Diagnostics

**Files:**
- Modify: `shaperail-codegen/src/rust.rs`
- Modify: `shaperail-codegen/src/diagnostics.rs`

- [ ] **Step 1: Write failing test for SR075 diagnostic**

Add to `diagnostics.rs` test module:

```rust
#[test]
fn non_convention_endpoint_without_handler_produces_sr075() {
    let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  archive:
    method: POST
    path: /items/:id/archive
    auth: [admin]
"#;
    let rd = parse_resource(yaml).unwrap();
    let diags = diagnose_resource(&rd);
    let d = diags.iter().find(|d| d.code == "SR075");
    assert!(d.is_some(), "Expected SR075 for non-convention endpoint missing handler");
    assert!(d.unwrap().fix.contains("handler"));
}

#[test]
fn non_convention_endpoint_with_handler_no_sr075() {
    let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  archive:
    method: POST
    path: /items/:id/archive
    auth: [admin]
    handler: archive_item
"#;
    let rd = parse_resource(yaml).unwrap();
    let diags = diagnose_resource(&rd);
    let has_sr075 = diags.iter().any(|d| d.code == "SR075");
    assert!(!has_sr075, "SR075 should not fire when handler is present");
}
```

- [ ] **Step 2: Run — confirm fails**

```bash
cargo test -p shaperail-codegen -- non_convention_endpoint 2>&1
```

Expected: first test fails (SR075 not produced).

- [ ] **Step 3: Add SR075 check to `diagnose_resource`**

In `diagnostics.rs`, add to the endpoint validation section (alongside the existing endpoint checks):

```rust
    // SR075: non-convention endpoints must declare a handler
    const CONVENTIONS: &[&str] = &["list", "get", "create", "update", "delete"];
    if let Some(endpoints) = &rd.endpoints {
        for (action, ep) in endpoints {
            if !CONVENTIONS.contains(&action.as_str()) && ep.handler.is_none() {
                diags.push(Diagnostic {
                    code: "SR075",
                    error: format!(
                        "resource '{}': endpoint '{}' is not a standard action (list/get/create/update/delete) and has no 'handler:' declared",
                        rd.resource, action
                    ),
                    fix: "add a 'handler: <function_name>' field pointing to a function in resources/<resource>.controller.rs".into(),
                    example: format!(
                        "{action}:\n  method: POST\n  path: /{name}/{action}\n  auth: [admin]\n  handler: {action}_{name}",
                        action = action,
                        name = rd.resource
                    ),
                });
            }
        }
    }
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p shaperail-codegen 2>&1
```

Expected: all tests pass including the two new SR075 tests.

- [ ] **Step 5: Commit**

```bash
git add shaperail-codegen/src/diagnostics.rs
git commit -m "feat(codegen): add SR075 diagnostic for non-convention endpoints missing handler declaration"
```

---

## Task 10: Saga DB Migration and Types

**Files:**
- Create: `shaperail-runtime/src/sagas/mod.rs`
- Create: `shaperail-runtime/src/sagas/executor.rs`
- Modify: `shaperail-runtime/src/lib.rs`

- [ ] **Step 1: Create `mod.rs` for the sagas module**

Create `shaperail-runtime/src/sagas/mod.rs`:

```rust
//! Saga orchestration: distributed multi-step transactions with compensating actions.

pub mod executor;
pub mod handler;

pub use executor::SagaExecutor;
```

- [ ] **Step 2: Create `executor.rs` with the SQL schema constant and tests**

Create `shaperail-runtime/src/sagas/executor.rs`:

```rust
//! Saga state machine executor.
//!
//! Persists saga execution state to Postgres and drives forward/compensate steps
//! by making HTTP calls to target services.

use std::collections::HashMap;
use std::sync::Arc;

use shaperail_core::{SagaDefinition, SagaExecutionStatus};
use sqlx::PgPool;

/// SQL to create the saga_executions table (run once at startup if not exists).
pub const CREATE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS saga_executions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    saga_name   TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'running',
    current_step INTEGER NOT NULL DEFAULT 0,
    step_results JSONB NOT NULL DEFAULT '[]',
    input       JSONB,
    error       TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
)
"#;

/// Runtime record for a saga execution.
#[derive(Debug, sqlx::FromRow)]
pub struct SagaExecution {
    pub id: uuid::Uuid,
    pub saga_name: String,
    pub status: String,
    pub current_step: i32,
    pub step_results: serde_json::Value,
    pub input: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Drives saga state machines: starts executions, advances steps, triggers compensation.
pub struct SagaExecutor {
    pool: PgPool,
    /// Maps service name → base URL (e.g., "inventory-api" → "http://inventory:8080")
    service_urls: HashMap<String, String>,
    http_client: reqwest::Client,
}

impl SagaExecutor {
    /// Create a new executor with a DB pool and service URL map.
    pub fn new(pool: PgPool, service_urls: HashMap<String, String>) -> Self {
        Self {
            pool,
            service_urls,
            http_client: reqwest::Client::new(),
        }
    }

    /// Ensure the saga_executions table exists.
    pub async fn ensure_table(&self) -> Result<(), sqlx::Error> {
        sqlx::query(CREATE_TABLE_SQL).execute(&self.pool).await?;
        Ok(())
    }

    /// Start a new saga execution. Returns the execution ID.
    pub async fn start(
        &self,
        saga: &SagaDefinition,
        input: serde_json::Value,
    ) -> Result<uuid::Uuid, crate::error::RuntimeError> {
        let row: (uuid::Uuid,) = sqlx::query_as(
            "INSERT INTO saga_executions (saga_name, input) VALUES ($1, $2) RETURNING id",
        )
        .bind(&saga.saga)
        .bind(&input)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| crate::error::RuntimeError::Database(e.to_string()))?;

        let execution_id = row.0;
        // Kick off the first step asynchronously
        let executor = Arc::new(Self {
            pool: self.pool.clone(),
            service_urls: self.service_urls.clone(),
            http_client: reqwest::Client::new(),
        });
        let saga_clone = saga.clone();
        tokio::spawn(async move {
            if let Err(e) = executor.advance(&execution_id, &saga_clone).await {
                tracing::error!(execution_id = %execution_id, error = %e, "Saga advance failed");
            }
        });

        Ok(execution_id)
    }

    /// Advance the saga by executing the current step.
    pub async fn advance(
        self: &Arc<Self>,
        execution_id: &uuid::Uuid,
        saga: &SagaDefinition,
    ) -> Result<SagaExecutionStatus, crate::error::RuntimeError> {
        let exec: SagaExecution =
            sqlx::query_as("SELECT * FROM saga_executions WHERE id = $1")
                .bind(execution_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| crate::error::RuntimeError::Database(e.to_string()))?;

        let step_index = exec.current_step as usize;
        if step_index >= saga.steps.len() {
            // All steps done
            self.update_status(execution_id, SagaExecutionStatus::Completed, None)
                .await?;
            return Ok(SagaExecutionStatus::Completed);
        }

        let step = &saga.steps[step_index];
        let base_url = self
            .service_urls
            .get(&step.service)
            .ok_or_else(|| crate::error::RuntimeError::Config(format!(
                "Service '{}' not in service registry",
                step.service
            )))?;

        // Parse "METHOD /path" from step.action
        let (method, path) = parse_action(&step.action)?;
        let url = format!("{base_url}{path}");
        let input = exec.input.clone().unwrap_or(serde_json::Value::Null);

        let response = self
            .http_client
            .request(method, &url)
            .json(&input)
            .timeout(std::time::Duration::from_secs(step.timeout_secs))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                let result = resp
                    .json::<serde_json::Value>()
                    .await
                    .unwrap_or(serde_json::Value::Null);
                // Append result to step_results, advance step counter
                sqlx::query(
                    "UPDATE saga_executions
                     SET current_step = current_step + 1,
                         step_results = step_results || $1::jsonb,
                         updated_at = NOW()
                     WHERE id = $2",
                )
                .bind(serde_json::json!([result]))
                .bind(execution_id)
                .execute(&self.pool)
                .await
                .map_err(|e| crate::error::RuntimeError::Database(e.to_string()))?;

                // Continue advancing
                self.advance(execution_id, saga).await
            }
            Ok(resp) => {
                let error_msg = format!("Step '{}' failed with HTTP {}", step.name, resp.status());
                self.update_status(execution_id, SagaExecutionStatus::Compensating, Some(&error_msg))
                    .await?;
                // Trigger compensation
                let executor = Arc::clone(self);
                let exec_id = *execution_id;
                let saga_clone = saga.clone();
                tokio::spawn(async move {
                    if let Err(e) = executor.compensate(&exec_id, &saga_clone).await {
                        tracing::error!(execution_id = %exec_id, error = %e, "Saga compensation failed");
                    }
                });
                Ok(SagaExecutionStatus::Compensating)
            }
            Err(e) => {
                let error_msg = format!("Step '{}' request error: {e}", step.name);
                self.update_status(execution_id, SagaExecutionStatus::Compensating, Some(&error_msg))
                    .await?;
                Ok(SagaExecutionStatus::Compensating)
            }
        }
    }

    /// Run compensating actions for all completed steps in reverse order.
    pub async fn compensate(
        self: &Arc<Self>,
        execution_id: &uuid::Uuid,
        saga: &SagaDefinition,
    ) -> Result<(), crate::error::RuntimeError> {
        let exec: SagaExecution =
            sqlx::query_as("SELECT * FROM saga_executions WHERE id = $1")
                .bind(execution_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| crate::error::RuntimeError::Database(e.to_string()))?;

        let completed_steps = exec.current_step as usize;
        let step_results: Vec<serde_json::Value> = serde_json::from_value(exec.step_results)
            .unwrap_or_default();

        // Run compensating actions in reverse
        for i in (0..completed_steps).rev() {
            let step = &saga.steps[i];
            let base_url = match self.service_urls.get(&step.service) {
                Some(url) => url.clone(),
                None => continue,
            };

            let (method, path) = match parse_action(&step.compensate) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Substitute :id from step result if present
            let result_id = step_results
                .get(i)
                .and_then(|r| r.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let path = path.replace(":id", result_id);
            let url = format!("{base_url}{path}");

            let _ = self
                .http_client
                .request(method, &url)
                .timeout(std::time::Duration::from_secs(step.timeout_secs))
                .send()
                .await;
        }

        self.update_status(execution_id, SagaExecutionStatus::Compensated, None)
            .await?;
        Ok(())
    }

    /// Get the current execution status by ID.
    pub async fn get_status(
        &self,
        execution_id: &uuid::Uuid,
    ) -> Result<SagaExecution, crate::error::RuntimeError> {
        sqlx::query_as("SELECT * FROM saga_executions WHERE id = $1")
            .bind(execution_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| crate::error::RuntimeError::Database(e.to_string()))?
            .ok_or_else(|| crate::error::RuntimeError::NotFound)
    }

    async fn update_status(
        &self,
        execution_id: &uuid::Uuid,
        status: SagaExecutionStatus,
        error: Option<&str>,
    ) -> Result<(), crate::error::RuntimeError> {
        sqlx::query(
            "UPDATE saga_executions SET status = $1, error = $2, updated_at = NOW() WHERE id = $3",
        )
        .bind(status.to_string())
        .bind(error)
        .bind(execution_id)
        .execute(&self.pool)
        .await
        .map_err(|e| crate::error::RuntimeError::Database(e.to_string()))?;
        Ok(())
    }
}

/// Parse "METHOD /path" into (reqwest::Method, String).
fn parse_action(action: &str) -> Result<(reqwest::Method, String), crate::error::RuntimeError> {
    let parts: Vec<&str> = action.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(crate::error::RuntimeError::Config(format!(
            "Invalid saga action format: '{action}' — expected 'METHOD /path'"
        )));
    }
    let method = parts[0]
        .parse::<reqwest::Method>()
        .map_err(|_| crate::error::RuntimeError::Config(format!("Unknown HTTP method: {}", parts[0])))?;
    Ok((method, parts[1].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_post() {
        let (method, path) = parse_action("POST /v1/reservations").unwrap();
        assert_eq!(method, reqwest::Method::POST);
        assert_eq!(path, "/v1/reservations");
    }

    #[test]
    fn parse_action_delete_with_id() {
        let (method, path) = parse_action("DELETE /v1/reservations/:id").unwrap();
        assert_eq!(method, reqwest::Method::DELETE);
        assert_eq!(path, "/v1/reservations/:id");
    }

    #[test]
    fn parse_action_invalid_format() {
        let result = parse_action("not-valid");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Check if `RuntimeError` exists in runtime; if not, look up the error type used**

```bash
grep -rn "pub enum RuntimeError\|RuntimeError" /Users/Mahin/Desktop/shaperail/shaperail-runtime/src/ | head -10
```

If `RuntimeError` does not exist, replace it with `ShaperailError` from `shaperail_core` throughout `executor.rs`. Specifically: replace `crate::error::RuntimeError::Database(e.to_string())` with `shaperail_core::ShaperailError::Internal(e.to_string())`, `RuntimeError::Config` with `ShaperailError::Internal`, `RuntimeError::NotFound` with `ShaperailError::NotFound`, and update the return types accordingly.

- [ ] **Step 4: Expose sagas in `shaperail-runtime/src/lib.rs`**

Add to the module declarations in `lib.rs`:

```rust
pub mod sagas;
```

- [ ] **Step 5: Run unit tests for the saga module**

```bash
cargo test -p shaperail-runtime sagas 2>&1
```

Expected: `parse_action_post`, `parse_action_delete_with_id`, `parse_action_invalid_format` all pass.

- [ ] **Step 6: Commit**

```bash
git add shaperail-runtime/src/sagas/ shaperail-runtime/src/lib.rs
git commit -m "feat(runtime): add SagaExecutor with start/advance/compensate state machine"
```

---

## Task 11: Saga HTTP Handlers and Routes

**Files:**
- Create: `shaperail-runtime/src/sagas/handler.rs`
- Modify: `shaperail-runtime/src/handlers/crud.rs` — add `saga_executor` to `AppState`
- Modify: `shaperail-cli/src/commands/init.rs` — wire SagaExecutor at startup and register routes

- [ ] **Step 1: Create `handler.rs`**

Create `shaperail-runtime/src/sagas/handler.rs`:

```rust
//! HTTP handlers for saga route endpoints.

use std::sync::Arc;

use actix_web::{web, HttpResponse, Responder};
use shaperail_core::SagaDefinition;
use uuid::Uuid;

use crate::handlers::crud::AppState;

/// POST /v1/sagas/{name} — start a saga execution.
pub async fn start_saga(
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
    state: web::Data<Arc<AppState>>,
    sagas: web::Data<Vec<SagaDefinition>>,
) -> impl Responder {
    let name = path.into_inner();
    let Some(executor) = state.saga_executor.as_ref() else {
        return HttpResponse::ServiceUnavailable()
            .json(serde_json::json!({"error": "Saga executor not configured"}));
    };
    let Some(saga) = sagas.iter().find(|s| s.saga == name) else {
        return HttpResponse::NotFound()
            .json(serde_json::json!({"error": format!("Saga '{name}' not found")}));
    };

    match executor.start(saga, body.into_inner()).await {
        Ok(execution_id) => HttpResponse::Accepted()
            .json(serde_json::json!({ "execution_id": execution_id })),
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// GET /v1/sagas/{id} — get saga execution status.
pub async fn get_saga_status(
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> impl Responder {
    let id_str = path.into_inner();
    let Ok(id) = id_str.parse::<Uuid>() else {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"error": "Invalid execution ID format"}));
    };
    let Some(executor) = state.saga_executor.as_ref() else {
        return HttpResponse::ServiceUnavailable()
            .json(serde_json::json!({"error": "Saga executor not configured"}));
    };

    match executor.get_status(&id).await {
        Ok(exec) => HttpResponse::Ok().json(serde_json::json!({
            "id": exec.id,
            "saga_name": exec.saga_name,
            "status": exec.status,
            "current_step": exec.current_step,
            "error": exec.error,
        })),
        Err(_) => HttpResponse::NotFound()
            .json(serde_json::json!({ "error": "Execution not found" })),
    }
}
```

- [ ] **Step 2: Add `saga_executor` to `AppState`**

In `shaperail-runtime/src/handlers/crud.rs`, add to `AppState` after `custom_handlers`:

```rust
    /// Saga execution engine. Present when sagas are defined in workspace.yaml.
    pub saga_executor: Option<Arc<crate::sagas::SagaExecutor>>,
```

- [ ] **Step 3: Add `saga_executor: None` to all `AppState { ... }` constructor sites**

Run:
```bash
grep -rn "AppState {" /Users/Mahin/Desktop/shaperail/ --include="*.rs" | grep -v "target/"
```

Add `saga_executor: None,` to every constructor site found in test files and CLI init.

- [ ] **Step 4: Add `load_sagas` helper and wire SagaExecutor in `init.rs`**

First, add the `load_sagas` helper function to `shaperail-cli/src/commands/init.rs` alongside the existing `load_channels` function:

```rust
/// Load all saga definitions from `sagas/*.saga.yaml` files.
fn load_sagas(dir: &std::path::Path) -> Vec<shaperail_core::SagaDefinition> {
    if !dir.exists() {
        return vec![];
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut sagas = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml")
            && path.to_str().map_or(false, |s| s.contains(".saga."))
        {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(saga) = serde_yaml::from_str::<shaperail_core::SagaDefinition>(&content) {
                    sagas.push(saga);
                }
            }
        }
    }
    sagas
}
```

Then, in `shaperail-cli/src/commands/init.rs`, after the existing `let state = Arc::new(AppState { ... })` block, add:

```rust
    // Build SagaExecutor if workspace.yaml declares sagas
    let saga_defs = load_sagas(std::path::Path::new("sagas/"));
    let saga_executor = if !saga_defs.is_empty() {
        let service_urls: std::collections::HashMap<String, String> =
            config.workspace.as_ref()
                .map(|w| {
                    w.services
                        .iter()
                        .map(|(name, svc)| (name.clone(), svc.base_url.clone()))
                        .collect()
                })
                .unwrap_or_default();
        let executor = Arc::new(shaperail_runtime::sagas::SagaExecutor::new(
            pool.clone(),
            service_urls,
        ));
        if let Err(e) = executor.ensure_table().await {
            tracing::warn!("Failed to create saga_executions table: {e}");
        }
        Some(executor)
    } else {
        None
    };
```

Then update the `AppState` construction to include:
```rust
        saga_executor: saga_executor.clone(),
```

In the Actix-web app configuration closure, register saga routes when sagas are present:
```rust
    let saga_defs_clone = saga_defs.clone();
    let saga_executor_clone = saga_executor.clone();
    // Inside the HttpServer::new closure, after existing route registrations:
    if !saga_defs_clone.is_empty() {
        cfg.app_data(web::Data::new(saga_defs_clone.clone()))
           .route("/v1/sagas/{name}", web::post().to(shaperail_runtime::sagas::handler::start_saga))
           .route("/v1/sagas/{id}", web::get().to(shaperail_runtime::sagas::handler::get_saga_status));
    }
```

- [ ] **Step 5: Build workspace**

```bash
cargo build --workspace 2>&1 | grep "^error"
```

Expected: no errors. Fix any missing imports or type errors that arise.

- [ ] **Step 6: Run full test suite**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all existing tests pass. Any new failures indicate a regression; investigate before continuing.

- [ ] **Step 7: Commit**

```bash
git add shaperail-runtime/src/sagas/ shaperail-runtime/src/handlers/crud.rs shaperail-cli/src/commands/init.rs
git commit -m "feat(runtime,cli): wire SagaExecutor with HTTP routes POST/GET /v1/sagas"
```

---

## Task 12: Cross-Protocol Auth Consistency Test

**Files:**
- Modify: `shaperail-runtime/tests/api_integration.rs`

This task verifies that auth rules enforce identically across REST and GraphQL for the same credentials — the exit criterion from the spec.

- [ ] **Step 1: Add cross-protocol auth test**

Add to `api_integration.rs`:

```rust
#[sqlx::test]
async fn cross_protocol_auth_member_gets_same_result_via_rest_and_graphql(pool: sqlx::PgPool) {
    let resource = ResourceDefinition {
        resource: "auth_items".to_string(),
        version: 1,
        schema: {
            let mut s = IndexMap::new();
            s.insert("id".to_string(), FieldSchema { field_type: FieldType::Uuid, primary: true, generated: true, required: true, unique: true, nullable: false, reference: None, min: None, max: None, format: None, values: None, default: None, sensitive: false, search: false, items: None });
            s.insert("name".to_string(), FieldSchema { field_type: FieldType::String, primary: false, generated: false, required: true, unique: false, nullable: false, reference: None, min: None, max: None, format: None, values: None, default: None, sensitive: false, search: false, items: None });
            s
        },
        endpoints: Some({
            let mut eps = std::collections::HashMap::new();
            eps.insert("list".to_string(), EndpointSpec {
                method: Some(HttpMethod::Get),
                path: Some("/auth_items".to_string()),
                auth: Some(AuthRule::Roles(vec!["admin".to_string()])),
                ..Default::default()
            });
            eps
        }),
        relations: None,
        indexes: None,
    };

    sqlx::query("CREATE TABLE auth_items (id UUID PRIMARY KEY DEFAULT gen_random_uuid(), name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();

    let jwt = test_jwt();
    let state = Arc::new(AppState {
        pool: pool.clone(),
        resources: vec![resource.clone()],
        stores: None,
        controllers: None,
        jwt_config: Some(Arc::new(jwt.clone())),
        cache: None,
        event_emitter: None,
        job_queue: None,
        rate_limiter: None,
        custom_handlers: None,
        saga_executor: None,
        metrics: Some(MetricsState::new().expect("metrics")),
        wasm_runtime: None,
        event_bus: tokio::sync::broadcast::channel(16).0,
    });

    let gql_schema = build_schema(vec![resource.clone()], pool.clone());
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(|cfg| register_resource(cfg, &resource, state.clone()))
            .route("/graphql", web::post().to(graphql_handler))
            .app_data(web::Data::new(gql_schema)),
    ).await;

    // Member token (not admin)
    let member_token = jwt.encode_for_user("user-1", "member").unwrap();

    // REST: GET /v1/auth_items with member token → 403
    let rest_req = actix_test::TestRequest::get()
        .uri("/v1/auth_items")
        .insert_header(("Authorization", format!("Bearer {member_token}")))
        .to_request();
    let rest_resp = actix_test::call_service(&app, rest_req).await;
    assert_eq!(rest_resp.status(), 403, "REST: member should get 403 on admin-only endpoint");

    // GraphQL: query auth_items with member token → should also be denied
    let gql_body = serde_json::json!({ "query": "{ listAuthItems { id } }" });
    let gql_req = actix_test::TestRequest::post()
        .uri("/graphql")
        .insert_header(("Authorization", format!("Bearer {member_token}")))
        .insert_header(("Content-Type", "application/json"))
        .set_json(&gql_body)
        .to_request();
    let gql_resp = actix_test::call_service(&app, gql_req).await;
    let gql_body: serde_json::Value = actix_test::read_body_json(gql_resp).await;
    assert!(
        gql_body["errors"].is_array() && !gql_body["errors"].as_array().unwrap().is_empty(),
        "GraphQL: member should get errors on admin-only query, got: {gql_body}"
    );
}
```

- [ ] **Step 2: Run the test**

```bash
cargo test -p shaperail-runtime --test api_integration cross_protocol_auth 2>&1
```

Expected: test passes. If GraphQL does not enforce auth consistently, this test will fail and the auth enforcement must be fixed in the GraphQL resolver before this test is green.

- [ ] **Step 3: Run the full suite one final time**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 4: Run clippy**

```bash
cargo clippy --workspace -- -D warnings 2>&1
```

Expected: clean.

- [ ] **Step 5: Final commit**

```bash
git add shaperail-runtime/tests/api_integration.rs
git commit -m "test(runtime): add cross-protocol auth consistency test (REST + GraphQL)"
```

---

## Self-Review Checklist

- **gRPC Update**: `handle_update` implemented and wired in dispatch. Integration test verifies record changes and missing-id returns 422. ✓
- **Event Subscriber**: `SubscriberSpec` added to `EndpointSpec`. Resource subscribers collected at startup and merged into `EventEmitter`. SR073/SR074 diagnostics added. ✓
- **Custom Endpoints**: `handler` field added to `EndpointSpec`. `CustomHandlerMap` in `AppState`. Route catch-all dispatches to registered handler. SR075 diagnostic added. ✓
- **Sagas**: `SagaExecutor` with start/advance/compensate. DB table created at startup. `GET /v1/sagas/:id` and `POST /v1/sagas/:name` routes registered. ✓
- **Cross-protocol auth**: consistency test added for REST + GraphQL. ✓

**Type consistency check:**
- `CustomHandlerMap` defined in `custom.rs`, imported as `super::custom::CustomHandlerMap` in `crud.rs`. ✓
- `SagaExecutor` defined in `sagas/executor.rs`, re-exported as `crate::sagas::SagaExecutor` via `mod.rs`. ✓
- `SubscriberSpec` defined in `endpoint.rs`, re-exported from `shaperail-core/src/lib.rs`. ✓

**No placeholders**: all steps contain actual code. ✓
