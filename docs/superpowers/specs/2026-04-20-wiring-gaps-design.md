# Wiring Gaps — Design Spec
**Date:** 2026-04-20
**Status:** Approved
**Scope:** Auto-connect four manual-wiring primitives so a generated app works without `main.rs` bootstrapping

---

## Problem

Four runtime systems exist and are documented but require manual registration code in `main.rs` before they do anything. A developer who declares `controller: { before: validate_org }` in YAML, creates `resources/users.controller.rs`, and runs `shaperail generate` still gets a non-functioning controller — because `build_controller_map()` returns empty. The same gap exists for jobs, WebSocket channels, and inbound webhook routes. Each gap contradicts the framework's core value: declare in YAML, get a working backend.

---

## Goals

After this change, a project where:
- resources declare `controller:` hooks → controller functions are registered automatically
- resources declare `jobs:` names → job handlers are registered and a worker starts automatically
- `channels/*.channel.yaml` files exist → WebSocket routes are registered automatically
- `events.inbound` entries exist in config → inbound webhook routes are registered automatically

No `main.rs` changes required. `shaperail generate` does the wiring.

---

## Out of Scope

- Changing how users write controller functions or job handlers (same Rust files, same signatures)
- Event subscriber dispatch (already works from `shaperail.config.yaml`)
- gRPC Update RPC (separate gap)
- WebSocket hook dispatch (on_connect/on_disconnect/on_message hooks remain manual for now)

---

## Architecture

Four independent changes that follow one pattern: **codegen or runtime reads declarations → generates/loads registration code → scaffold template calls it**.

| Gap | Fix location | Mechanism |
|-----|-------------|-----------|
| Controllers | `shaperail-codegen` | `build_controller_map()` populated from resource YAML |
| Jobs | `shaperail-codegen` + scaffold | `build_job_registry()` generated; scaffold starts `Worker` |
| WebSockets | `shaperail-runtime` + scaffold | `load_channels()` reads YAML at startup; scaffold loops routes |
| Events inbound | `shaperail-runtime` + scaffold | `configure_inbound_routes()` registers POST handlers from config |

---

## Change 1: Controllers

**File:** `shaperail-codegen/src/rust.rs` — `generate_registry_module()`

### Current output
```rust
pub fn build_controller_map() -> shaperail_runtime::handlers::controller::ControllerMap {
    shaperail_runtime::handlers::controller::ControllerMap::new()
}
```

### New output (when any resource declares controller hooks)
```rust
#[path = "../resources/users.controller.rs"]
mod users_controller;

#[path = "../resources/orders.controller.rs"]
mod orders_controller;

pub fn build_controller_map() -> shaperail_runtime::handlers::controller::ControllerMap {
    let mut map = shaperail_runtime::handlers::controller::ControllerMap::new();
    map.register("users", "validate_org", users_controller::validate_org);
    map.register("orders", "check_inventory", orders_controller::check_inventory);
    map
}
```

### Stub file generation
When `shaperail generate` finds a declared controller function and the `.controller.rs` file does not yet exist, it writes a stub:

```rust
// resources/users.controller.rs — generated stub
pub async fn validate_org(
    _ctx: &shaperail_runtime::handlers::ControllerContext,
    _input: &serde_json::Value,
) -> Result<(), shaperail_core::ShaperailError> {
    todo!("implement validate_org")
}
```

If the file already exists, it is never overwritten. Compiler errors for undeclared functions are intentional — the compiler enforces that every declared hook is implemented.

### Path convention
`#[path]` is relative to `generated/mod.rs`. Resources are always at `../resources/`. Jobs at `../jobs/`.

---

## Change 2: Jobs

**Files:**
- `shaperail-codegen/src/rust.rs` — add `generate_job_registry()`
- `shaperail-cli/src/commands/init.rs` — scaffold template: call `build_job_registry()`, start `Worker`

### New codegen output
```rust
#[path = "../jobs/send_welcome_email.rs"]
mod job_send_welcome_email;

#[path = "../jobs/notify_on_call.rs"]
mod job_notify_on_call;

pub fn build_job_registry() -> shaperail_runtime::jobs::JobRegistry {
    let mut handlers = std::collections::HashMap::new();
    handlers.insert(
        "send_welcome_email".to_string(),
        std::sync::Arc::new(|payload| Box::pin(job_send_welcome_email::handle(payload))),
    );
    handlers.insert(
        "notify_on_call".to_string(),
        std::sync::Arc::new(|payload| Box::pin(job_notify_on_call::handle(payload))),
    );
    shaperail_runtime::jobs::JobRegistry::from_handlers(handlers)
}
```

Job names are collected from `jobs:` lists across all resources (deduplicated). If no resource declares any jobs, `build_job_registry()` returns `JobRegistry::new()` and the scaffold does not start a worker.

### Stub file generation
```rust
// jobs/send_welcome_email.rs — generated stub
pub async fn handle(
    _payload: serde_json::Value,
) -> Result<(), shaperail_core::ShaperailError> {
    todo!("implement send_welcome_email")
}
```

Existing files are never overwritten.

### Scaffold template addition
After the pool and Redis are set up, and before `HttpServer::new`:
```rust
let job_registry = generated::build_job_registry();
if !job_registry.is_empty() {
    if let Some(ref jq) = job_queue {
        let (_, shutdown_rx) = tokio::sync::watch::channel(false);
        let worker = shaperail_runtime::jobs::Worker::new(
            jq.clone(),
            job_registry,
            std::time::Duration::from_secs(1),
        );
        tokio::spawn(async move { worker.spawn(shutdown_rx).await });
    }
}
```

`JobRegistry` needs an `is_empty()` method added (returns `true` when `handlers` map is empty).

---

## Change 3: WebSockets

**Files:**
- `shaperail-runtime/src/ws/session.rs` — add `load_channels()`
- `shaperail-cli/src/commands/init.rs` — scaffold template: load channels + loop routes

### New runtime function
```rust
/// Reads all `*.channel.yaml` files from `dir` and returns parsed channel definitions.
/// Returns an empty vec if the directory does not exist or contains no channel files.
pub fn load_channels(dir: &std::path::Path) -> Vec<shaperail_core::ChannelDefinition> {
    // glob dir/*.channel.yaml, parse each with serde_yaml
    // skip files that fail to parse (log warning)
    // return collected definitions
}
```

### Scaffold template addition
At startup (before `HttpServer::new`):
```rust
let channels = shaperail_runtime::ws::load_channels(std::path::Path::new("channels/"));
```

Inside the app factory (conditional on Redis being configured, same guard as existing `ws_pubsub`):
```rust
if let (Some(ref pubsub), Some(ref rm), Some(ref jwt)) = (ws_pubsub, room_manager, jwt_config) {
    for channel in &channels {
        shaperail_runtime::ws::configure_ws_routes(
            cfg,
            channel.clone(),
            rm.clone(),
            pubsub.clone(),
            jwt.clone(),
        );
    }
}
```

### Behaviour
- No `channels/` directory → `load_channels` returns empty vec → no WS routes registered → no error
- Channel YAML parse error → warning logged, file skipped, server starts normally

---

## Change 4: Events inbound routes

**Files:**
- `shaperail-runtime/src/events/emitter.rs` — add `configure_inbound_routes()`
- `shaperail-cli/src/commands/init.rs` — scaffold template: call `configure_inbound_routes()`

### `InboundWebhookConfig` addition (`shaperail-core/src/config.rs`)

Add one optional field:

```rust
pub struct InboundWebhookConfig {
    pub path: String,
    pub secret_env: String,
    pub events: Vec<String>,
    /// HTTP header carrying the HMAC-SHA256 signature. Defaults to `X-Webhook-Signature`.
    /// Set to `X-Hub-Signature-256` for GitHub, `Stripe-Signature` for Stripe, etc.
    #[serde(default = "default_signature_header")]
    pub signature_header: String,
}

fn default_signature_header() -> String { "X-Webhook-Signature".to_string() }
```

### New runtime function
```rust
/// Registers inbound webhook routes from config onto the Actix ServiceConfig.
///
/// Each entry in `inbound` becomes a `POST <path>` route that:
/// 1. Reads the raw body
/// 2. Verifies HMAC-SHA256 signature from the header named by `signature_header`
/// 3. Parses the body as JSON
/// 4. Emits each event in the payload via the EventEmitter
pub fn configure_inbound_routes(
    cfg: &mut web::ServiceConfig,
    inbound: &[shaperail_core::InboundWebhookConfig],
    emitter: std::sync::Arc<EventEmitter>,
)
```

Signature verification: reads the env var named by `secret_env`, checks HMAC-SHA256 hex of raw body against `signature_header` value. Returns `401` on mismatch, `400` on unparseable body.

### Scaffold template addition
Inside the app factory:
```rust
if let Some(ref events_cfg) = config.events {
    if !events_cfg.inbound.is_empty() {
        if let Some(ref emitter) = event_emitter {
            shaperail_runtime::events::configure_inbound_routes(
                cfg,
                &events_cfg.inbound,
                emitter.clone(),
            );
        }
    }
}
```

---

## Testing

Each change has unit tests in its crate and an integration test in `shaperail-runtime`:

| Change | Test |
|--------|------|
| Controllers codegen | Snapshot test: resource with `controller:` produces populated `build_controller_map()` |
| Jobs codegen | Snapshot test: resources with `jobs:` produce `build_job_registry()` with correct names |
| `load_channels()` | Unit test: reads fixture YAML files, returns correct `ChannelDefinition` values; missing dir returns empty |
| `configure_inbound_routes()` | Unit test: valid HMAC → 200; wrong HMAC → 401; missing header → 401; bad JSON → 400 |
| Stub generation (both) | Test: stub file written when target file missing; existing file not overwritten |

---

## Files Changed

| File | Change |
|------|--------|
| `shaperail-codegen/src/rust.rs` | Populate `build_controller_map()`; add `build_job_registry()` generation; add stub file writing for controllers and jobs |
| `shaperail-runtime/src/ws/session.rs` | Add `load_channels()` |
| `shaperail-core/src/config.rs` | Add `signature_header` field to `InboundWebhookConfig` |
| `shaperail-runtime/src/events/emitter.rs` | Add `configure_inbound_routes()` |
| `shaperail-runtime/src/jobs/worker.rs` | Add `JobRegistry::is_empty()` |
| `shaperail-cli/src/commands/init.rs` | Update scaffold `main.rs` template: job worker startup, channel loading, inbound route registration |
