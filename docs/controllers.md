---
title: Controllers
parent: Guides
nav_order: 3
---

# Controllers

Controllers let you run synchronous business logic before or after a database
operation, within the same HTTP request. Use them for input validation,
normalization, authorization checks, response enrichment, and computed fields.

For background work that should not block the response, use
[jobs]({{ '/background-jobs/' | relative_url }}) instead.

---

## Declaring controllers

Add a `controller` field to any write endpoint in your resource YAML:

```yaml
resource: users
version: 1

schema:
  id:         { type: uuid, primary: true, generated: true }
  email:      { type: string, format: email, unique: true, required: true }
  name:       { type: string, min: 1, max: 200, required: true }
  role:       { type: enum, values: [admin, member, viewer], default: member }
  org_id:     { type: uuid, ref: organizations.id, required: true }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }

endpoints:
  create:
    method: POST
    path: /users
    auth: [admin]
    input: [email, name, role, org_id]
    controller:
      before: validate_org
      after: enrich_response
    events: [user.created]
    jobs: [send_welcome_email]

  update:
    method: PATCH
    path: /users/:id
    auth: [admin, owner]
    input: [name, role]
    controller:
      before: normalize_name
```

Each endpoint supports at most one `before` and one `after` function. Both are
optional — you can declare just `before`, just `after`, or both.

---

## Writing controller functions

Controller functions live in a file co-located with the resource YAML:

```text
resources/
  users.yaml                # schema + endpoints
  users.controller.rs       # all controller functions for users
  orders.yaml
  orders.controller.rs
```

Two files give you the complete picture of a resource — the YAML declares what
exists, the controller file declares how it behaves.

Each function is a named async function that takes `&mut Context`:

```rust
// resources/users.controller.rs
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use shaperail_core::ShaperailError;

/// Called before create — normalize email and validate org exists.
pub async fn validate_org(ctx: &mut Context) -> ControllerResult {
    // Normalize email to lowercase
    if let Some(email) = ctx.input.get("email").and_then(|v| v.as_str()) {
        ctx.input["email"] = serde_json::json!(email.to_lowercase());
    }

    // Validate that org_id references a real organization
    if let Some(org_id) = ctx.input.get("org_id").and_then(|v| v.as_str()) {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM organizations WHERE id = $1)"
        )
        .bind(org_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap_or(false);

        if !exists {
            return Err(ShaperailError::Validation(vec![
                shaperail_core::FieldError {
                    field: "org_id".into(),
                    message: "organization does not exist".into(),
                    code: "invalid_reference".into(),
                },
            ]));
        }
    }

    Ok(())
}

/// Called after create — add computed fields to the response.
pub async fn enrich_response(ctx: &mut Context) -> ControllerResult {
    if let Some(data) = &mut ctx.data {
        data["display_name"] = serde_json::json!(
            format!("{} ({})",
                data["name"].as_str().unwrap_or(""),
                data["role"].as_str().unwrap_or("member")
            )
        );
    }
    Ok(())
}

/// Called before update — normalize the name field.
pub async fn normalize_name(ctx: &mut Context) -> ControllerResult {
    if let Some(name) = ctx.input.get("name").and_then(|v| v.as_str()) {
        let trimmed = name.trim().to_string();
        ctx.input["name"] = serde_json::json!(trimmed);
    }
    Ok(())
}
```

Function names in the controller file must match what is declared in the YAML.

---

## ControllerContext API

The `Context` struct is the single type passed to all controller functions:

| Field | Type | Available | Description |
| --- | --- | --- | --- |
| `input` | `serde_json::Map<String, Value>` | before + after | Mutable request input. Before-controllers can modify what gets written to the database. |
| `data` | `Option<serde_json::Value>` | after only | The database result. `None` in before-controllers, `Some(...)` in after-controllers. After-controllers can modify the response. |
| `user` | `Option<AuthenticatedUser>` | before + after | The authenticated user from the JWT or API key, if present. Contains `id`, `role`, and `tenant_id`. |
| `pool` | `sqlx::PgPool` | before + after | Database connection pool for running custom queries. |
| `headers` | `HashMap<String, String>` | before + after | Read-only copy of the request headers. |
| `response_headers` | `Vec<(String, String)>` | before + after | Push `(name, value)` pairs to add extra response headers. |
| `tenant_id` | `Option<String>` | before + after | The tenant ID from the JWT claim, when the resource has `tenant_key` set. Use for tenant-specific business logic. |

---

## Request lifecycle

Controllers run synchronously within the request. The full handler flow is:

```
auth check
  → extract input
    → validate fields
      → BEFORE controller
        → DB operation (insert / update / delete)
      → AFTER controller
    → side effects (cache invalidation, events, jobs)
  → HTTP response
```

Key behaviors:

- **Before-controllers** run after validation but before the DB write. They can
  modify `ctx.input` (e.g., normalize email, inject tenant ID) or return
  `Err(...)` to halt the request.
- **After-controllers** run after the DB write. They can modify `ctx.data`
  (e.g., strip internal fields, add computed values) or add response headers.
- If a controller returns `Err(ShaperailError::...)`, the request is aborted
  with the corresponding error response. For before-controllers, the DB
  operation is skipped entirely.

---

## ControllerMap registry

Generated code populates a `ControllerMap` at startup. The map stores
`(resource_name, function_name) → function` entries and is shared across all
request handlers via `AppState`.

```rust
// In your generated main.rs (produced by `shaperail generate`)
let mut controllers = ControllerMap::new();
controllers.register("users", "validate_org", users_controller::validate_org);
controllers.register("users", "enrich_response", users_controller::enrich_response);
controllers.register("users", "normalize_name", users_controller::normalize_name);
```

You do not write this wiring by hand — `shaperail generate` reads the YAML
declarations and emits the registry code.

---

## Common patterns

### Auto-fill `created_by` from token

```rust
pub async fn set_created_by(ctx: &mut Context) -> ControllerResult {
    if let Some(user) = &ctx.user {
        ctx.input["created_by"] = serde_json::json!(user.user_id);
    } else {
        return Err(ShaperailError::Auth("No authenticated user".into()));
    }
    Ok(())
}
```

### Strip internal fields from the response

```rust
pub async fn strip_internals(ctx: &mut Context) -> ControllerResult {
    if let Some(data) = &mut ctx.data {
        if let Some(obj) = data.as_object_mut() {
            obj.remove("internal_score");
            obj.remove("admin_notes");
        }
    }
    Ok(())
}
```

### Conditional logic based on role

```rust
pub async fn admin_only_fields(ctx: &mut Context) -> ControllerResult {
    let is_admin = ctx.user.as_ref().map_or(false, |u| u.role == "admin");
    if !is_admin {
        ctx.input.remove("role");
        ctx.input.remove("org_id");
    }
    Ok(())
}
```

### Add custom response headers

```rust
pub async fn add_deprecation_header(ctx: &mut Context) -> ControllerResult {
    ctx.response_headers.push((
        "Deprecation".into(),
        "true".into(),
    ));
    ctx.response_headers.push((
        "Sunset".into(),
        "2026-06-01".into(),
    ));
    Ok(())
}
```

---

## Controllers vs jobs vs events

| Mechanism | When it runs | Blocks response | Use case |
| --- | --- | --- | --- |
| `controller` | In the request | Yes | Validation, normalization, response enrichment, auth checks |
| `jobs` | Background (Redis queue) | No | Sending emails, generating reports, external API calls |
| `events` | After response (async) | No | Audit logs, webhooks, WebSocket broadcasts |

Use controllers for logic that must complete before the response is sent. Use
jobs for work that can happen later. Use events to notify other systems.

---

## Migration from hooks

In v0.2.x, the `hooks:` field enqueued functions as background jobs — identical
to `jobs:`. In v0.3.0, `hooks:` was removed and replaced with `controller:` for
synchronous in-request logic.

If your resource files use `hooks:`, update them:

```yaml
# v0.2.x (no longer valid)
endpoints:
  create:
    hooks: [validate_org]

# v0.3.0
endpoints:
  create:
    controller:
      before: validate_org
```

Using the old `hooks:` field now produces a clear "unknown field" error thanks to
`deny_unknown_fields` on all Shaperail types.

---

## WASM plugins

WASM plugins let you write controller hooks in any language that compiles to
WebAssembly — TypeScript, Python, Rust, Go, or C. Plugins run in a fully
sandboxed environment with no filesystem, network, env, or clock access.

### Declaring WASM hooks

Use the `wasm:` prefix in the `before` or `after` field to point to a `.wasm`
file:

```yaml
endpoints:
  create:
    method: POST
    path: /users
    auth: [admin]
    input: [email, name, role, org_id]
    controller:
      before: "wasm:./plugins/validate_email.wasm"
      after: "wasm:./plugins/enrich_response.wasm"
```

You can mix Rust and WASM controllers across endpoints, but each `before` or
`after` slot is either a Rust function name or a `wasm:` path — not both.

### Plugin interface

WASM modules must export these functions:

| Export | Signature | Description |
| --- | --- | --- |
| `memory` | `(memory 2)` | Linear memory (at least 2 pages / 128 KB) |
| `alloc` | `(i32) -> i32` | Allocate bytes in guest memory, return pointer |
| `dealloc` | `(i32, i32)` | Free memory (ptr, size) |
| `before_hook` | `(i32, i32) -> i64` | Before DB op: receives `(ptr, len)` of JSON context, returns packed `(result_ptr << 32) \| result_len` |
| `after_hook` | `(i32, i32) -> i64` | After DB op (optional, same signature) |

### Context JSON (input)

The host serializes the controller context as JSON and writes it into guest
memory:

```json
{
  "input": { "name": "Alice", "email": "alice@example.com" },
  "data": null,
  "user": { "id": "user-123", "role": "admin" },
  "headers": { "content-type": "application/json" },
  "tenant_id": null
}
```

`data` is `null` in before-hooks and contains the DB result in after-hooks.

### Result JSON (output)

Return `{"ok": true}` for a no-op passthrough:

```json
{"ok": true}
```

Return modified context to change input or data:

```json
{
  "ok": true,
  "ctx": {
    "input": { "name": "alice", "email": "alice@example.com" },
    "data": null,
    "user": null,
    "headers": {},
    "tenant_id": null
  }
}
```

Return an error to halt the request:

```json
{
  "ok": false,
  "error": "validation failed: email is required"
}
```

### Sandboxing

Plugins run with zero host capabilities by default:

- **No filesystem** — cannot read or write files
- **No network** — cannot make HTTP calls or open sockets
- **No environment** — cannot access env vars or system clock
- **Fuel-limited** — execution is capped to prevent infinite loops
- **Memory-limited** — default 16 MB per instance

Each request creates a fresh WASM instance, so plugins cannot retain state
between calls.

### Crash isolation

If a plugin traps (e.g., out-of-bounds memory access, unreachable instruction,
fuel exhaustion), the server does **not** crash. The request returns a
validation error and the server continues serving other requests.

### Compiling plugins

From **TypeScript** (AssemblyScript):
```bash
npm install -g assemblyscript
asc validate_email.ts --outFile validate_email.wasm --exportRuntime
```

From **Python** (componentize-py):
```bash
pip install componentize-py
componentize-py -d normalize_input.py -o normalize_input.wasm
```

From **Rust**:
```bash
cargo build --target wasm32-unknown-unknown --release
```

See `examples/wasm-plugins/` for complete TypeScript and Python plugin
examples.
