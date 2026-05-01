# Custom Handlers

A custom endpoint declares `handler:` (and optionally `method:` / `path:`) instead of using one of the conventional CRUD actions (`list` / `get` / `create` / `update` / `delete` / `bulk_create` / `bulk_delete`). Custom handlers own their own request parsing AND response generation — the framework gives you the route binding and authentication, but the rest is your code.

## What custom handlers do NOT get for free

Unlike CRUD endpoints, custom handlers do **not** inherit:

- Automatic input validation against the resource's `schema:` and the endpoint's `input:`.
- Automatic tenant isolation (`WHERE tenant_key = $tenant`).
- Automatic event emission (`events:`).
- The before/after controller pipeline (`controller: { before, after }`). Declaring `controller:` on a custom endpoint is a validation error in v0.11+.

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

## Sharing logic across custom handlers

There is no controller pipeline for custom endpoints — that's why declaring `controller:` on a custom endpoint is rejected. Share logic the normal Rust way: extract a helper function in `resources/<name>.handlers.rs` and call it from each handler. The framework's job is to give you `Subject` and the runtime's `AppState`; your job is to use them.

## What if I want CRUD-style hooks?

Use a CRUD endpoint and the `controller: { before, after }` declaration. The two-phase pipeline plus `Context.session` and `Context.response_extras` cover most "I need to mint a one-time value" cases without a custom handler — see `agent_docs/hooks-system.md`.
