# Custom Handlers

A custom endpoint declares `handler:` (and optionally `method:` / `path:`) instead of using one of the conventional CRUD actions (`list` / `get` / `create` / `update` / `delete` / `bulk_create` / `bulk_delete`). Custom handlers own their own request parsing AND response generation — the framework gives you the route binding and authentication, but the rest is your code.

## What custom handlers do NOT get for free

Unlike CRUD endpoints, custom handlers do **not** inherit:

- Automatic input validation against the resource's `schema:` and the endpoint's `input:`.
- Automatic tenant isolation (`WHERE tenant_key = $tenant`).
- Automatic event emission (`events:`).
- The full before/after controller pipeline. Specifically:
  - `controller: { before: <name> }` IS supported on custom endpoints (see the section below).
  - `controller: { after: <name> }` is a validation error on custom endpoints — custom handlers own their response shape, so there is no place to merge `ctx.response_extras` after they return.

You write that logic explicitly inside the handler.

## Authenticating and tenant-scoping queries

Use `Subject` from `shaperail_runtime::auth`:

```rust
use actix_web::{HttpRequest, HttpResponse};
use shaperail_runtime::auth::Subject;
use sqlx::{Postgres, QueryBuilder};

pub async fn regenerate_secret(
    req: HttpRequest,
    state: actix_web::web::Data<std::sync::Arc<shaperail_runtime::handlers::crud::AppState>>,
    path: actix_web::web::Path<uuid::Uuid>,
) -> HttpResponse {
    // 1. Authenticate.
    let subject = match Subject::from_request(&req) {
        Ok(s) => s,
        Err(_) => return HttpResponse::Unauthorized().finish(),
    };

    // 2. Build a tenant-scoped UPDATE.
    let agent_id = path.into_inner();
    let new_hash: &str = ""; // computed earlier
    let mut q = QueryBuilder::<Postgres>::new("UPDATE agents SET mcp_secret_hash = ");
    q.push_bind(new_hash);
    q.push(" WHERE id = ");
    q.push_bind(agent_id);
    if subject.scope_to_tenant(&mut q, "org_id").is_err() {
        return HttpResponse::Unauthorized().finish();
    }

    // 3. Execute and respond.
    match q.build().execute(&state.pool).await {
        Ok(res) if res.rows_affected() == 1 => HttpResponse::Ok().finish(),
        Ok(_) => HttpResponse::NotFound().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}
```

`scope_to_tenant`:

- Is a **no-op** for `super_admin` (no filter applied — full visibility).
- Appends `" AND <column> = $N"` with the bound `tenant_id` for any other role.
- Returns `Err(Unauthorized)` for a non-`super_admin` subject whose JWT carries no `tenant_id` claim. That case is a config error and must fail loudly — never silently scope to "no filter."

For post-fetch checks (read-then-validate flows), use `assert_tenant_match(record_tenant_id)` instead.

## Auto-populating tenant context via `controller: { before: ... }`

If you declare a `before:` controller on a custom endpoint, the runtime
runs the same hook pipeline as CRUD endpoints get and stashes the resulting
`Context` into the request's actix extensions. Your handler can read
`tenant_id`, `user`, `session`, and `response_extras` from there:

```yaml
# resources/agents.yaml
endpoints:
  regenerate_secret:
    method: POST
    path: /agents/:id/regenerate_secret
    auth: [super_admin, admin]
    controller: { before: prepare_secret_rotation }
    handler: regenerate_secret
```

```rust
// resources/agents.controller.rs — runs before the handler
pub async fn prepare_secret_rotation(ctx: &mut Context) -> ControllerResult {
    // ctx.tenant_id is already populated; use it for cross-handler logic.
    // Stash anything you want the handler to see in ctx.session.
    ctx.session.insert("rotation_started_at".into(), json!(chrono::Utc::now()));
    Ok(())
}

// resources/agents.handlers.rs — runs after the before-controller
pub async fn regenerate_secret(
    req: HttpRequest,
    state: Arc<AppState>,
    _resource: Arc<ResourceDefinition>,
    _endpoint: Arc<EndpointSpec>,
) -> HttpResponse {
    use shaperail_runtime::handlers::controller::Context;
    let ctx = req.extensions().get::<Context>().cloned();
    let tenant = ctx.as_ref().and_then(|c| c.tenant_id.as_deref());
    // ... use tenant for SQL scoping ...
}
```

`after:` controllers are NOT supported on custom endpoints because the
custom handler owns the response shape — there's no `data:` envelope for
the runtime to merge `response_extras` into. If your logic needs an
after-pass, factor it into a helper called from the handler.

## Sharing logic across custom handlers

For endpoints without a before-controller, share logic the normal Rust way: extract a helper function in `resources/<name>.handlers.rs` and call it from each handler. The framework's job is to give you `Subject` and the runtime's `AppState`; your job is to use them.

## What if I want CRUD-style hooks?

Use a CRUD endpoint and the `controller: { before, after }` declaration. The two-phase pipeline plus `Context.session` and `Context.response_extras` cover most "I need to mint a one-time value" cases without a custom handler — see `agent_docs/hooks-system.md`.
