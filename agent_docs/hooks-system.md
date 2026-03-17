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
controller: { before: fn_name }
controller: { after: fn_name }
controller: { before: fn_before, after: fn_after }
```

Only one `before` and one `after` function per endpoint. If you need to compose
multiple steps, call them from within your single controller function.

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

## Before / After Semantics

### Before controller
- Runs **before** the DB write.
- `ctx.input` is mutable — you can validate, transform, or reject the request.
- `ctx.output` is `None` (the write has not happened yet).
- Returning `Err` aborts the DB write and returns the error to the client.

### After controller
- Runs **after** the DB write.
- `ctx.output` contains the response data.
- `ctx.input` is read-only at this point.
- Returning `Err` is logged but the response is still 200 (configurable).

## Execution Order
1. `before` controller function runs (if declared)
2. DB write executes
3. `after` controller function runs (if declared)
4. Response returned to client

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

## What NOT to Do in Controllers
- Do NOT make direct HTTP calls (use the job queue instead — controllers must not block)
- Do NOT catch and swallow errors silently
- Do NOT spawn new Tokio tasks (use `ctx.jobs` for background work)
- Do NOT read `ctx.output` in a `before` function (it does not exist yet)
