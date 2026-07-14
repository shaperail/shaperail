---
title: Error handling
parent: Guides
nav_order: 5
---

# Error handling

Shaperail uses a single error type -- `ShaperailError` -- across all crates.
Every error maps to exactly one HTTP status code, one machine-readable code, and
one JSON response shape. There are no alternative error types, no custom
response builders, and no way to accidentally return a bare string as an error.

---

## ShaperailError variants

All errors in Shaperail are variants of `shaperail_core::ShaperailError`:

| Variant | HTTP Status | Code | When to use |
| --- | --- | --- | --- |
| `NotFound` | 404 | `NOT_FOUND` | Record does not exist, or `sqlx::Error::RowNotFound` |
| `Unauthorized` | 401 | `UNAUTHORIZED` | Missing or invalid JWT / API key |
| `Forbidden` | 403 | `FORBIDDEN` | Authenticated but insufficient permissions |
| `Validation(Vec<FieldError>)` | 422 | `VALIDATION_ERROR` | One or more fields failed validation |
| `Conflict(String)` | 409 | `CONFLICT` | Unique constraint violation or state conflict |
| `RateLimited` | 429 | `RATE_LIMITED` | Rate limit exceeded |
| `Internal(String)` | 500 | `INTERNAL_ERROR` | Unexpected server error |

Each variant implements `Display` and `actix_web::ResponseError`, so you can
return any `ShaperailError` directly from a handler or controller with `?` or
`return Err(...)`.

---

## Error response format

Every error response uses the same JSON envelope:

```json
{
  "error": {
    "code": "NOT_FOUND",
    "status": 404,
    "message": "Resource not found",
    "request_id": "req-abc-123",
    "details": null
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| `code` | string | Machine-readable error code (e.g., `NOT_FOUND`, `VALIDATION_ERROR`) |
| `status` | integer | HTTP status code |
| `message` | string | Human-readable description |
| `request_id` | string | Request ID from middleware (for log correlation) |
| `details` | array or null | Per-field errors for `VALIDATION_ERROR`, `null` for all others |

### Example responses

**401 Unauthorized:**

```json
{
  "error": {
    "code": "UNAUTHORIZED",
    "status": 401,
    "message": "Unauthorized",
    "request_id": "req-7f3a2b",
    "details": null
  }
}
```

**409 Conflict:**

```json
{
  "error": {
    "code": "CONFLICT",
    "status": 409,
    "message": "Conflict: duplicate key value violates unique constraint \"users_email_key\"",
    "request_id": "req-e92c41",
    "details": null
  }
}
```

**429 Rate Limited:**

```json
{
  "error": {
    "code": "RATE_LIMITED",
    "status": 429,
    "message": "Rate limit exceeded",
    "request_id": "req-d18f55",
    "details": null
  }
}
```

---

## Validation errors and FieldError

When a request fails validation, the response includes a `details` array with
one entry per invalid field. Each entry is a `FieldError`:

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "status": 422,
    "message": "Validation failed",
    "request_id": "req-def-456",
    "details": [
      { "field": "email", "message": "is required", "code": "required" },
      { "field": "name", "message": "too short", "code": "too_short" }
    ]
  }
}
```

Each `FieldError` has three fields:

| Key | Type | Description |
| --- | --- | --- |
| `field` | string | The schema field that failed |
| `message` | string | Human-readable description |
| `code` | string | Machine-readable code (e.g., `required`, `too_short`, `invalid_format`) |

### Built-in validation codes

These codes are produced automatically by Shaperail's generated validators:

| Code | Meaning |
| --- | --- |
| `required` | A required field is missing |
| `too_short` | String length is below `min` |
| `too_long` | String length exceeds `max` |
| `invalid_format` | String does not match declared `format` (e.g., email) |
| `invalid_enum` | Value is not in the declared `values` list |
| `invalid_type` | Value type does not match the declared field type |
| `invalid_reference` | Foreign key points to a nonexistent record (controller-level) |

### Constructing validation errors in code

```rust
use shaperail_core::{FieldError, ShaperailError};

// Single field error
let err = ShaperailError::Validation(vec![
    FieldError {
        field: "email".into(),
        message: "is required".into(),
        code: "required".into(),
    },
]);

// Multiple field errors
let err = ShaperailError::Validation(vec![
    FieldError {
        field: "email".into(),
        message: "already taken".into(),
        code: "uniqueness".into(),
    },
    FieldError {
        field: "name".into(),
        message: "must be between 1 and 200 characters".into(),
        code: "too_short".into(),
    },
]);
```

---

## Returning errors from controller functions

Controllers return `ControllerResult`, which is `Result<(), ShaperailError>`.
Return `Err(...)` with any `ShaperailError` variant to halt the request.

### Before-controller: reject invalid input

```rust
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use shaperail_core::{FieldError, ShaperailError};

pub async fn validate_org(ctx: &mut Context) -> ControllerResult {
    let org_id = ctx.input.get("org_id").and_then(|v| v.as_str());

    let Some(org_id) = org_id else {
        return Err(ShaperailError::Validation(vec![
            FieldError {
                field: "org_id".into(),
                message: "is required".into(),
                code: "required".into(),
            },
        ]));
    };

    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM organizations WHERE id = $1)"
    )
    .bind(org_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap_or(false);

    if !exists {
        return Err(ShaperailError::Validation(vec![
            FieldError {
                field: "org_id".into(),
                message: "organization does not exist".into(),
                code: "invalid_reference".into(),
            },
        ]));
    }

    Ok(())
}
```

When a before-controller returns `Err`, the database operation is skipped
entirely and the error response is sent immediately.

### Before-controller: authorization check

```rust
pub async fn owner_only(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;

    if user.role != "admin" {
        // This field stores the opaque auth subject, not a users.id foreign key.
        let owner_subject = ctx.input
            .get("owner_subject")
            .and_then(|v| v.as_str());
        if owner_subject != Some(user.sub.as_str()) {
            return Err(ShaperailError::Forbidden);
        }
    }

    Ok(())
}
```

### After-controller: conditional error

```rust
pub async fn verify_result(ctx: &mut Context) -> ControllerResult {
    if let Some(data) = &ctx.data {
        let status = data["status"].as_str().unwrap_or("");
        if status == "suspended" {
            return Err(ShaperailError::Forbidden);
        }
    }
    Ok(())
}
```

### Using the ? operator with sqlx

Because `ShaperailError` implements `From<sqlx::Error>`, you can use `?`
directly on sqlx calls inside controllers:

```rust
pub async fn check_quota(ctx: &mut Context) -> ControllerResult {
    let subject = ctx.user.as_ref()
        .map(|u| u.sub.as_str())
        .ok_or(ShaperailError::Unauthorized)?;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM orders WHERE created_by_subject = $1"
    )
    .bind(subject)
    .fetch_one(&ctx.pool)
    .await?;  // sqlx::Error automatically converts to ShaperailError

    if count >= 100 {
        return Err(ShaperailError::Validation(vec![
            FieldError {
                field: "created_by_subject".into(),
                message: "order quota exceeded (max 100)".into(),
                code: "quota_exceeded".into(),
            },
        ]));
    }

    Ok(())
}
```

These examples compare or store `sub` only in string fields whose explicit
meaning is the external authentication subject. If your schema uses a
`users.id` foreign key, resolve and verify the application user row first; do
not bind `sub` directly.

---

## Database error mapping

Shaperail automatically converts `sqlx::Error` into `ShaperailError` via a
`From` implementation. The mapping is:

| sqlx::Error | ShaperailError | Rationale |
| --- | --- | --- |
| `RowNotFound` | `NotFound` | Record does not exist |
| `Database` with PostgreSQL code `23505` | `Conflict(message)` | Unique constraint violation |
| All other variants | `Internal(message)` | Unexpected database error |

This means you never need to manually handle sqlx errors in most cases. The `?`
operator does the right thing:

```rust
// RowNotFound becomes ShaperailError::NotFound (HTTP 404)
let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
    .bind(user_id)
    .fetch_one(&pool)
    .await?;

// Unique violation becomes ShaperailError::Conflict (HTTP 409)
sqlx::query("INSERT INTO users (email, name) VALUES ($1, $2)")
    .bind(&input.email)
    .bind(&input.name)
    .execute(&pool)
    .await?;
```

### Handling specific database errors

If you need finer control over a database error, match on the sqlx error before
it converts:

```rust
pub async fn create_with_retry(ctx: &mut Context) -> ControllerResult {
    let email = ctx.input["email"].as_str().unwrap_or("");

    let result = sqlx::query("INSERT INTO users (email) VALUES ($1)")
        .bind(email)
        .execute(&ctx.pool)
        .await;

    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505") => {
            Err(ShaperailError::Validation(vec![
                FieldError {
                    field: "email".into(),
                    message: "email is already registered".into(),
                    code: "uniqueness".into(),
                },
            ]))
        }
        Err(e) => Err(ShaperailError::Internal(e.to_string())),
    }
}
```

This converts a unique violation into a user-friendly validation error instead
of a generic 409 Conflict.

---

## Error handling in background jobs

Background jobs run outside the HTTP request lifecycle, so errors do not produce
HTTP responses. Instead, they drive the retry and dead letter queue system.

### Job handler errors

A job handler returns `Result<(), ShaperailError>`. If it returns `Err`, the
job is marked as failed:

```rust
pub async fn send_welcome_email(payload: serde_json::Value) -> Result<(), ShaperailError> {
    let email = payload["email"]
        .as_str()
        .ok_or_else(|| ShaperailError::Internal("missing email in job payload".into()))?;

    let result = send_email(email, "Welcome!", "...").await;

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(ShaperailError::Internal(format!("email send failed: {e}"))),
    }
}
```

### Retry behavior

Failed jobs are retried with exponential backoff (`2^attempt` seconds). After
exhausting all retries (default: 3), the job moves to the dead letter queue.

| Attempt | Backoff |
| --- | --- |
| 1 | 2s |
| 2 | 4s |
| 3 | 8s |

### Designing retryable jobs

Write job handlers so that retries are safe:

```rust
pub async fn provision_account(payload: serde_json::Value) -> Result<(), ShaperailError> {
    let user_id = payload["id"].as_str()
        .ok_or_else(|| ShaperailError::Internal("missing user id".into()))?;

    // Idempotency check: skip if already provisioned
    let already_done = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM provisions WHERE user_id = $1)"
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap_or(false);

    if already_done {
        return Ok(());
    }

    // Do the work
    create_provisioning_record(user_id).await
        .map_err(|e| ShaperailError::Internal(format!("provisioning failed: {e}")))?;

    Ok(())
}
```

### Monitoring failed jobs

Use the CLI to check the dead letter queue:

```bash
shaperail jobs:status           # queue depths + recent failures
shaperail jobs:status <job_id>  # status of a specific job
```

---

## Diagnostic error codes (shaperail check)

The `shaperail check` command validates your resource YAML files and reports
structured diagnostics. Each diagnostic has a stable code, a human-readable
error message, a suggested fix, and a corrected YAML example.

Run it:

```bash
shaperail check
```

Output looks like:

```
[SR010] resource 'items': field 'status' is type enum but has no values
  fix: add 'values: [value1, value2]' to the 'status' field
  example: status: { type: enum, values: [option_a, option_b] }
```

### Full code reference

#### Resource-level (SR001--SR005)

| Code | Error | Fix |
| --- | --- | --- |
| SR001 | Resource name is empty | Add a snake_case plural name to `resource:` |
| SR002 | Version is 0 | Set `version: 1` or higher |
| SR003 | Schema has no fields | Add at least an `id` field |
| SR004 | Schema has no primary key | Add `primary: true` to one field |
| SR005 | Schema has multiple primary keys | Keep `primary: true` on exactly one field |

#### Field-level (SR010--SR016)

| Code | Error | Fix |
| --- | --- | --- |
| SR010 | Enum field has no `values` | Add `values: [a, b]` |
| SR011 | Non-enum field has `values` | Change type to `enum` or remove `values` |
| SR012 | `ref` field is not type `uuid` | Change the field type to `uuid` |
| SR013 | `ref` is not in `resource.field` format | Use `organizations.id` format |
| SR014 | Array field has no `items` | Add `items: <element_type>` |
| SR015 | `format` on non-string field | Change type to `string` or remove `format` |
| SR016 | Primary key is neither `generated` nor `required` | Add `generated: true` or `required: true` |

#### Tenant (SR020--SR021)

| Code | Error | Fix |
| --- | --- | --- |
| SR020 | `tenant_key` field is not type `uuid` | Change the field type to `uuid` |
| SR021 | `tenant_key` field not found in schema | Add the field to the schema |

#### Endpoint-level (SR030--SR054)

| Code | Error | Fix |
| --- | --- | --- |
| SR030 | Empty `controller.before` name | Provide a function name |
| SR031 | Empty `controller.after` name | Provide a function name |
| SR032 | Empty event name | Use `resource.action` format |
| SR033 | Empty job name | Provide a snake_case job name |
| SR035 | `wasm:` prefix with no path | Add a `.wasm` file path |
| SR036 | WASM path does not end with `.wasm` | Fix the file extension |
| SR040 | Input/filter/search/sort field not in schema | Add the field to the schema or remove it |
| SR041 | `soft_delete` without `updated_at` field | Add `updated_at: { type: timestamp, generated: true }` |
| SR050 | Upload on non-write method | Change method to POST, PATCH, or PUT |
| SR051 | Upload field is not type `file` | Change the field type to `file` |
| SR052 | Upload field not found in schema | Add the field to the schema |
| SR053 | Invalid upload storage provider | Use `local`, `s3`, `gcs`, or `azure` |
| SR054 | Upload field not in endpoint `input` | Add the field to the `input` array |

#### Relation-level (SR060--SR062)

| Code | Error | Fix |
| --- | --- | --- |
| SR060 | `belongs_to` relation has no `key` | Add `key: <local_fk_field>` |
| SR061 | `has_many`/`has_one` relation has no `foreign_key` | Add `foreign_key: <fk_on_related_table>` |
| SR062 | Relation `key` field not found in schema | Add the FK field to the schema |

#### Index-level (SR070--SR072)

| Code | Error | Fix |
| --- | --- | --- |
| SR070 | Index has no fields | Add at least one field |
| SR071 | Index references a field not in schema | Add the field or remove it from the index |
| SR072 | Invalid index order | Use `asc` or `desc` |

### Using diagnostics in CI

```bash
# Fail the build if any resource has errors
shaperail check --json | jq -e 'length == 0'
```

The `--json` flag outputs an array of diagnostic objects, each with
`code`, `error`, `fix`, and `example` fields. This is designed for AI tools and
CI pipelines that need machine-readable output.

---

## Best practices

### 1. Fail loudly

Never swallow errors silently. If something goes wrong, return a `ShaperailError`
so the framework can log it, set the correct status code, and include the
request ID for debugging.

```rust
// Bad: silently returns a default
let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM orders")
    .fetch_one(&pool)
    .await
    .unwrap_or(0);

// Good: propagates the error
let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM orders")
    .fetch_one(&pool)
    .await?;
```

### 2. Log context with tracing

Use the `tracing` crate to add structured context before returning errors. The
request ID is already attached to the span by Shaperail's middleware.

```rust
use tracing::warn;

pub async fn validate_org(ctx: &mut Context) -> ControllerResult {
    let org_id = ctx.input.get("org_id").and_then(|v| v.as_str());

    let Some(org_id) = org_id else {
        warn!("validate_org called without org_id in input");
        return Err(ShaperailError::Validation(vec![
            FieldError {
                field: "org_id".into(),
                message: "is required".into(),
                code: "required".into(),
            },
        ]));
    };

    // ...
    Ok(())
}
```

### 3. Never expose internals

The `Internal` variant includes a message for logging, but the framework does
not expose raw database errors, stack traces, or file paths to the client. The
client sees:

```json
{
  "error": {
    "code": "INTERNAL_ERROR",
    "status": 500,
    "message": "Internal server error: <message>",
    "request_id": "req-abc-123",
    "details": null
  }
}
```

When constructing `Internal` errors, write messages that help you debug without
leaking schema names, SQL queries, or credentials:

```rust
// Bad: leaks table structure
Err(ShaperailError::Internal(
    format!("INSERT INTO users (email, password_hash) failed: {e}")
))

// Good: describes the failure without internals
Err(ShaperailError::Internal(
    format!("failed to create user record: {e}")
))
```

### 4. Use the right variant

Do not use `Internal` for expected failures. Each variant exists for a reason:

```rust
// Bad: using Internal for a known case
if !authorized {
    return Err(ShaperailError::Internal("not authorized".into()));
}

// Good: using the correct variant
if !authorized {
    return Err(ShaperailError::Forbidden);
}
```

### 5. Use Validation for user-correctable problems

If the user can fix the problem by changing their input, return `Validation`
with specific field errors. This gives API clients enough information to
highlight the exact fields that need correction.

```rust
// Bad: generic error
return Err(ShaperailError::Conflict("bad input".into()));

// Good: actionable field-level feedback
return Err(ShaperailError::Validation(vec![
    FieldError {
        field: "start_date".into(),
        message: "must be before end_date".into(),
        code: "invalid_range".into(),
    },
]));
```

### 6. Keep job handlers idempotent

Background jobs may be retried. Design handlers so that running the same job
twice does not produce duplicate side effects. Check for existing state before
performing the action.

### 7. Run shaperail check in CI

Catch resource file errors before they reach runtime. The diagnostic codes are
stable and machine-readable, so you can track specific error classes across your
project.

```bash
shaperail check
```
