# Shaperail Controller System

Controllers add synchronous, in-request business logic around generated CRUD.
The historical `hooks:` resource key is removed; use `controller.before` and
`controller.after`.

## Declaration

```yaml
endpoints:
  create:
    auth: [admin]
    input: [email, name]
    controller:
      before: normalize_email
      after: add_profile_url
```

Each phase accepts one function name or a non-empty ordered list:

```yaml
controller:
  before: [validate_org, normalize_email]
  after: [audit_create, enrich_response]
```

Controller source lives in `resources/<resource>.controller.rs`. Running
`shaperail generate` creates a missing stub and registers declared functions,
but never overwrites an existing file.

## Function signature

```rust
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

pub async fn normalize_email(ctx: &mut Context) -> ControllerResult {
    if let Some(email) = ctx.input.get_mut("email") {
        if let Some(value) = email.as_str() {
            *email = serde_json::json!(value.trim().to_lowercase());
        }
    }
    Ok(())
}
```

`ControllerResult` is `Result<(), shaperail_core::ShaperailError>`. Returning
an error stops the request.

## Lifecycle

One `Context` survives both phases:

1. The runtime filters request input using the endpoint's `input` list.
2. It validates every supplied value.
3. The before-controller may validate or mutate `ctx.input`.
4. The runtime validates required fields and controller-injected values.
5. It persists the record and sets `ctx.data`.
6. The after-controller may enrich the response.
7. Declared events and jobs run after a successful write.

Anything placed in `ctx.session` during `before` remains available during
`after` for that request.

## Context fields

| Field | Availability | Purpose |
| --- | --- | --- |
| `input` | before + after | Mutable write input. In `after`, it reflects the submitted/mutated input, not the full persisted record. |
| `data` | after | Persisted record as `Option<serde_json::Value>`. It is `None` before persistence. |
| `user` | before + after | `Option<AuthenticatedUser>` with `sub`, `role`, and `tenant_id`. |
| `pool` | before + after | PostgreSQL pool for application-specific queries. |
| `headers` | before + after | Read-only request headers normalized into a map. |
| `client_ip()` | before + after | Canonical client IP from the trusted-proxy resolver. Never read forwarding headers directly. |
| `response_headers` | before + after | Headers to append to the HTTP response. |
| `tenant_id` | before + after | Tenant claim resolved by the runtime when applicable. |
| `session` | before + after | Request-local cross-phase scratch data; never persisted or returned. |
| `response_extras` | before + after | Fields merged into the response's `data` object; never persisted. |
| `path_params` | before + after | URL parameters. Prefer `ctx.path_param("id")`. |

There are no `ctx.output`, `ctx.jobs`, or `ctx.events` fields. Declare jobs and
events in resource YAML.

## Validation

Use field-level errors for client-correctable input:

```rust
use shaperail_core::{FieldError, ShaperailError};

pub async fn validate_name(ctx: &mut Context) -> ControllerResult {
    let valid = ctx
        .input
        .get("name")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|name| !name.trim().is_empty());

    if !valid {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "name".into(),
            message: "must not be blank".into(),
            code: "required".into(),
        }]));
    }

    Ok(())
}
```

Use `Unauthorized` when credentials are missing/invalid and `Forbidden` when an
authenticated caller lacks permission. Use `Conflict(String)` for business
conflicts and `Internal(String)` only for server failures.

## Authenticated subjects

`AuthenticatedUser.sub` is the opaque JWT `sub` claim. It is not guaranteed to
be a `users.id`, especially for platform identities such as `super_admin`.

- It is safe to store `sub` in a string field intended to preserve the external
  subject.
- Before writing it to a database foreign key, resolve and verify the
  application user row.
- Keep server-owned subject fields out of endpoint `input`; inject them in a
  before-controller.

```rust
pub async fn set_created_by(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
    ctx.input
        .insert("created_by".into(), serde_json::json!(&user.sub));
    Ok(())
}
```

## Path parameters

```rust
pub async fn prevent_locked_delete(ctx: &mut Context) -> ControllerResult {
    let id = ctx
        .path_param("id")
        .ok_or_else(|| ShaperailError::Internal("missing path parameter: id".into()))?;

    let locked = sqlx::query_scalar::<_, bool>(
        "SELECT locked FROM documents WHERE id = $1::uuid",
    )
    .bind(id)
    .fetch_one(&ctx.pool)
    .await?;

    if locked {
        return Err(ShaperailError::Conflict("document is locked".into()));
    }
    Ok(())
}
```

## Cross-phase response enrichment

```rust
pub async fn mint_token(ctx: &mut Context) -> ControllerResult {
    if ctx.data.is_none() {
        let token = create_one_time_token();
        ctx.input
            .insert("token_hash".into(), serde_json::json!(hash_token(&token)));
        ctx.session.insert("plaintext_token".into(), token.into());
    } else if let Some(token) = ctx.session.remove("plaintext_token") {
        ctx.response_extras.insert("token".into(), token);
    }
    Ok(())
}
```

`response_extras` keys shadow same-named persisted fields in the response. Do
not put secrets in `ctx.data` or persisted columns merely to return them once.

## Boundaries

- Controllers run inside the HTTP request; keep them bounded and deterministic.
- Do not spawn detached Tokio tasks. Declare background work with endpoint
  `jobs`.
- Do not emit events manually from controllers. Declare endpoint `events`.
- Do not return secrets through error messages or logs.
- Do not create a service layer between resources and runtime.
- Test controller mutations and error paths directly, then cover critical flows
  with endpoint integration tests.
