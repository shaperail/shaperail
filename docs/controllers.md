---
title: Controllers
parent: Guides
nav_order: 3
---

# Controllers

Controllers run bounded business logic before or after generated CRUD inside
the same HTTP request. Use them to validate or normalize input, enforce
application rules, populate server-owned fields, and enrich responses.

Use [background jobs]({{ '/background-jobs/' | relative_url }}) for work that
should not delay the response.

## Declare a controller

```yaml
resource: users
version: 1

schema:
  id:         { type: uuid, primary: true, generated: true }
  email:      { type: string, format: email, required: true, unique: true }
  name:       { type: string, min: 1, max: 200, required: true }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }

endpoints:
  create:
    auth: [admin]
    input: [email, name]
    controller:
      before: normalize_email
      after: add_profile_url
    events: [user.created]
    jobs: [send_welcome_email]
```

Controller source is co-located with the resource:

```text
resources/
├── users.yaml
└── users.controller.rs
```

Run `shaperail generate` after declaring a controller. It creates a missing
controller stub and adds registration code to `generated/mod.rs`; it never
overwrites an existing controller file.

## Write a controller

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
`Err(...)` stops the request. Function names must match the YAML declaration.

## Controller chains

Each phase accepts one function or a non-empty ordered list:

```yaml
controller:
  before: [validate_org, normalize_email]
  after: [add_profile_url, add_cache_header]
```

Functions run sequentially on the same `Context`. The first error stops the
chain. Native Rust functions and `wasm:./path/to/plugin.wasm` entries may appear
in the same list when the `wasm-plugins` feature is enabled.

## Lifecycle

For a write endpoint, Shaperail:

1. Filters the request body using the endpoint's `input` list.
2. Validates all supplied values.
3. Runs the before-controller.
4. Validates required fields and values injected by the controller.
5. Persists the record and assigns it to `ctx.data`.
6. Runs the after-controller.
7. Enqueues declared events and jobs after a successful write.

The same `Context` survives both phases. Values written to `ctx.session` in
`before` remain available in `after`.

## Context reference

| Field | Available | Description |
| --- | --- | --- |
| `input` | before + after | Mutable write input. In `after`, this is still the submitted/mutated input, not the full row. |
| `data` | after | Persisted record as `Option<serde_json::Value>`. It is `None` before persistence. |
| `user` | before + after | Optional authenticated subject with `sub`, `role`, and `tenant_id`. |
| `pool` | before + after | PostgreSQL pool for application-specific queries. |
| `headers` | before + after | Read-only request headers. |
| `client_ip()` | before + after | Canonical client IP. Raw forwarding headers must not be trusted directly. |
| `response_headers` | before + after | Response headers to append. |
| `tenant_id` | before + after | Tenant claim resolved by Shaperail when applicable. |
| `session` | before + after | Request-local scratch values, never persisted or returned. |
| `response_extras` | before + after | Values merged into the response's `data` object, never persisted. |
| `path_params` | before + after | URL path parameters. Prefer `ctx.path_param("id")`. |

There are no `ctx.output`, `ctx.jobs`, or `ctx.events` fields. Declare jobs and
events in resource YAML.

## Validate input

Return `ShaperailError::Validation` for client-correctable field failures:

```rust
use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

pub async fn validate_org(ctx: &mut Context) -> ControllerResult {
    let org_id = ctx
        .input
        .get("org_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ShaperailError::Validation(vec![FieldError {
                field: "org_id".into(),
                message: "is required".into(),
                code: "required".into(),
            }])
        })?;

    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM organizations WHERE id = $1::uuid)",
    )
    .bind(org_id)
    .fetch_one(&ctx.pool)
    .await?;

    if !exists {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "org_id".into(),
            message: "organization does not exist".into(),
            code: "invalid_reference".into(),
        }]));
    }

    Ok(())
}
```

Use:

- `Unauthorized` for missing or invalid credentials;
- `Forbidden` when an authenticated subject lacks permission;
- `Conflict(String)` for application conflicts;
- `Internal(String)` for server failures that clients cannot correct.

## Populate server-owned fields

Do not expose fields such as `created_by` in endpoint `input`. Declare them
required in the schema and let a before-controller populate them:

```yaml
schema:
  created_by: { type: string, required: true }

endpoints:
  create:
    auth: [member, admin]
    input: [title, body]
    controller: { before: set_created_by }
```

```rust
use shaperail_core::ShaperailError;

pub async fn set_created_by(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
    ctx.input
        .insert("created_by".into(), serde_json::json!(&user.sub));
    Ok(())
}
```

`AuthenticatedUser.sub` is the opaque JWT `sub` claim. A string field intended
to store the external subject can preserve it directly.

### Resolve a database user before writing a foreign key

Do not assume `sub` is a `users.id`. Platform identities such as
`super_admin` may have no user row. If the target field is a foreign key,
resolve the application's mapping first:

```rust
async fn resolve_user_id(
    ctx: &Context,
    subject: &str,
) -> Result<uuid::Uuid, ShaperailError> {
    sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM users WHERE external_subject = $1",
    )
    .bind(subject)
    .fetch_optional(&ctx.pool)
    .await?
    .ok_or(ShaperailError::Forbidden)
}

pub async fn set_owner_id(ctx: &mut Context) -> ControllerResult {
    let subject = ctx
        .user
        .as_ref()
        .ok_or(ShaperailError::Unauthorized)?
        .sub
        .clone();
    let user_id = resolve_user_id(ctx, &subject).await?;
    ctx.input
        .insert("owner_id".into(), serde_json::json!(user_id));
    Ok(())
}
```

## Read path parameters

Update and delete input does not contain the URL ID. Read it from the context:

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

## Enrich a response

After-controllers receive the persisted row in `ctx.data`:

```rust
pub async fn add_profile_url(ctx: &mut Context) -> ControllerResult {
    let Some(data) = ctx.data.as_ref() else {
        return Err(ShaperailError::Internal(
            "after-controller ran without persisted data".into(),
        ));
    };
    let id = data
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ShaperailError::Internal("response is missing id".into()))?;

    ctx.response_extras.insert(
        "profile_url".into(),
        serde_json::json!(format!("/profiles/{id}")),
    );
    ctx.response_headers
        .push(("Cache-Control".into(), "private, max-age=60".into()));
    Ok(())
}
```

`response_extras` is merged into the response's `data` object. It is not
written to the database. An extra shadows a persisted field with the same name.

## Pass data between phases

Use `ctx.session` for a value that must survive from `before` to `after` but
must never be persisted:

```rust
pub async fn mint_api_token(ctx: &mut Context) -> ControllerResult {
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

## Custom endpoints

An endpoint with `handler:` supports a before-controller. Shaperail builds and
runs the `Context`, then stores it in request extensions for the handler:

```rust
use actix_web::HttpMessage;

let ctx = request
    .extensions()
    .get::<Context>()
    .cloned()
    .ok_or_else(|| ShaperailError::Internal("controller context missing".into()))?;
```

Custom endpoints reject `controller.after`: the custom handler owns its
response, so Shaperail has no generated response stage in which to merge
after-controller changes. Call a shared helper from the custom handler instead.
See [Custom handlers]({{ '/custom-handlers/' | relative_url }}) for request-body
extraction and registration.

## What not to do

- Do not trust client input for server-owned identity, tenant, audit, or status
  fields.
- Do not bind `user.sub` to a foreign key without resolving a verified row.
- Do not read `ctx.data` in a before-controller.
- Do not use `ctx.input["id"]` for URL IDs; use `ctx.path_param("id")`.
- Do not spawn detached Tokio tasks. Declare endpoint `jobs`.
- Do not emit events from controller-local infrastructure. Declare endpoint
  `events`.
- Do not swallow database or network errors.
- Do not put one-time secrets into persisted data merely to return them.

## Test controllers

Unit-test controller mutations and errors by constructing a `Context`; cover
critical end-to-end behavior with Actix integration tests. The complete context
factory and examples are in [Testing]({{ '/testing/' | relative_url }}).

## Migrating from `hooks:`

The old resource-level `hooks:` syntax is invalid:

```yaml
# old
hooks: [validate_org]

# current
controller: { before: validate_org }
```

Move the function into `resources/<resource>.controller.rs`, use the
`Context`/`ControllerResult` signature, and run `shaperail generate`.
