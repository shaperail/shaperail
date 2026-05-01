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

> **Note:** `controller: { before, after }` is only valid on conventional CRUD
> endpoints (`list` / `get` / `create` / `update` / `delete` / `bulk_create` /
> `bulk_delete`). Declaring `controller:` on a custom endpoint (one that uses
> `handler:`) now fails `shaperail check` with a clear error. For shared logic
> on custom handlers, use the `Subject` API from `shaperail_runtime::auth`
> directly inside your handler — see [Multi-tenancy]({{ '/multi-tenancy/' | relative_url }})
> for the pattern.

## Declaring controllers

Add a `controller` field to any CRUD endpoint in your resource YAML:

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
    auth: [admin]
    input: [email, name, role, org_id]
    controller:
      before: validate_org
      after: enrich_response
    events: [user.created]
    jobs: [send_welcome_email]

  update:
    auth: [admin, owner]
    input: [name, role]
    controller:
      before: normalize_name
```

Each endpoint supports at most one `before` and one `after` function. Both are
optional — you can declare just `before`, just `after`, or both.

---

## Writing controller functions

One workable convention is a file co-located with the resource YAML:

```text
resources/
  users.yaml                # schema + endpoints
  users.controller.rs       # controller module for users
  orders.yaml
  orders.controller.rs
```

Current limitation: scaffolded apps do not auto-discover controller files or
populate the controller map yet. The runtime controller API exists today, but
you must register functions yourself during bootstrap.

```rust
// src/main.rs or another bootstrap module
#[path = "../resources/users.controller.rs"]
mod users_controller;

let mut controllers = generated::build_controller_map();
controllers.register("users", "validate_org", users_controller::validate_org);
```

The YAML declares what exists; the registered controller functions define the
extra runtime behavior.

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
```

Function names you register in the controller map must match what is declared
in the YAML.

---

## Generated helper stubs

`shaperail generate` currently writes controller-related helper artifacts into
`generated/mod.rs`, including typed input structs and trait stubs for resources
that declare controllers.

Current limitation: those generated trait stubs still use legacy
`ControllerContext` naming in their comments/signatures. Treat them as
reference material only. The callable runtime controller API is the
`&mut shaperail_runtime::handlers::controller::Context` signature shown in this
guide.

---

## Context API

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
| `session` | `serde_json::Map<String, Value>` | before + after | Cross-phase scratch space. Anything written in `before:` is visible in `after:`. Never persisted, never serialized to the client. |
| `response_extras` | `serde_json::Map<String, Value>` | before + after | Keys merged into the response's `data:` envelope after the after-hook returns. Never persisted. Use for one-time values like minted secrets that must reach the client exactly once. |

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

### Preserved-Context lifecycle

The **same `Context` struct instance** is used for both the before- and
after-phase of a single request. This means:

- `ctx.input` is not reset between phases — modifications from the before-phase
  are still visible in the after-phase.
- State written to `ctx.session` in `before:` is readable in `after:`.
- Keys placed in `ctx.response_extras` in either phase are merged into the
  outgoing response's `data:` envelope before the HTTP response is sent. They
  are never written to the database.

Use `ctx.session` to communicate between the two phases without touching
`ctx.input` or `ctx.data`.

---

## Common patterns

### One-time secret on create

Use `ctx.session` to pass a plaintext value from the before-phase to the
after-phase without exposing it to the database or to intermediate log output.
The after-phase then places it in `ctx.response_extras` so it appears in the
response exactly once:

```rust
// resources/agents.controller.rs
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

/// Mints a 32-byte secret on agent create:
///  - before: stores the hash on the row, stashes plaintext in session
///  - after:  moves plaintext from session into response_extras
pub async fn mint_mcp_secret(ctx: &mut Context) -> ControllerResult {
    if ctx.data.is_none() {
        // before-phase: write hash to DB, stash plaintext for the after-hook
        let plaintext = generate_random_secret_32_bytes();
        let hash = hash_secret(&plaintext);
        ctx.input.insert("mcp_secret_hash".into(), serde_json::json!(hash));
        ctx.session.insert("plaintext".into(), serde_json::json!(plaintext));
    } else {
        // after-phase: hand the plaintext to the response
        if let Some(plaintext) = ctx.session.remove("plaintext") {
            ctx.response_extras.insert("mcp_secret".into(), plaintext);
        }
    }
    Ok(())
}
```

The response `data:` envelope will contain `"mcp_secret": "<value>"` for this
one request. Subsequent reads of the agent record will not include it.

### Auto-fill `created_by` from token

```rust
pub async fn set_created_by(ctx: &mut Context) -> ControllerResult {
    if let Some(user) = &ctx.user {
        ctx.input["created_by"] = serde_json::json!(user.id);
    } else {
        return Err(ShaperailError::Unauthorized);
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

## Enterprise patterns

These patterns address real-world requirements that large organizations face:
multi-step approval workflows, cross-resource transactions, audit trails,
compliance enforcement, and external service integration.

### Multi-step approval workflow

Implement a state-machine for resources that require approval before going live.
The controller enforces valid state transitions and checks role-based approval
authority.

```yaml
# resources/documents.yaml
resource: documents
version: 1

schema:
  id:            { type: uuid, primary: true, generated: true }
  title:         { type: string, required: true }
  body:          { type: string, required: true }
  status:        { type: enum, values: [draft, pending_review, approved, published, rejected], default: draft }
  submitted_by:  { type: uuid, nullable: true }
  reviewed_by:   { type: uuid, nullable: true }
  approved_by:   { type: uuid, nullable: true }
  rejection_reason: { type: string, nullable: true }
  org_id:        { type: uuid, ref: organizations.id, required: true }
  created_at:    { type: timestamp, generated: true }
  updated_at:    { type: timestamp, generated: true }

endpoints:
  update:
    auth: [member, reviewer, admin]
    input: [title, body, status, rejection_reason]
    controller:
      before: enforce_workflow
      after: notify_stakeholders
    events: [document.status_changed]
```

```rust
// resources/documents.controller.rs
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use shaperail_core::{ShaperailError, FieldError};

/// Allowed state transitions and the roles that can perform them.
const TRANSITIONS: &[(&str, &str, &[&str])] = &[
    // (from,           to,              allowed_roles)
    ("draft",           "pending_review", &["member", "admin"]),
    ("pending_review",  "approved",       &["reviewer", "admin"]),
    ("pending_review",  "rejected",       &["reviewer", "admin"]),
    ("approved",        "published",      &["admin"]),
    ("rejected",        "draft",          &["member", "admin"]),   // re-submit
    ("published",       "draft",          &["admin"]),             // unpublish
];

pub async fn enforce_workflow(ctx: &mut Context) -> ControllerResult {
    let new_status = match ctx.input.get("status").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return Ok(()), // not changing status, skip
    };

    // Fetch current status from DB
    let doc_id: uuid::Uuid = ctx.input.get("id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or(ShaperailError::Internal("Missing document id".into()))?;

    let current_status: String = sqlx::query_scalar(
        "SELECT status FROM documents WHERE id = $1"
    )
    .bind(doc_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|_| ShaperailError::NotFound)?;

    // Check if this transition is valid
    let user_role = ctx.user.as_ref()
        .map(|u| u.role.as_str())
        .unwrap_or("anonymous");

    let allowed = TRANSITIONS.iter().any(|(from, to, roles)| {
        *from == current_status && *to == new_status && roles.contains(&user_role)
    });

    if !allowed {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "status".into(),
            message: format!(
                "cannot transition from '{}' to '{}' with role '{}'",
                current_status, new_status, user_role
            ),
            code: "invalid_transition".into(),
        }]));
    }

    // Auto-fill audit fields based on transition
    match new_status.as_str() {
        "pending_review" => {
            if let Some(user) = &ctx.user {
                ctx.input["submitted_by"] = serde_json::json!(user.id);
            }
        }
        "approved" => {
            if let Some(user) = &ctx.user {
                ctx.input["approved_by"] = serde_json::json!(user.id);
            }
            ctx.input.remove("rejection_reason");
        }
        "rejected" => {
            if let Some(user) = &ctx.user {
                ctx.input["reviewed_by"] = serde_json::json!(user.id);
            }
            // Require rejection reason
            if ctx.input.get("rejection_reason")
                .and_then(|v| v.as_str())
                .map_or(true, |s| s.trim().is_empty())
            {
                return Err(ShaperailError::Validation(vec![FieldError {
                    field: "rejection_reason".into(),
                    message: "rejection reason is required when rejecting".into(),
                    code: "required_for_rejection".into(),
                }]));
            }
        }
        _ => {}
    }

    Ok(())
}

pub async fn notify_stakeholders(ctx: &mut Context) -> ControllerResult {
    if let Some(data) = &ctx.data {
        let status = data["status"].as_str().unwrap_or("");
        let doc_id = data["id"].as_str().unwrap_or("");

        // Add header so clients know a notification was sent
        ctx.response_headers.push((
            "X-Notification-Sent".into(),
            format!("document.{status}"),
        ));
    }
    Ok(())
}
```

### Cross-resource validation with transactions

When creating or updating a resource requires checking constraints across
multiple tables, use `ctx.pool` to run queries within the same connection.

```yaml
# resources/orders.yaml
resource: orders
version: 1

schema:
  id:           { type: uuid, primary: true, generated: true }
  user_id:      { type: uuid, ref: users.id, required: true }
  product_id:   { type: uuid, ref: products.id, required: true }
  quantity:     { type: integer, min: 1, required: true }
  total_cents:  { type: bigint, required: true }
  status:       { type: enum, values: [pending, confirmed, shipped, delivered, cancelled], default: pending }
  created_at:   { type: timestamp, generated: true }
  updated_at:   { type: timestamp, generated: true }

endpoints:
  create:
    auth: [member, admin]
    input: [user_id, product_id, quantity]
    controller:
      before: validate_and_reserve
    events: [order.created]
    jobs: [send_order_confirmation]
```

```rust
// resources/orders.controller.rs
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use shaperail_core::{ShaperailError, FieldError};

/// Validate inventory, calculate price, and reserve stock — all in one controller.
pub async fn validate_and_reserve(ctx: &mut Context) -> ControllerResult {
    let product_id: uuid::Uuid = serde_json::from_value(
        ctx.input["product_id"].clone()
    ).map_err(|_| ShaperailError::Validation(vec![FieldError {
        field: "product_id".into(),
        message: "invalid product ID".into(),
        code: "invalid_uuid".into(),
    }]))?;

    let quantity: i32 = ctx.input.get("quantity")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32)
        .unwrap_or(0);

    // Check product exists and has enough stock
    let product = sqlx::query_as::<_, (i32, i64, bool)>(
        "SELECT stock_count, price_cents, is_active FROM products WHERE id = $1"
    )
    .bind(product_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?
    .ok_or(ShaperailError::Validation(vec![FieldError {
        field: "product_id".into(),
        message: "product not found".into(),
        code: "not_found".into(),
    }]))?;

    let (stock, price_cents, is_active) = product;

    if !is_active {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "product_id".into(),
            message: "product is no longer available".into(),
            code: "product_inactive".into(),
        }]));
    }

    if stock < quantity {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "quantity".into(),
            message: format!("only {} units available", stock),
            code: "insufficient_stock".into(),
        }]));
    }

    // Calculate total and inject into input
    let total = price_cents * quantity as i64;
    ctx.input["total_cents"] = serde_json::json!(total);

    // Reserve stock (decrement)
    sqlx::query("UPDATE products SET stock_count = stock_count - $1 WHERE id = $2")
        .bind(quantity)
        .bind(product_id)
        .execute(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    Ok(())
}
```

### Comprehensive audit trail

Maintain a complete audit log of every mutation with the user, timestamp, IP,
and before/after snapshots. Useful for compliance (SOC 2, HIPAA, GDPR).

```yaml
# resources/accounts.yaml
resource: accounts
version: 1

schema:
  id:         { type: uuid, primary: true, generated: true }
  name:       { type: string, required: true }
  balance:    { type: bigint, required: true }
  status:     { type: enum, values: [active, suspended, closed], default: active }
  org_id:     { type: uuid, ref: organizations.id, required: true }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }

endpoints:
  update:
    auth: [admin]
    input: [name, balance, status]
    controller:
      before: capture_snapshot
      after: write_audit_log
```

```rust
// resources/accounts.controller.rs
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use shaperail_core::ShaperailError;

/// Capture the current state before the update for auditing.
pub async fn capture_snapshot(ctx: &mut Context) -> ControllerResult {
    let account_id = ctx.input.get("id")
        .and_then(|v| v.as_str())
        .ok_or(ShaperailError::Internal("Missing account id".into()))?;

    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT row_to_json(a) FROM accounts a WHERE id = $1::uuid"
    )
    .bind(account_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    // Stash the snapshot in a response header so the after-controller can read it.
    // This is a pattern for passing data between before and after controllers.
    ctx.response_headers.push((
        "X-Audit-Before".into(),
        before.to_string(),
    ));

    Ok(())
}

/// Write an audit log entry after the update completes.
pub async fn write_audit_log(ctx: &mut Context) -> ControllerResult {
    let user_id = ctx.user.as_ref()
        .map(|u| u.id.clone())
        .unwrap_or_else(|| "system".to_string());

    let before_json = ctx.response_headers.iter()
        .find(|(k, _)| k == "X-Audit-Before")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "null".to_string());

    // Remove the internal header — clients should not see it
    ctx.response_headers.retain(|(k, _)| k != "X-Audit-Before");

    let after_json = ctx.data.as_ref()
        .map(|d| d.to_string())
        .unwrap_or_else(|| "null".to_string());

    let resource_id = ctx.data.as_ref()
        .and_then(|d| d["id"].as_str())
        .unwrap_or("unknown");

    let ip_address = ctx.headers.get("x-forwarded-for")
        .or_else(|| ctx.headers.get("x-real-ip"))
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());

    sqlx::query(
        "INSERT INTO audit_logs (user_id, resource_type, resource_id, action, before_data, after_data, ip_address, created_at)
         VALUES ($1, 'accounts', $2, 'update', $3::jsonb, $4::jsonb, $5, NOW())"
    )
    .bind(&user_id)
    .bind(resource_id)
    .bind(&before_json)
    .bind(&after_json)
    .bind(&ip_address)
    .execute(&ctx.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to write audit log: {}", e);
        // Don't fail the request for audit log errors — log and continue
        ShaperailError::Internal(e.to_string())
    })?;

    Ok(())
}
```

### External service integration (idempotent)

Call an external API (payment processor, identity provider, CRM) during a
controller, with idempotency keys to prevent double-processing.

```yaml
# resources/subscriptions.yaml
resource: subscriptions
version: 1

schema:
  id:             { type: uuid, primary: true, generated: true }
  user_id:        { type: uuid, ref: users.id, required: true }
  plan:           { type: enum, values: [free, starter, pro, enterprise], required: true }
  stripe_sub_id:  { type: string, nullable: true }
  status:         { type: enum, values: [active, past_due, cancelled], default: active }
  org_id:         { type: uuid, ref: organizations.id, required: true }
  created_at:     { type: timestamp, generated: true }
  updated_at:     { type: timestamp, generated: true }

endpoints:
  create:
    auth: [admin]
    input: [user_id, plan, org_id]
    controller:
      before: create_stripe_subscription
    events: [subscription.created]

  update:
    auth: [admin]
    input: [plan, status]
    controller:
      before: update_stripe_subscription
    events: [subscription.updated]
```

```rust
// resources/subscriptions.controller.rs
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use shaperail_core::{ShaperailError, FieldError};

/// Create a Stripe subscription before saving to our database.
/// Uses an idempotency key to prevent double charges on retries.
pub async fn create_stripe_subscription(ctx: &mut Context) -> ControllerResult {
    let user_id = ctx.input.get("user_id")
        .and_then(|v| v.as_str())
        .ok_or(ShaperailError::Validation(vec![FieldError {
            field: "user_id".into(),
            message: "user_id is required".into(),
            code: "required".into(),
        }]))?;

    let plan = ctx.input.get("plan")
        .and_then(|v| v.as_str())
        .ok_or(ShaperailError::Validation(vec![FieldError {
            field: "plan".into(),
            message: "plan is required".into(),
            code: "required".into(),
        }]))?;

    // Look up user's Stripe customer ID
    let stripe_customer_id: Option<String> = sqlx::query_scalar(
        "SELECT stripe_customer_id FROM users WHERE id = $1::uuid"
    )
    .bind(user_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?
    .flatten();

    let customer_id = stripe_customer_id.ok_or(ShaperailError::Validation(vec![
        FieldError {
            field: "user_id".into(),
            message: "user has no payment method on file".into(),
            code: "no_payment_method".into(),
        },
    ]))?;

    // Map plan to Stripe price ID
    let price_id = match plan {
        "starter" => "price_starter_monthly",
        "pro" => "price_pro_monthly",
        "enterprise" => "price_enterprise_monthly",
        "free" => {
            // Free plan — no Stripe subscription needed
            ctx.input["stripe_sub_id"] = serde_json::json!(null);
            return Ok(());
        }
        _ => return Err(ShaperailError::Validation(vec![FieldError {
            field: "plan".into(),
            message: format!("unknown plan '{plan}'"),
            code: "invalid_plan".into(),
        }])),
    };

    // Generate idempotency key from request ID to prevent double charges
    let idempotency_key = ctx.headers.get("x-request-id")
        .cloned()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Call Stripe API
    let client = reqwest::Client::new();
    let stripe_key = std::env::var("STRIPE_SECRET_KEY")
        .map_err(|_| ShaperailError::Internal("STRIPE_SECRET_KEY not set".into()))?;

    let response = client
        .post("https://api.stripe.com/v1/subscriptions")
        .header("Authorization", format!("Bearer {stripe_key}"))
        .header("Idempotency-Key", &idempotency_key)
        .form(&[
            ("customer", customer_id.as_str()),
            ("items[0][price]", price_id),
        ])
        .send()
        .await
        .map_err(|e| ShaperailError::Internal(format!("Stripe API error: {e}")))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        tracing::error!("Stripe subscription creation failed: {body}");
        return Err(ShaperailError::Internal(
            "Payment provider error — please try again".into()
        ));
    }

    let stripe_sub: serde_json::Value = response.json().await
        .map_err(|e| ShaperailError::Internal(format!("Stripe response parse error: {e}")))?;

    // Inject the Stripe subscription ID into the input
    ctx.input["stripe_sub_id"] = stripe_sub["id"].clone();

    Ok(())
}

/// Update the Stripe subscription when the plan changes.
pub async fn update_stripe_subscription(ctx: &mut Context) -> ControllerResult {
    // Only call Stripe if plan is actually changing
    let new_plan = match ctx.input.get("plan").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return Ok(()),
    };

    let sub_id_opt: Option<String> = ctx.input.get("id")
        .and_then(|v| v.as_str())
        .map(|id| async move {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT stripe_sub_id FROM subscriptions WHERE id = $1::uuid"
            )
            .bind(id)
            .fetch_one(&ctx.pool)
            .await
            .ok()
            .flatten()
        })
        .map(|fut| tokio::runtime::Handle::current().block_on(fut))
        .flatten();

    if let Some(stripe_sub_id) = sub_id_opt {
        tracing::info!(
            stripe_sub_id = %stripe_sub_id,
            new_plan = %new_plan,
            "Updating Stripe subscription"
        );
        // Call Stripe API to update the subscription...
        // (similar pattern to create_stripe_subscription)
    }

    Ok(())
}
```

### Row-level security beyond tenant_key

For cases where `tenant_key` alone is not sufficient — e.g., department-level
isolation, project-based access, or hierarchical permissions.

```rust
// resources/confidential_reports.controller.rs
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use shaperail_core::{ShaperailError, FieldError};

/// Enforce department-level access control.
/// Users can only see reports from their own department,
/// managers can see reports from any department in their org.
pub async fn enforce_department_access(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref()
        .ok_or(ShaperailError::Unauthorized)?;

    let report_id = ctx.input.get("id")
        .and_then(|v| v.as_str())
        .ok_or(ShaperailError::Internal("Missing report id".into()))?;

    // Fetch the report's department
    let report_dept: String = sqlx::query_scalar(
        "SELECT department_id FROM confidential_reports WHERE id = $1::uuid"
    )
    .bind(report_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|_| ShaperailError::NotFound)?;

    // Fetch user's department and manager status
    let (user_dept, is_manager): (String, bool) = sqlx::query_as(
        "SELECT department_id, is_manager FROM users WHERE id = $1"
    )
    .bind(&user.id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    // Managers can access any department in their org (tenant_key handles org isolation)
    // Non-managers can only access their own department
    if !is_manager && user_dept != report_dept {
        return Err(ShaperailError::Forbidden);
    }

    Ok(())
}
```

### Data masking based on role

Return different levels of detail depending on the requester's role.
For example, only admins see full SSNs, everyone else sees `***-**-1234`.

```rust
// resources/employees.controller.rs
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use shaperail_core::ShaperailError;

/// Mask sensitive fields based on the user's role.
pub async fn mask_sensitive_fields(ctx: &mut Context) -> ControllerResult {
    let is_admin = ctx.user.as_ref().map_or(false, |u| u.role == "admin");
    let is_hr = ctx.user.as_ref().map_or(false, |u| u.role == "hr");

    if let Some(data) = &mut ctx.data {
        if let Some(obj) = data.as_object_mut() {
            // SSN: only admin and HR see full value
            if !is_admin && !is_hr {
                if let Some(ssn) = obj.get("ssn").and_then(|v| v.as_str()) {
                    if ssn.len() >= 4 {
                        let masked = format!("***-**-{}", &ssn[ssn.len()-4..]);
                        obj["ssn"] = serde_json::json!(masked);
                    }
                }
            }

            // Salary: only admin sees this
            if !is_admin {
                obj.remove("salary_cents");
                obj.remove("bonus_cents");
            }

            // Home address: only admin and HR
            if !is_admin && !is_hr {
                obj.remove("home_address");
                obj.remove("phone_personal");
            }
        }
    }

    Ok(())
}
```

### Rate limiting per operation

Apply custom rate limits beyond the global rate limiter — e.g., limit password
reset requests to 3 per hour per user, or limit bulk exports.

```rust
// resources/password_resets.controller.rs
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use shaperail_core::{ShaperailError, FieldError};

/// Custom rate limit: max 3 password reset requests per email per hour.
pub async fn rate_limit_reset(ctx: &mut Context) -> ControllerResult {
    let email = ctx.input.get("email")
        .and_then(|v| v.as_str())
        .ok_or(ShaperailError::Validation(vec![FieldError {
            field: "email".into(),
            message: "email is required".into(),
            code: "required".into(),
        }]))?;

    let recent_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM password_resets
         WHERE email = $1 AND created_at > NOW() - INTERVAL '1 hour'"
    )
    .bind(email)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    if recent_count >= 3 {
        ctx.response_headers.push((
            "Retry-After".into(),
            "3600".into(),
        ));
        return Err(ShaperailError::RateLimited(
            "Too many password reset requests. Try again in 1 hour.".into()
        ));
    }

    Ok(())
}
```

### Composing multiple validation steps

Since each endpoint supports only one `before` function, compose multiple
checks within a single controller function:

```rust
// resources/invoices.controller.rs
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use shaperail_core::ShaperailError;

pub async fn validate_invoice(ctx: &mut Context) -> ControllerResult {
    validate_customer(ctx).await?;
    validate_line_items(ctx).await?;
    calculate_totals(ctx).await?;
    enforce_credit_limit(ctx).await?;
    Ok(())
}

async fn validate_customer(ctx: &mut Context) -> ControllerResult {
    let customer_id = ctx.input.get("customer_id")
        .and_then(|v| v.as_str())
        .ok_or(ShaperailError::Validation(vec![
            shaperail_core::FieldError {
                field: "customer_id".into(),
                message: "customer is required".into(),
                code: "required".into(),
            }
        ]))?;

    let is_active: bool = sqlx::query_scalar(
        "SELECT is_active FROM customers WHERE id = $1::uuid"
    )
    .bind(customer_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?
    .unwrap_or(false);

    if !is_active {
        return Err(ShaperailError::Validation(vec![
            shaperail_core::FieldError {
                field: "customer_id".into(),
                message: "customer account is not active".into(),
                code: "customer_inactive".into(),
            }
        ]));
    }

    Ok(())
}

async fn validate_line_items(ctx: &mut Context) -> ControllerResult {
    let items = ctx.input.get("line_items")
        .and_then(|v| v.as_array())
        .ok_or(ShaperailError::Validation(vec![
            shaperail_core::FieldError {
                field: "line_items".into(),
                message: "at least one line item is required".into(),
                code: "required".into(),
            }
        ]))?;

    if items.is_empty() {
        return Err(ShaperailError::Validation(vec![
            shaperail_core::FieldError {
                field: "line_items".into(),
                message: "at least one line item is required".into(),
                code: "min_items".into(),
            }
        ]));
    }

    for (i, item) in items.iter().enumerate() {
        if item.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0) <= 0 {
            return Err(ShaperailError::Validation(vec![
                shaperail_core::FieldError {
                    field: format!("line_items[{i}].quantity"),
                    message: "quantity must be positive".into(),
                    code: "min_value".into(),
                }
            ]));
        }
    }

    Ok(())
}

async fn calculate_totals(ctx: &mut Context) -> ControllerResult {
    let items = ctx.input.get("line_items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let subtotal: i64 = items.iter()
        .map(|item| {
            let qty = item["quantity"].as_i64().unwrap_or(0);
            let price = item["unit_price_cents"].as_i64().unwrap_or(0);
            qty * price
        })
        .sum();

    let tax_rate = 0.08; // 8% — in production, fetch from tax service
    let tax = (subtotal as f64 * tax_rate) as i64;

    ctx.input["subtotal_cents"] = serde_json::json!(subtotal);
    ctx.input["tax_cents"] = serde_json::json!(tax);
    ctx.input["total_cents"] = serde_json::json!(subtotal + tax);

    Ok(())
}

async fn enforce_credit_limit(ctx: &mut Context) -> ControllerResult {
    let customer_id = ctx.input.get("customer_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let total = ctx.input.get("total_cents")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let (credit_limit, outstanding): (i64, i64) = sqlx::query_as(
        "SELECT c.credit_limit_cents,
                COALESCE(SUM(i.total_cents) FILTER (WHERE i.status = 'outstanding'), 0)
         FROM customers c
         LEFT JOIN invoices i ON i.customer_id = c.id
         WHERE c.id = $1::uuid
         GROUP BY c.credit_limit_cents"
    )
    .bind(customer_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    if outstanding + total > credit_limit {
        return Err(ShaperailError::Validation(vec![
            shaperail_core::FieldError {
                field: "total_cents".into(),
                message: format!(
                    "invoice total ({}) would exceed credit limit ({}). Outstanding: {}",
                    total, credit_limit, outstanding
                ),
                code: "credit_limit_exceeded".into(),
            }
        ]));
    }

    Ok(())
}
```

---

## ControllerMap registry

The runtime uses a `ControllerMap` shared across request handlers via
`AppState`. The map stores `(resource_name, function_name) → function` entries.

Current limitation: generated code currently returns an empty controller map.
If you want controllers to run, register them during bootstrap yourself.

```rust
// In your app bootstrap
let mut controllers = generated::build_controller_map();
controllers.register("users", "validate_org", users_controller::validate_org);
controllers.register("users", "enrich_response", users_controller::enrich_response);
controllers.register("users", "normalize_name", users_controller::normalize_name);
```

Keep the registered names aligned with the YAML declarations. If a function is
declared in YAML but not registered here, the runtime returns a controller-not-
found error when that endpoint executes.

---

## Complete implementation walkthrough

The walkthrough below combines the enterprise patterns above into one larger
billing service. It is not a separate product template or special doc type; it
is a normal controller example that shows how a bigger team can wire several
controllers, resources, and migrations together in one app.

### What you are building

This example is a multi-tenant billing API with:

- customer plan enforcement
- invoice approval workflow
- payment validation and invoice reconciliation
- audit logs for finance-sensitive mutations
- manual controller registration in the current runtime

The service exposes three versioned resources:

- `customers` for billing accounts and plan limits
- `invoices` for finance-reviewed invoices with explicit status transitions
- `payments` for payment capture and automatic invoice reconciliation

All three resources use `tenant_key: org_id`, so the authenticated user's
`tenant_id` claim scopes every request automatically.

This walkthrough assumes your platform already has an `organizations` resource
or tenant directory elsewhere. The finance service starts at
`customers -> invoices -> payments`.

### Project layout

```text
enterprise-saas/
  resources/
    customers.yaml
    customers.controller.rs
    invoices.yaml
    invoices.controller.rs
    payments.yaml
    payments.controller.rs
  migrations/
    0001_create_customers.sql
    0002_create_invoices.sql
    0003_create_payments.sql
    0004_create_audit_logs.sql
  src/
    main.rs
  seeds/
    customers.yaml
  shaperail.config.yaml
  docker-compose.yml
  .env
  requests.http
```

### Step 1: Scaffold the app

```bash
shaperail init enterprise-saas
cd enterprise-saas
docker compose up -d
```

Then replace the scaffolded resource files with the ones below, add the
controller modules, and update `src/main.rs` to register them.

Current limitation: controller modules are not auto-discovered by the
scaffolded app. The runtime supports them, but you must register them manually.

### Step 2: Define the resource contracts

#### `resources/customers.yaml`

```yaml
resource: customers
version: 1
tenant_key: org_id

schema:
  id:                 { type: uuid, primary: true, generated: true }
  org_id:             { type: uuid, ref: organizations.id, required: true }
  name:               { type: string, min: 1, max: 200, required: true }
  email:              { type: string, format: email, unique: true, required: true }
  plan:               { type: enum, values: [free, starter, pro, enterprise], default: starter }
  status:             { type: enum, values: [active, suspended, closed], default: active }
  credit_limit_cents: { type: bigint, default: 0 }
  created_by:         { type: uuid, required: true }
  deleted_at:         { type: timestamp, nullable: true }
  created_at:         { type: timestamp, generated: true }
  updated_at:         { type: timestamp, generated: true }

endpoints:
  list:
    auth: [finance, admin]
    filters: [plan, status]
    search: [name, email]
    pagination: cursor

  get:
    auth: [finance, admin]

  create:
    auth: [admin]
    input: [name, email, plan, status, credit_limit_cents]
    controller:
      before: validate_customer

  update:
    auth: [finance, admin]
    input: [plan, status, credit_limit_cents]
    controller:
      before: enforce_plan_change

  delete:
    auth: [admin]
    soft_delete: true

indexes:
  - { fields: [org_id, plan] }
  - { fields: [email], unique: true }
```

#### `resources/invoices.yaml`

```yaml
resource: invoices
version: 1
tenant_key: org_id

schema:
  id:             { type: uuid, primary: true, generated: true }
  org_id:         { type: uuid, ref: organizations.id, required: true }
  customer_id:    { type: uuid, ref: customers.id, required: true }
  invoice_number: { type: string, unique: true, required: true }
  status:         { type: enum, values: [draft, pending, sent, paid, void, overdue], default: draft }
  subtotal_cents: { type: bigint, required: true }
  tax_cents:      { type: bigint, default: 0 }
  total_cents:    { type: bigint, required: true }
  due_date:       { type: date, required: true }
  notes:          { type: string, nullable: true }
  sent_at:        { type: timestamp, nullable: true }
  paid_at:        { type: timestamp, nullable: true }
  created_by:     { type: uuid, required: true }
  deleted_at:     { type: timestamp, nullable: true }
  created_at:     { type: timestamp, generated: true }
  updated_at:     { type: timestamp, generated: true }

endpoints:
  list:
    auth: [finance, admin]
    filters: [status, customer_id]
    search: [invoice_number]
    pagination: offset
    sort: [created_at, due_date]

  get:
    auth: [finance, admin]

  create:
    auth: [finance, admin]
    input: [customer_id, subtotal_cents, tax_cents, total_cents, due_date, notes]
    controller:
      before: prepare_invoice

  update:
    auth: [finance, admin]
    input: [status, due_date, notes]
    controller:
      before: enforce_invoice_workflow
      after: audit_invoice_change

  delete:
    auth: [admin]
    soft_delete: true

indexes:
  - { fields: [org_id, status] }
  - { fields: [invoice_number], unique: true }
  - { fields: [customer_id, due_date] }
```

#### `resources/payments.yaml`

```yaml
resource: payments
version: 1
tenant_key: org_id

schema:
  id:               { type: uuid, primary: true, generated: true }
  org_id:           { type: uuid, ref: organizations.id, required: true }
  invoice_id:       { type: uuid, ref: invoices.id, required: true }
  amount_cents:     { type: bigint, required: true }
  method:           { type: enum, values: [card, ach, wire, manual], required: true }
  status:           { type: enum, values: [pending, completed, failed, refunded], default: pending }
  reference_number: { type: string, nullable: true }
  completed_at:     { type: timestamp, nullable: true }
  created_by:       { type: uuid, required: true }
  created_at:       { type: timestamp, generated: true }
  updated_at:       { type: timestamp, generated: true }

endpoints:
  list:
    auth: [finance, admin]
    filters: [invoice_id, status, method]
    pagination: offset
    sort: [created_at]

  get:
    auth: [finance, admin]

  create:
    auth: [finance, admin]
    input: [invoice_id, amount_cents, method, reference_number, status]
    controller:
      before: validate_payment
      after: reconcile_invoice_status

  update:
    auth: [finance, admin]
    input: [status, reference_number]
    controller:
      before: lock_payment_state
      after: reconcile_invoice_status

indexes:
  - { fields: [org_id, status] }
  - { fields: [invoice_id, created_at], order: desc }
```

### Step 3: Register controller modules in `src/main.rs`

The scaffold creates:

```rust
let controllers = generated::build_controller_map();
```

Replace that with explicit registration:

```rust
#[path = "../resources/customers.controller.rs"]
mod customers_controller;
#[path = "../resources/invoices.controller.rs"]
mod invoices_controller;
#[path = "../resources/payments.controller.rs"]
mod payments_controller;

fn build_custom_controller_map() -> shaperail_runtime::handlers::controller::ControllerMap {
    let mut controllers = generated::build_controller_map();

    controllers.register("customers", "validate_customer", customers_controller::validate_customer);
    controllers.register("customers", "enforce_plan_change", customers_controller::enforce_plan_change);

    controllers.register("invoices", "prepare_invoice", invoices_controller::prepare_invoice);
    controllers.register("invoices", "enforce_invoice_workflow", invoices_controller::enforce_invoice_workflow);
    controllers.register("invoices", "audit_invoice_change", invoices_controller::audit_invoice_change);

    controllers.register("payments", "validate_payment", payments_controller::validate_payment);
    controllers.register("payments", "lock_payment_state", payments_controller::lock_payment_state);
    controllers.register("payments", "reconcile_invoice_status", payments_controller::reconcile_invoice_status);

    controllers
}
```

Then replace the scaffolded line with:

```rust
let controllers = build_custom_controller_map();
```

Everything else in `AppState` stays the same.

### Step 4: Implement the customer controllers

`customers.controller.rs` enforces plan policy before customer rows are written.

```rust
use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

fn plan_rank(plan: &str) -> Option<i32> {
    match plan {
        "free" => Some(0),
        "starter" => Some(1),
        "pro" => Some(2),
        "enterprise" => Some(3),
        _ => None,
    }
}

fn max_credit_limit(plan: &str) -> Option<i64> {
    match plan {
        "free" => Some(0),
        "starter" => Some(50_000),
        "pro" => Some(500_000),
        "enterprise" => None,
        _ => Some(0),
    }
}

pub async fn validate_customer(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
    ctx.input["created_by"] = serde_json::json!(user.id);

    if !ctx.input.contains_key("org_id") {
        if let Some(tenant_id) = &ctx.tenant_id {
            ctx.input["org_id"] = serde_json::json!(tenant_id);
        }
    }

    let plan = ctx.input.get("plan").and_then(|v| v.as_str()).unwrap_or("starter");
    let credit_limit = ctx.input
        .get("credit_limit_cents")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    if let Some(max) = max_credit_limit(plan) {
        if credit_limit > max {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "credit_limit_cents".into(),
                message: format!("plan '{plan}' cannot exceed {max} cents"),
                code: "plan_limit_exceeded".into(),
            }]));
        }
    }

    Ok(())
}

pub async fn enforce_plan_change(ctx: &mut Context) -> ControllerResult {
    let customer_id = match ctx.input.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return Ok(()),
    };
    let new_plan = match ctx.input.get("plan").and_then(|v| v.as_str()) {
        Some(plan) => plan,
        None => return Ok(()),
    };

    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
    if user.role != "admin" && user.role != "finance" {
        return Err(ShaperailError::Forbidden);
    }

    let current_plan: String = sqlx::query_scalar(
        "SELECT plan FROM customers WHERE id = $1::uuid"
    )
    .bind(customer_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?
    .ok_or(ShaperailError::NotFound)?;

    let outstanding_invoices: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total_cents), 0)
         FROM invoices
         WHERE customer_id = $1::uuid
           AND status IN ('pending', 'sent', 'overdue')"
    )
    .bind(customer_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    let current_rank = plan_rank(&current_plan).unwrap_or_default();
    let new_rank = plan_rank(new_plan).unwrap_or_default();

    if (current_rank - new_rank).abs() > 1 {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "plan".into(),
            message: "plan changes can only move one tier at a time".into(),
            code: "invalid_plan_jump".into(),
        }]));
    }

    if new_rank < current_rank && outstanding_invoices > 0 {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "plan".into(),
            message: "cannot downgrade while invoices are still outstanding".into(),
            code: "outstanding_balance".into(),
        }]));
    }

    Ok(())
}
```

### Step 5: Implement the invoice controllers

This module handles invoice number generation, workflow transitions, and audit
logging.

```rust
use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

pub async fn prepare_invoice(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
    ctx.input["created_by"] = serde_json::json!(user.id);

    if !ctx.input.contains_key("org_id") {
        if let Some(tenant_id) = &ctx.tenant_id {
            ctx.input["org_id"] = serde_json::json!(tenant_id);
        }
    }

    let customer_id = ctx.input
        .get("customer_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShaperailError::Validation(vec![FieldError {
            field: "customer_id".into(),
            message: "customer_id is required".into(),
            code: "required".into(),
        }]))?;

    let customer_status: String = sqlx::query_scalar(
        "SELECT status FROM customers WHERE id = $1::uuid"
    )
    .bind(customer_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?
    .ok_or(ShaperailError::NotFound)?;

    if customer_status != "active" {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "customer_id".into(),
            message: "customer must be active before creating invoices".into(),
            code: "customer_inactive".into(),
        }]));
    }

    let subtotal = ctx.input.get("subtotal_cents").and_then(|v| v.as_i64()).unwrap_or(0);
    let tax = ctx.input.get("tax_cents").and_then(|v| v.as_i64()).unwrap_or(0);
    let total = ctx.input.get("total_cents").and_then(|v| v.as_i64()).unwrap_or(0);

    if subtotal + tax != total {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "total_cents".into(),
            message: "total_cents must equal subtotal_cents + tax_cents".into(),
            code: "invalid_total".into(),
        }]));
    }

    let today = chrono::Utc::now().format("%Y%m%d").to_string();
    let prefix = format!("INV-{today}-%");
    let count_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoices WHERE invoice_number LIKE $1"
    )
    .bind(prefix)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    ctx.input["invoice_number"] = serde_json::json!(format!(
        "INV-{today}-{:04}",
        count_today + 1
    ));
    ctx.input["status"] = serde_json::json!("draft");

    Ok(())
}

pub async fn enforce_invoice_workflow(ctx: &mut Context) -> ControllerResult {
    let invoice_id = ctx.input
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShaperailError::Internal("Missing invoice id".into()))?;

    let current_status: String = sqlx::query_scalar(
        "SELECT status FROM invoices WHERE id = $1::uuid"
    )
    .bind(invoice_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?
    .ok_or(ShaperailError::NotFound)?;

    let before_snapshot: serde_json::Value = sqlx::query_scalar(
        "SELECT row_to_json(i) FROM invoices i WHERE id = $1::uuid"
    )
    .bind(invoice_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    ctx.response_headers.push((
        "X-Audit-Before".into(),
        before_snapshot.to_string(),
    ));

    if current_status == "paid" || current_status == "void" {
        return Err(ShaperailError::Forbidden);
    }

    let Some(new_status) = ctx.input.get("status").and_then(|v| v.as_str()) else {
        return Ok(());
    };

    let role = ctx.user.as_ref().map(|u| u.role.as_str()).unwrap_or("anonymous");
    let allowed = matches!(
        (current_status.as_str(), new_status, role),
        ("draft", "pending", "finance" | "admin")
            | ("pending", "sent", "finance" | "admin")
            | ("sent", "paid", "finance" | "admin")
            | ("overdue", "paid", "finance" | "admin")
            | ("sent", "overdue", "finance" | "admin")
            | ("draft", "void", "admin")
            | ("pending", "void", "admin")
    );

    if !allowed {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "status".into(),
            message: format!("cannot transition from '{current_status}' to '{new_status}'"),
            code: "invalid_transition".into(),
        }]));
    }

    if new_status == "sent" {
        ctx.input["sent_at"] = serde_json::json!(chrono::Utc::now());
    }
    if new_status == "paid" {
        ctx.input["paid_at"] = serde_json::json!(chrono::Utc::now());
    }

    Ok(())
}

pub async fn audit_invoice_change(ctx: &mut Context) -> ControllerResult {
    let Some(data) = &ctx.data else {
        return Ok(());
    };

    let before_json = ctx.response_headers
        .iter()
        .find(|(k, _)| k == "X-Audit-Before")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| "null".to_string());
    ctx.response_headers.retain(|(k, _)| k != "X-Audit-Before");

    let user_id = ctx.user.as_ref()
        .map(|u| u.id.clone())
        .unwrap_or_else(|| "system".to_string());
    let ip_address = ctx.headers
        .get("x-forwarded-for")
        .or_else(|| ctx.headers.get("x-real-ip"))
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());

    if let Err(e) = sqlx::query(
        "INSERT INTO audit_logs (user_id, resource_type, resource_id, action, before_data, after_data, ip_address, created_at)
         VALUES ($1, 'invoices', $2, 'update', $3::jsonb, $4::jsonb, $5, NOW())"
    )
    .bind(&user_id)
    .bind(data["id"].as_str().unwrap_or("unknown"))
    .bind(&before_json)
    .bind(data.to_string())
    .bind(&ip_address)
    .execute(&ctx.pool)
    .await
    {
        tracing::error!("failed to insert audit log: {e}");
    }

    Ok(())
}
```

### Step 6: Implement the payment controllers

Payment logic validates business rules on create and keeps invoice status in
sync after create/update.

```rust
use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

pub async fn validate_payment(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
    ctx.input["created_by"] = serde_json::json!(user.id);

    if !ctx.input.contains_key("org_id") {
        if let Some(tenant_id) = &ctx.tenant_id {
            ctx.input["org_id"] = serde_json::json!(tenant_id);
        }
    }

    let invoice_id = ctx.input
        .get("invoice_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShaperailError::Validation(vec![FieldError {
            field: "invoice_id".into(),
            message: "invoice_id is required".into(),
            code: "required".into(),
        }]))?;
    let amount = ctx.input.get("amount_cents").and_then(|v| v.as_i64()).unwrap_or(0);

    let (invoice_status, invoice_total): (String, i64) = sqlx::query_as(
        "SELECT status, total_cents FROM invoices WHERE id = $1::uuid"
    )
    .bind(invoice_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?
    .ok_or(ShaperailError::NotFound)?;

    if invoice_status != "sent" && invoice_status != "overdue" {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "invoice_id".into(),
            message: "payments are allowed only for sent or overdue invoices".into(),
            code: "invoice_not_payable".into(),
        }]));
    }

    let already_recorded: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_cents), 0)
         FROM payments
         WHERE invoice_id = $1::uuid
           AND status IN ('pending', 'completed')"
    )
    .bind(invoice_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    if amount > invoice_total - already_recorded {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "amount_cents".into(),
            message: "payment exceeds invoice remaining balance".into(),
            code: "overpayment".into(),
        }]));
    }

    let duplicate: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1
            FROM payments
            WHERE invoice_id = $1::uuid
              AND amount_cents = $2
              AND created_at > NOW() - INTERVAL '5 minutes'
        )"
    )
    .bind(invoice_id)
    .bind(amount)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    if duplicate {
        return Err(ShaperailError::Conflict(
            "duplicate payment request detected".into(),
        ));
    }

    if ctx.input.get("status").and_then(|v| v.as_str()) == Some("completed") {
        ctx.input["completed_at"] = serde_json::json!(chrono::Utc::now());
    }

    Ok(())
}

pub async fn lock_payment_state(ctx: &mut Context) -> ControllerResult {
    let payment_id = ctx.input
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShaperailError::Internal("Missing payment id".into()))?;

    let current_status: String = sqlx::query_scalar(
        "SELECT status FROM payments WHERE id = $1::uuid"
    )
    .bind(payment_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?
    .ok_or(ShaperailError::NotFound)?;

    if current_status == "completed" || current_status == "refunded" {
        return Err(ShaperailError::Forbidden);
    }

    if ctx.input.get("status").and_then(|v| v.as_str()) == Some("completed") {
        ctx.input["completed_at"] = serde_json::json!(chrono::Utc::now());
    }

    Ok(())
}

pub async fn reconcile_invoice_status(ctx: &mut Context) -> ControllerResult {
    let Some(data) = &ctx.data else {
        return Ok(());
    };
    let payment_status = data["status"].as_str().unwrap_or("");
    if payment_status != "completed" {
        return Ok(());
    }

    let invoice_id = data["invoice_id"]
        .as_str()
        .ok_or_else(|| ShaperailError::Internal("payment record missing invoice_id".into()))?;

    let paid_total: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_cents), 0)
         FROM payments
         WHERE invoice_id = $1::uuid
           AND status = 'completed'"
    )
    .bind(invoice_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    let invoice_total: i64 = sqlx::query_scalar(
        "SELECT total_cents FROM invoices WHERE id = $1::uuid"
    )
    .bind(invoice_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(e.to_string()))?;

    if paid_total >= invoice_total {
        sqlx::query(
            "UPDATE invoices
             SET status = 'paid',
                 paid_at = COALESCE(paid_at, NOW())
             WHERE id = $1::uuid"
        )
        .bind(invoice_id)
        .execute(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(e.to_string()))?;
    }

    Ok(())
}
```

### Step 7: Add the manual audit log migration

`shaperail migrate` can generate the initial `customers`, `invoices`, and
`payments` create-table files, but the cross-cutting `audit_logs` table is a
manual SQL migration:

```sql
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE IF NOT EXISTS audit_logs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id TEXT NOT NULL,
  resource_type TEXT NOT NULL,
  resource_id TEXT NOT NULL,
  action TEXT NOT NULL,
  before_data JSONB NOT NULL DEFAULT 'null'::jsonb,
  after_data JSONB NOT NULL DEFAULT 'null'::jsonb,
  ip_address TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_resource
  ON audit_logs (resource_type, resource_id);

CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at
  ON audit_logs (created_at DESC);
```

Apply it with the normal migration flow:

```bash
shaperail migrate
```

### Step 8: Exercise the workflow

The repository already includes `examples/enterprise-saas/requests.http` with a
full request sequence. The critical path is:

1. Create a customer on `starter` or `pro`
2. Create a draft invoice for that customer
3. Move invoice `draft -> pending -> sent`
4. Create one or more payments
5. Mark a payment `completed`
6. Watch the invoice auto-transition to `paid` once completed payments cover `total_cents`

Good failure-path checks:

- create a `free` customer with non-zero credit limit
- skip from `free` straight to `pro`
- create an invoice for a suspended customer
- move an invoice from `sent` back to `draft`
- overpay an invoice
- modify a completed payment

### Step 9: Test the business rules

Use a mix of controller unit tests and HTTP integration tests.

Recommended unit tests:

- `validate_customer` rejects plan/credit mismatches
- `enforce_plan_change` blocks downgrade with outstanding invoices
- `prepare_invoice` generates `invoice_number` and rejects inactive customers
- `enforce_invoice_workflow` allows only valid transitions per role
- `validate_payment` blocks overpayment and duplicate requests
- `reconcile_invoice_status` marks the invoice as paid when the balance reaches zero

Recommended integration tests:

- finance user can move `pending -> sent`
- member cannot send or void an invoice
- creating two completed payments updates the invoice to `paid`
- tenant A cannot read tenant B's customers, invoices, or payments
- `audit_logs` rows are written after invoice updates

Use the same test patterns shown in [Testing]({{ '/testing/' | relative_url }})
for `Context` unit tests and `actix_web::test` endpoint tests.

### Step 10: Operating notes

- Keep customer, invoice, and payment controllers narrow. Each one should own a
  specific financial invariant.
- Put cross-resource bookkeeping in controllers only when it must happen in the
  request lifecycle. Move slower side effects to jobs.
- Treat invoice and payment state changes as contract-critical. Add explicit
  tests for every allowed and disallowed transition.
- Keep `audit_logs` append-only.
- For later schema edits, remember that follow-up SQL migrations are still
  manual today.

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

## What NOT to do in controllers

- **Do NOT spawn new Tokio tasks** — use `ctx.jobs` for background work
- **Do NOT catch and swallow errors silently** — always propagate or log
- **Do NOT read `ctx.data` in a before-controller** — it does not exist yet
- **Do NOT make slow HTTP calls without a timeout** — set timeouts on external requests
- **Do NOT write to tables without considering rollback** — if the main DB write
  fails after your controller's side-write, you have inconsistent state.
  For critical cases, use a transaction via `ctx.pool`.

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

# v0.3.0+
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
