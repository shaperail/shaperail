# Shaperail Controller System

## What Controllers Are
Controllers are the escape hatch for custom business logic that cannot be
expressed declaratively in the resource YAML. They replace the previous hook
system with a clearer before/after model tied to each endpoint.

A controller is a pair of optional Rust async functions (`before` and `after`)
declared on an endpoint via the `controller:` field.

## YAML Syntax
```yaml
endpoints:
  create:
    auth: [admin]
    input: [email, name, role, org_id]
    controller: { before: validate_org }          # before only
    events: [user.created]

  update:
    method: PATCH
    path: /users/:id
    auth: [admin, owner]
    input: [name, role]
    controller: { before: check_permissions, after: log_update }  # both

  delete:
    method: DELETE
    path: /users/:id
    auth: [admin]
    controller: { after: cleanup_related }        # after only
```

Note: For the five standard CRUD names (list, get, create, update, delete),
`method` and `path` are optional — they are inferred from the resource name.
The `create` endpoint above omits them; the parser fills in `POST /users`
automatically.

Valid shapes:
```yaml
controller: { before: fn_name }                              # scalar
controller: { after: fn_name }
controller: { before: fn_before, after: fn_after }
controller: { before: [validate_currencies, validate_org] }  # array
controller: { before: validate_x, after: [enrich_a, enrich_b] }
```

`before:` and `after:` each accept either a single hook name (scalar) or a
non-empty array of hook names. WASM hooks (entries starting with `wasm:`)
may be freely mixed with Rust hook names. An empty array (`before: []` or
`after: []`) is a validator error (SR063).

## Hook chains

When `before:` or `after:` is an array, hooks run in **declaration order**,
sequentially, on the same `Context`. The first `Err(_)` short-circuits the
chain — remaining hooks do not run, and (for `before:`) the DB write is
skipped.

```yaml
endpoints:
  create:
    controller:
      before:
        - validate_currencies   # runs first; returns Err -> chain aborts
        - validate_org          # runs only if validate_currencies returned Ok
        - "wasm:./plugins/normalize.wasm"  # runs only if both Rust hooks passed
```

Rationale: chains let an endpoint compose small, single-purpose validators
without each one having to call the next. The same `Context` flows through,
so a hook can stash state in `ctx.session` for the next link in the chain.

Internally, `ControllerSpec.before` and `ControllerSpec.after` parse to
`Option<HookList>` — an untagged enum (`String | Vec<String>`) defined in
`shaperail-core/src/endpoint.rs`. Code that walks the parsed AST iterates
via `controller.before.as_ref().map(HookList::names).unwrap_or(&[])`.

## File Location
Controller implementations live alongside resource YAML files:

```
resources/
  users.yaml
  users.controller.rs        # controller fns for users resource
  orders.yaml
  orders.controller.rs       # controller fns for orders resource
```

The file MUST be named `<resource>.controller.rs` where `<resource>` matches the
`resource:` value in the YAML (e.g. `users`).

## Function Signature (always this shape)
```rust
pub async fn fn_name(ctx: &mut ControllerContext) -> Result<(), ShaperailError> {
    // your logic here
    Ok(())
}
```

## ControllerContext — Everything Available on `ctx`
```rust
pub struct ControllerContext {
    // Input data (before: mutable, after: read-only)
    pub input: Value,              // JSON of the request body
    pub resource: Option<Value>,   // current DB record (for update/delete)
    pub output: Option<Value>,     // response data (after only)

    // Auth
    pub user: Option<AuthUser>,    // authenticated user (None if public endpoint)

    // Infrastructure — all pre-connected, just use them
    pub db: &PgPool,               // database connection pool
    pub cache: &RedisClient,       // Redis client
    pub jobs: &JobQueue,           // enqueue background jobs
    pub events: &EventEmitter,     // emit custom events
    pub storage: &StorageBackend,  // file storage

    // Request metadata
    pub request_id: String,
    pub headers: HeaderMap,
}
```

## Path params

`Context.path_params` is a `HashMap<String, String>` populated by the runtime
from URL `:name` segments before any controller runs. Use the typed helper
`ctx.path_param(name) -> Option<&str>` to read a value:

```rust
pub async fn check_owner(ctx: &mut Context) -> Result<(), ShaperailError> {
    let id = ctx.path_param("id")
        .ok_or_else(|| ShaperailError::Internal("missing :id".into()))?;
    let id: Uuid = id.parse().map_err(|_| ShaperailError::BadRequest("invalid id".into()))?;
    // ... fetch + verify ownership ...
    Ok(())
}
```

Population coverage:

| Endpoint | `path_params` |
|---|---|
| `list` (`GET /resource`) | `{}` |
| `create` (`POST /resource`) | `{}` |
| `update` (`PATCH /resource/:id`) | `{"id": "<...>"}` |
| `delete` (`DELETE /resource/:id`) | `{"id": "<...>"}` |
| custom endpoints with `:name` segments | one entry per `:name` |

`get` and `update_upload` do not currently dispatch before-hooks, so
`path_params` is not yet populated for those handlers — adding hook
dispatch to them is a separate change.

`ctx.input` is **not** auto-populated with `id` from the path. Authors who
want id-in-input write `ctx.input.insert("id", json!(ctx.path_param("id")))`
themselves; otherwise the explicit single-line read at the top of an update
hook is honest about where the value came from.

## Before / After Semantics

### Lifecycle: before → DB → after

```text
request ─▶ [before-hook(ctx)] ─▶ [DB op fills ctx.data] ─▶ [after-hook(ctx)] ─▶ response
                     │                                              │
                     └── ctx.session, ctx.response_extras shared ──┘
```

The `Context` is **the same struct instance** in both phases. Anything written to `ctx.session` in `before:` is visible in `after:`. `ctx.response_extras` is merged into the response's `data:` envelope after `after:` returns and before serialization — never persisted, never re-readable, perfect for one-time secrets like a freshly-minted token whose plaintext form must reach the client exactly once.

### Before controller
- Runs **before** the DB write.
- `ctx.input` is mutable — you can validate, transform, or reject the request.
- `ctx.data` is `None` (the write has not happened yet).
- May write to `ctx.session` for state the after-hook will need.
- May write to `ctx.response_extras` for fields that should appear in the response but never persist.
- Returning `Err` aborts the DB write and returns the error to the client.

### After controller
- Runs **after** the DB write.
- `ctx.data` contains the persisted record.
- `ctx.input` and `ctx.session` carry whatever the before-hook left there.
- `ctx.response_extras` keys (from either phase) are merged into the response's `data:` envelope.
- Returning `Err` is logged but the response is still 200 (configurable).

## Execution Order
1. `before` controller function runs (if declared)
2. DB write executes
3. `after` controller function runs (if declared) — receives the same `ctx`
4. `ctx.response_extras` are merged into `ctx.data`
5. Response returned to client

## Common Patterns

### Validate input (before)
```rust
pub async fn validate_org(ctx: &mut ControllerContext) -> Result<(), ShaperailError> {
    let org_id: Uuid = serde_json::from_value(ctx.input["org_id"].clone())?;
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;

    if user.org_id != org_id {
        return Err(ShaperailError::Forbidden("Cannot create user in another org".into()));
    }
    Ok(())
}
```

### Mutate input (before)
```rust
pub async fn hash_password(ctx: &mut ControllerContext) -> Result<(), ShaperailError> {
    if let Some(password) = ctx.input.get("password").and_then(|v| v.as_str()) {
        let hash = bcrypt::hash(password, 12)?;
        ctx.input["password_hash"] = Value::String(hash);
        ctx.input.as_object_mut().unwrap().remove("password");
    }
    Ok(())
}
```

### Enqueue a job (after)
```rust
pub async fn send_welcome_email(ctx: &mut ControllerContext) -> Result<(), ShaperailError> {
    let user_id = ctx.output.as_ref()
        .and_then(|o| o["id"].as_str())
        .ok_or(ShaperailError::Internal("Missing user id in output".into()))?;

    ctx.jobs.enqueue("send_welcome_email", json!({ "user_id": user_id })).await?;
    Ok(())
}
```

### Emit a custom event (after)
```rust
pub async fn emit_user_created(ctx: &mut ControllerContext) -> Result<(), ShaperailError> {
    ctx.events.emit("user.onboarded", ctx.output.clone().unwrap_or_default()).await?;
    Ok(())
}
```

## Enterprise Patterns (v0.7.0+)

See `docs/controllers.md` for full enterprise-grade patterns including:
- Multi-step approval workflows (state machine with role-based transitions)
- Cross-resource validation with DB queries
- Comprehensive audit trails (before/after snapshots, IP, user, timestamp)
- External service integration with idempotency keys (Stripe, etc.)
- Row-level security beyond tenant_key (department-level, hierarchical)
- Data masking based on role (SSN masking, salary hiding)
- Custom per-operation rate limiting
- Composing multiple validation steps in a single controller

## Generated Controller Traits (v0.7.0+)

`shaperail generate` now produces typed controller traits for resources that
declare controllers. The trait defines the exact function signatures, and the
compiler enforces them — LLMs cannot guess wrong signatures.

## What NOT to Do in Controllers
- Do NOT make direct HTTP calls without timeouts (use the job queue for slow calls)
- Do NOT catch and swallow errors silently
- Do NOT spawn new Tokio tasks (use `ctx.jobs` for background work)
- Do NOT read `ctx.output` in a `before` function (it does not exist yet)
- Do NOT write to side tables without considering rollback if the main write fails
