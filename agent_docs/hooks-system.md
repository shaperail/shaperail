# SteelAPI Hook System

## What Hooks Are
Hooks are escape hatches for custom business logic that can't be expressed declaratively.
They are Rust async functions with a fixed signature, injected at declared lifecycle points.

## Hook Signature (always this shape)
```rust
pub async fn hook_name(ctx: &mut HookContext) -> Result<(), SteelError> {
    // your logic here
    Ok(())
}
```

## HookContext — Everything Available on `ctx`
```rust
pub struct HookContext {
    // Input data (before-hooks: mutable, after-hooks: read-only)
    pub input: Value,              // JSON of the request body
    pub resource: Option<Value>,   // current DB record (for update/delete)
    pub output: Option<Value>,     // response data (after-hooks only)

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

## Using ctx — Common Patterns

### Validate input
```rust
pub async fn validate_org_membership(ctx: &mut HookContext) -> Result<(), SteelError> {
    let org_id: Uuid = serde_json::from_value(ctx.input["org_id"].clone())?;
    let user = ctx.user.as_ref().ok_or(SteelError::Unauthorized)?;

    if user.org_id != org_id {
        return Err(SteelError::Forbidden("Cannot create user in another org".into()));
    }
    Ok(())
}
```

### Mutate input (before-hooks only)
```rust
pub async fn hash_password(ctx: &mut HookContext) -> Result<(), SteelError> {
    if let Some(password) = ctx.input.get("password").and_then(|v| v.as_str()) {
        let hash = bcrypt::hash(password, 12)?;
        ctx.input["password_hash"] = Value::String(hash);
        ctx.input.as_object_mut().unwrap().remove("password");
    }
    Ok(())
}
```

### Enqueue a job (after-hooks)
```rust
pub async fn send_welcome_email(ctx: &mut HookContext) -> Result<(), SteelError> {
    let user_id = ctx.output.as_ref()
        .and_then(|o| o["id"].as_str())
        .ok_or(SteelError::Internal("Missing user id in output".into()))?;

    ctx.jobs.enqueue("send_welcome_email", json!({ "user_id": user_id })).await?;
    Ok(())
}
```

### Emit a custom event
```rust
pub async fn emit_user_created_event(ctx: &mut HookContext) -> Result<(), SteelError> {
    ctx.events.emit("user.onboarded", ctx.output.clone().unwrap_or_default()).await?;
    Ok(())
}
```

## Hook Execution Order
1. `before` hooks run in declaration order, before DB write
2. DB write executes
3. `after` hooks run in declaration order, after DB write
4. Response returned to client

If any `before` hook returns `Err`, the DB write is aborted and the error is returned.
If any `after` hook returns `Err`, the error is logged but the response is still 200 (configurable).

## Hook File Location
Custom hook implementations live in `steel-runtime/src/hooks/<resource>/`.
They are referenced by name in the resource YAML file.
The codegen generates the hook registration; you write the implementation.

## What NOT to Do in Hooks
- Do NOT make direct HTTP calls (use the job queue instead — hooks must not block)
- Do NOT catch and swallow errors silently
- Do NOT spawn new Tokio tasks (use ctx.jobs for background work)
- Do NOT mutate ctx.output in before-hooks (it doesn't exist yet)
