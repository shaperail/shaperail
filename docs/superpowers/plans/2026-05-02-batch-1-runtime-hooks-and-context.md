# Batch 1 — Runtime Hooks & Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the four hook/context gaps community users hit: validate-and-reject `controller:` declared on custom (non-CRUD) endpoints; preserve `Context` across before/after hooks; ship `response_extras` and `session` fields for first-class one-time fields and cross-phase scratch; ship `Subject` helpers for tenant-isolated custom handlers.

**Architecture:** Five small additions plus one validator rule. (1) New validator rule rejects `controller:` on custom endpoints. (2) `run_before_controller` returns the full `Context` and `run_after_controller` takes ownership of it — same struct instance threads from before to after. (3) `Context` gains `response_extras` (merged into the response `data:` envelope) and `session` (cross-phase scratch). (4) New `Subject` type in `shaperail_runtime::auth` with role/tenant accessors and `sqlx::QueryBuilder` integration; CRUD's existing tenant logic delegates to it. (5) Lifecycle docs on `Context` and a new `agent_docs/custom-handlers.md`.

**Tech Stack:** Rust 2021, actix-web, sqlx (`QueryBuilder<Postgres>`), `serde_json::Map<String, Value>`.

**Spec:** `docs/superpowers/specs/2026-05-02-batch-1-runtime-hooks-and-context-design.md`

**Branch:** `fix/init-template-cleanup` (continuing — to be renamed before push)

---

## File Structure

| File | Responsibility | Action |
|------|----------------|--------|
| `shaperail-codegen/src/validator.rs` (or wherever endpoint validation lives) | Reject `controller:` on custom endpoints | Modify |
| `shaperail-runtime/src/handlers/controller.rs` | `Context` definition + dispatch | Modify — add `session`, `response_extras`; expand rustdoc |
| `shaperail-runtime/src/handlers/crud.rs` | CRUD pipeline | Modify — refactor before/after to thread one `Context`; merge `response_extras` into response |
| `shaperail-runtime/src/auth/subject.rs` | New `Subject` type | Create |
| `shaperail-runtime/src/auth/mod.rs` | Auth re-exports | Modify — re-export `Subject` |
| `shaperail-runtime/src/handlers/crud.rs` (tenant helpers) | Tenant-isolation primitives | Modify — delegate to `Subject` where it makes sense |
| `agent_docs/custom-handlers.md` | Custom-handler doc | Create |
| `agent_docs/hooks-system.md` | Hook lifecycle doc | Modify |
| `CHANGELOG.md` | Changelog | Modify |

---

## Task 1: Validate-and-reject `controller:` on custom endpoints (#1)

This is the cheapest change in the batch and surfaces a clear failure path for the migration. Do it first.

**Files:**
- Modify: `shaperail-codegen/src/validator.rs`

- [ ] **Step 1.1: Locate endpoint validation**

In `shaperail-codegen/src/validator.rs` (or whatever module owns `validate_resource`), find the endpoint-validation function. There may be multiple validation functions iterated over each endpoint — pattern is similar to existing checks (e.g., the `transient` rules from #67).

If endpoint validation lives in a different file, adapt: search `shaperail-codegen/src` for a function that iterates `resource.endpoints` and emits `ValidationError`s.

- [ ] **Step 1.2: Add the rule**

Add a new validation function:

```rust
fn validate_controller_only_on_crud(
    resource: &str,
    action: &str,
    endpoint: &EndpointSpec,
) -> Option<ValidationError> {
    const CRUD_ACTIONS: &[&str] = &[
        "list", "get", "create", "update", "delete",
        "bulk_create", "bulk_delete",
    ];
    if !CRUD_ACTIONS.contains(&action) && endpoint.controller.is_some() {
        return Some(ValidationError::CustomEndpointWithController {
            resource: resource.to_string(),
            action: action.to_string(),
        });
    }
    None
}
```

The exact `ValidationError` variant name and structure depends on the existing enum in `shaperail-codegen` — adapt to whatever pattern is established (search for `ValidationError::` to see the conventions).

- [ ] **Step 1.3: Add the error variant + Display impl**

Wherever `ValidationError` is defined, add:

```rust
#[error("endpoint `{action}` on resource `{resource}` declares `controller:` but is a custom endpoint (dispatched via `handler:`). `controller` is only valid on conventional CRUD endpoints (list / get / create / update / delete / bulk_create / bulk_delete). For shared logic on custom handlers, use the typed `Subject` and tenant helpers from `shaperail_runtime::auth` directly inside your handler.")]
CustomEndpointWithController {
    resource: String,
    action: String,
},
```

(Adjust the `#[error(...)]` macro form to match the existing variants — likely `thiserror`. If the file uses manual `Display` impls, do the same.)

- [ ] **Step 1.4: Wire it into `validate_resource`**

Find the function that returns `Vec<ValidationError>` for a `ResourceDefinition` (probably `validate_resource`). Add the new check inside the per-endpoint loop:

```rust
for (action, endpoint) in resource.endpoints.iter().flatten() {
    if let Some(err) = validate_controller_only_on_crud(&resource.resource, action, endpoint) {
        errors.push(err);
    }
    // ... existing checks ...
}
```

(Adapt to the existing iteration pattern.)

- [ ] **Step 1.5: Test**

Add to `shaperail-codegen/src/validator.rs` (or `tests/`):

```rust
#[test]
fn reject_controller_on_custom_endpoint() {
    let yaml = r#"
resource: agents
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  regenerate_secret:
    method: POST
    path: /agents/:id/regenerate_secret
    auth: [admin]
    controller: { before: my_before }
"#;
    let resource = shaperail_codegen::parser::parse_resource_str(yaml).unwrap();
    let errors = validate_resource(&resource);
    assert!(
        errors.iter().any(|e| matches!(e, ValidationError::CustomEndpointWithController { .. })),
        "expected a CustomEndpointWithController error, got: {errors:?}"
    );
}

#[test]
fn allow_controller_on_crud_endpoints() {
    let yaml = r#"
resource: agents
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  create:
    auth: [admin]
    input: [name]
    controller: { before: my_before }
"#;
    let resource = shaperail_codegen::parser::parse_resource_str(yaml).unwrap();
    let errors = validate_resource(&resource);
    assert!(
        !errors.iter().any(|e| matches!(e, ValidationError::CustomEndpointWithController { .. })),
        "create endpoint with controller should NOT trip the rule, got: {errors:?}"
    );
}
```

(Adapt `parser::parse_resource_str` to the actual YAML-string parser entry point.)

- [ ] **Step 1.6: Verify**

```
cargo test -p shaperail-codegen validator::
cargo build --workspace
```

If existing fixtures somewhere in the codebase declare `controller:` on a custom endpoint (an oversight that previously did nothing), the new rule will reject them. Update those fixtures: either remove `controller:` from the custom endpoint, or move the logic to a CRUD endpoint.

Run a wider test pass to surface affected fixtures: `cargo test --workspace`.

- [ ] **Step 1.7: Commit**

```
git add shaperail-codegen/src/validator.rs shaperail-codegen/src/parser.rs shaperail-codegen/tests resources
git commit -m "$(cat <<'EOF'
feat(codegen)!: reject controller: on custom endpoints

A controller: { before/after: ... } declaration on a non-CRUD
(custom) endpoint was silently ignored at runtime — the dispatch
went through state.custom_handlers, which are built from handler:
only. Now shaperail check rejects the config with a clear error
pointing the user at the Subject helpers in shaperail_runtime::auth
for shared logic in custom handlers.

Migration: remove controller: from any custom endpoint that
declares it. The old behavior was a no-op; the new behavior is a
loud failure that prevents the same mistake.

Refs #1.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add `Context.session` and `Context.response_extras` (#2, #11)

**Files:**
- Modify: `shaperail-runtime/src/handlers/controller.rs`

- [ ] **Step 2.1: Add the fields**

In `shaperail-runtime/src/handlers/controller.rs`, in the `pub struct Context { ... }` definition (around line 28), append two fields:

```rust
    /// Cross-phase scratch space. Anything written here in a `before:` controller
    /// is visible in the matching `after:` controller for the same request. Never
    /// persisted to the database, never sent to the client.
    pub session: serde_json::Map<String, serde_json::Value>,

    /// Fields to inject into the JSON response body without persisting them.
    ///
    /// Merged into the response under the `data:` envelope key after the after-hook
    /// returns. Useful for one-time values (minted secrets, server-computed URLs,
    /// signed download tokens). Keys here will **shadow** any same-named field on
    /// the persisted record.
    pub response_extras: serde_json::Map<String, serde_json::Value>,
```

- [ ] **Step 2.2: Initialize at every construction site**

Search for `Context {` and `super::controller::Context {` across `shaperail-runtime/src/`. For each `Context { ... }` literal, add the two new fields initialized to `serde_json::Map::new()`.

Known sites to update (verify via grep):

- `shaperail-runtime/src/handlers/crud.rs:705` (before-hook context).
- `shaperail-runtime/src/handlers/crud.rs:755` (after-hook context — but Task 3 will refactor this away, so leave a TODO if it's awkward).
- `shaperail-runtime/src/handlers/controller.rs` test module (multiple `Context { ... }` constructions in `#[tokio::test]` bodies).

- [ ] **Step 2.3: Expand the `Context` rustdoc**

Replace the existing `pub struct Context` doc comment (around lines 12–27) with the lifecycle-aware version from the spec:

```rust
/// Context passed to controller functions for synchronous in-request business logic.
///
/// # Lifecycle
///
/// One `Context` is constructed per CRUD request and **survives both phases**:
///
/// 1. `before:` controller — `data` is `None`. May read/mutate `input`, `session`,
///    `response_extras`, `response_headers`, `tenant_id`.
/// 2. CRUD operation runs. The runtime sets `data` to the persisted record.
/// 3. `after:` controller — `data` is `Some(record)`. May read everything,
///    mutate `data`, `session`, `response_extras`, `response_headers`.
///
/// Anything written to `session` in `before:` is visible in `after:`. Anything
/// written to `response_extras` in either phase is merged into the JSON response
/// body (under the `data:` envelope key) but **never persisted**.
///
/// `input` is **not** reset between phases — by `after:` it reflects what the
/// before-hook wrote, but it is no longer authoritative for the persisted record.
///
/// # Example: minting a one-time secret
///
/// ```rust,ignore
/// async fn mint_mcp_secret(ctx: &mut Context) -> ControllerResult {
///     if ctx.data.is_none() {
///         // before-phase
///         let plaintext = generate_random_secret_32_bytes();
///         let hash = hash_secret(&plaintext);
///         ctx.input.insert("mcp_secret_hash".into(), serde_json::json!(hash));
///         ctx.session.insert("plaintext".into(), serde_json::json!(plaintext));
///     } else {
///         // after-phase
///         if let Some(plaintext) = ctx.session.remove("plaintext") {
///             ctx.response_extras.insert("mcp_secret".into(), plaintext);
///         }
///     }
///     Ok(())
/// }
/// ```
```

- [ ] **Step 2.4: Verify build**

```
cargo build -p shaperail-runtime
```

Expected: success. If any callsite of `Context { ... }` is missed, the compiler will say so — add the field there too.

- [ ] **Step 2.5: Commit**

```
git add shaperail-runtime/src/handlers/controller.rs shaperail-runtime/src/handlers/crud.rs
git commit -m "$(cat <<'EOF'
feat(runtime): add Context.session and Context.response_extras

session is a cross-phase scratch space — anything written in
before: is visible in after:, never persisted, never serialized
to the client. response_extras is merged into the response body's
data: envelope after the after-hook returns; useful for one-time
fields like minted plaintext secrets that must reach the client
exactly once but never hit the database.

The Context lifecycle rustdoc is expanded to make it clear that
both phases share the same Context — Task 3 wires the runtime to
actually preserve it across phases.

Refs #2, #11.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Preserve `Context` across before/after, merge `response_extras` (#11, #2)

**Files:**
- Modify: `shaperail-runtime/src/handlers/crud.rs`

- [ ] **Step 3.1: Refactor `run_before_controller`**

In `shaperail-runtime/src/handlers/crud.rs`, change `run_before_controller`'s signature so it returns the full `Context` instead of just the input map.

Old (around line 683):

```rust
async fn run_before_controller(
    state: &AppState,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    input: serde_json::Map<String, serde_json::Value>,
    user: Option<&AuthenticatedUser>,
    req: &HttpRequest,
) -> Result<serde_json::Map<String, serde_json::Value>, ShaperailError> {
    // ...
    Ok(ctx.input)
}
```

New:

```rust
async fn run_before_controller(
    state: &AppState,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    input: serde_json::Map<String, serde_json::Value>,
    user: Option<&AuthenticatedUser>,
    req: &HttpRequest,
) -> Result<super::controller::Context, ShaperailError> {
    // ... build ctx as before, including session/response_extras: Map::new() ...
    // If no before-controller declared, RETURN the freshly-built ctx anyway.
    // (This way the caller has one consistent code path.)
    // ... if controller declared, dispatch as before, then return ctx ...
    Ok(ctx)
}
```

The early-return path (when no before-controller is declared) currently returns `Ok(input)`. Change it to:

```rust
    let name = match endpoint.controller.as_ref().and_then(|c| c.before.as_deref()) {
        Some(n) => n,
        None => {
            // No before-controller — still build a Context so the after-hook (if any)
            // sees consistent session/response_extras state. data is None at this point.
            return Ok(super::controller::Context {
                input,
                data: None,
                user: user.cloned(),
                pool: state.pool.clone(),
                headers: build_headers_map(req),
                response_headers: vec![],
                tenant_id: resolve_tenant_id(resource, user),
                session: serde_json::Map::new(),
                response_extras: serde_json::Map::new(),
            });
        }
    };
```

(Extract the headers-map construction into a small helper if it's duplicated; the existing code already inlines it.)

- [ ] **Step 3.2: Refactor `run_after_controller`**

Change signature to take ownership of the `Context` from before-phase, write the persisted `data`, and run the after-hook (if any).

Old (around line 733):

```rust
async fn run_after_controller(
    state: &AppState,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    data: serde_json::Value,
    user: Option<&AuthenticatedUser>,
    req: &HttpRequest,
) -> Result<serde_json::Value, ShaperailError> {
    // ... rebuilds ctx from scratch ...
}
```

New:

```rust
async fn run_after_controller(
    state: &AppState,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    mut ctx: super::controller::Context,
    persisted: serde_json::Value,
) -> Result<super::controller::Context, ShaperailError> {
    ctx.data = Some(persisted);
    let Some(name) = endpoint.controller.as_ref().and_then(|c| c.after.as_deref()) else {
        return Ok(ctx);
    };
    #[cfg(feature = "wasm-plugins")]
    let wasm_rt = state.wasm_runtime.as_ref();
    #[cfg(not(feature = "wasm-plugins"))]
    let wasm_rt = None;
    super::controller::dispatch_controller(
        name,
        &resource.resource,
        &mut ctx,
        state.controllers.as_ref(),
        wasm_rt,
    )
    .await?;
    Ok(ctx)
}
```

- [ ] **Step 3.3: Update callers in `handle_create`, `handle_update`, `handle_get`, etc.**

Find every call to `run_before_controller` and `run_after_controller` in `crud.rs` and update them to thread the `Context` through:

Old pattern:

```rust
let input = run_before_controller(state, resource, endpoint, input, user, &req).await?;
// ... DB op produces `persisted` ...
let data = run_after_controller(state, resource, endpoint, persisted, user, &req).await?;
return response::created(data);
```

New pattern:

```rust
let mut ctx = run_before_controller(state, resource, endpoint, input, user, &req).await?;
// ... DB op uses `ctx.input`, produces `persisted` ...
let ctx = run_after_controller(state, resource, endpoint, ctx, persisted).await?;
return build_response_with_extras(ctx, response::created);
```

Where `build_response_with_extras` is a new helper (Step 3.4).

- [ ] **Step 3.4: Add `build_response_with_extras` helper**

Add to `crud.rs` (near the other response helpers):

```rust
/// Merges `ctx.response_extras` into `ctx.data` (when both are objects) and
/// hands the resulting JSON to the given response builder. Logs a warning if
/// extras are set but `data` is not a JSON object (e.g., a list endpoint).
fn build_response_with_extras<F>(ctx: super::controller::Context, builder: F) -> actix_web::HttpResponse
where
    F: FnOnce(serde_json::Value) -> actix_web::HttpResponse,
{
    let mut data = ctx.data.unwrap_or(serde_json::Value::Null);
    if !ctx.response_extras.is_empty() {
        if let Some(obj) = data.as_object_mut() {
            for (k, v) in ctx.response_extras {
                obj.insert(k, v);
            }
        } else {
            tracing::warn!(
                "response_extras set but record data is not a JSON object; dropping extras"
            );
        }
    }
    let mut response = builder(data);
    for (name, value) in ctx.response_headers {
        if let (Ok(name), Ok(value)) = (
            actix_web::http::header::HeaderName::from_bytes(name.as_bytes()),
            actix_web::http::header::HeaderValue::from_str(&value),
        ) {
            response.headers_mut().insert(name, value);
        }
    }
    response
}
```

(Existing code may already merge `ctx.response_headers` somewhere — find that and leave it where it is, or move the headers-merge into the new helper. Don't double-apply.)

- [ ] **Step 3.5: Tests for `session` and `response_extras`**

Add to whatever integration-style test file already exercises the controller pipeline (likely `shaperail-runtime/src/handlers/crud.rs` `#[cfg(test)] mod tests` or a `tests/` integration). If those tests need a live Postgres, gate the new tests behind the same `DATABASE_URL` check.

Tests to add:

```rust
#[tokio::test]
async fn session_round_trips_before_to_after() {
    // Register a before-controller that does ctx.session.insert("scratch", json!("hello")).
    // Register an after-controller that reads ctx.session.get("scratch") and asserts == "hello".
    // Drive a create request; assert the after-controller did not panic.
}

#[tokio::test]
async fn response_extras_appear_in_response_body() {
    // Register an after-controller that does ctx.response_extras.insert("plaintext_secret", json!("xyz")).
    // Drive a create request; deserialize the response body; assert .data.plaintext_secret == "xyz".
    // Re-fetch the row from DB; assert no plaintext_secret column exists / value is null.
}
```

If creating end-to-end tests is too heavy, at minimum add unit-level assertions that the new helpers behave correctly:

```rust
#[test]
fn build_response_with_extras_merges_into_object() {
    let mut ctx = make_test_ctx();
    ctx.data = Some(serde_json::json!({"id": "abc", "name": "Alice"}));
    ctx.response_extras.insert("token".into(), serde_json::json!("xyz"));
    let resp = build_response_with_extras(ctx, response::created);
    // Read body bytes, assert `{"data": {"id": "abc", "name": "Alice", "token": "xyz"}}`.
}
```

(`make_test_ctx` is whatever helper builds a `Context` for unit tests — match the surrounding patterns.)

- [ ] **Step 3.6: Verify**

```
cargo build -p shaperail-runtime
cargo test -p shaperail-runtime handlers::
cargo clippy -p shaperail-runtime --all-targets --all-features -- -D warnings
```

All three must succeed.

- [ ] **Step 3.7: Commit**

```
git add shaperail-runtime/src/handlers/crud.rs
git commit -m "$(cat <<'EOF'
feat(runtime): preserve Context across before/after, merge response_extras

run_before_controller now returns the full Context (not just
input). run_after_controller takes ownership of it, sets data to
the persisted record, runs the after-hook if declared, and returns
it. Callers thread the same Context through both phases — session
state set in before: is visible in after:.

build_response_with_extras merges ctx.response_extras into the
response data: object after the after-hook returns. Extras are
never persisted; if data is not a JSON object (e.g., list response),
extras are dropped with a warn line.

Closes #11. Closes #2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Add `Subject` API for tenant-scoped custom handlers (#3)

**Files:**
- Create: `shaperail-runtime/src/auth/subject.rs`
- Modify: `shaperail-runtime/src/auth/mod.rs`

- [ ] **Step 4.1: Write the `Subject` module**

Create `shaperail-runtime/src/auth/subject.rs`:

```rust
//! Authenticated subject — role + tenant accessors for custom handlers.

use actix_web::HttpRequest;
use shaperail_core::ShaperailError;

use super::extractor::AuthenticatedUser;

/// The authenticated subject of a request, with role and tenant accessors.
///
/// Use this in custom handlers as the authoritative source of "who is calling
/// and what tenant are they in." Wraps `AuthenticatedUser` with helpers that
/// match the tenant-isolation logic the CRUD path applies automatically.
///
/// # Example
///
/// ```rust,ignore
/// use shaperail_runtime::auth::Subject;
/// use sqlx::QueryBuilder;
///
/// pub async fn regenerate_secret(req: actix_web::HttpRequest, /* state... */) -> actix_web::HttpResponse {
///     let subject = match Subject::from_request(&req) {
///         Ok(s) => s,
///         Err(_) => return actix_web::HttpResponse::Unauthorized().finish(),
///     };
///     let mut q = QueryBuilder::<sqlx::Postgres>::new("UPDATE agents SET mcp_secret_hash = ");
///     q.push_bind(/* new_hash */ "");
///     q.push(" WHERE id = ");
///     q.push_bind(/* agent_id */ uuid::Uuid::nil());
///     subject.scope_to_tenant(&mut q, "org_id").unwrap();
///     // execute q ...
///     actix_web::HttpResponse::Ok().finish()
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Subject {
    pub id: String,
    pub role: String,
    pub tenant_id: Option<String>,
}

impl Subject {
    /// Extracts the subject from an authenticated request. Returns
    /// `Err(ShaperailError::Unauthorized)` if no valid JWT/API key is present.
    pub fn from_request(req: &HttpRequest) -> Result<Self, ShaperailError> {
        let user = super::extractor::try_extract_auth(req).ok_or(ShaperailError::Unauthorized)?;
        Ok(Self::from(&user))
    }

    /// True for the global `super_admin` role, which is exempt from tenant isolation.
    pub fn is_super_admin(&self) -> bool {
        self.role == "super_admin"
    }

    /// The tenant filter to apply to queries.
    ///
    /// - `Ok(None)` for `super_admin` (full visibility).
    /// - `Ok(Some(tenant))` for a normal user with a `tenant_id` claim.
    /// - `Err(Unauthorized)` for a non-`super_admin` subject whose JWT carries
    ///   no `tenant_id` claim — that is a configuration error, not a silent
    ///   "no filter" pass.
    pub fn tenant_filter(&self) -> Result<Option<&str>, ShaperailError> {
        if self.is_super_admin() {
            return Ok(None);
        }
        match self.tenant_id.as_deref() {
            Some(t) if !t.is_empty() => Ok(Some(t)),
            _ => Err(ShaperailError::Unauthorized),
        }
    }

    /// Asserts that a record's tenant column matches this subject's tenant.
    ///
    /// - `Ok(())` for `super_admin` (no check applied).
    /// - `Ok(())` for a normal user whose tenant matches `record_tenant_id`.
    /// - `Err(Forbidden)` for a normal user whose tenant does NOT match.
    /// - `Err(Unauthorized)` for a normal user with no `tenant_id` claim.
    pub fn assert_tenant_match(&self, record_tenant_id: &str) -> Result<(), ShaperailError> {
        match self.tenant_filter()? {
            None => Ok(()),
            Some(t) if t == record_tenant_id => Ok(()),
            Some(_) => Err(ShaperailError::Forbidden),
        }
    }

    /// Appends a tenant filter to a sqlx `QueryBuilder` for tenant-scoped queries.
    /// No-op for `super_admin`.
    ///
    /// Pushes `" AND <column> = "` followed by a bound `tenant_id`. Caller is
    /// responsible for the surrounding query shape.
    pub fn scope_to_tenant<'q>(
        &self,
        builder: &mut sqlx::QueryBuilder<'q, sqlx::Postgres>,
        column: &str,
    ) -> Result<(), ShaperailError> {
        let Some(tenant) = self.tenant_filter()? else {
            return Ok(());
        };
        builder.push(" AND ");
        builder.push(column);
        builder.push(" = ");
        builder.push_bind(tenant.to_string());
        Ok(())
    }
}

impl From<&AuthenticatedUser> for Subject {
    fn from(user: &AuthenticatedUser) -> Self {
        Self {
            id: user.id.clone(),
            role: user.role.clone(),
            tenant_id: user.tenant_id.clone(),
        }
    }
}

impl From<AuthenticatedUser> for Subject {
    fn from(user: AuthenticatedUser) -> Self {
        Self::from(&user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn super_admin() -> Subject {
        Subject {
            id: "u1".into(),
            role: "super_admin".into(),
            tenant_id: None,
        }
    }

    fn member(tenant: &str) -> Subject {
        Subject {
            id: "u2".into(),
            role: "member".into(),
            tenant_id: Some(tenant.into()),
        }
    }

    fn member_no_tenant() -> Subject {
        Subject {
            id: "u3".into(),
            role: "member".into(),
            tenant_id: None,
        }
    }

    #[test]
    fn super_admin_tenant_filter_is_none() {
        assert!(super_admin().tenant_filter().unwrap().is_none());
    }

    #[test]
    fn member_tenant_filter_is_their_tenant() {
        let s = member("org-1");
        assert_eq!(s.tenant_filter().unwrap(), Some("org-1"));
    }

    #[test]
    fn member_without_tenant_is_unauthorized() {
        let s = member_no_tenant();
        assert!(matches!(
            s.tenant_filter(),
            Err(ShaperailError::Unauthorized)
        ));
    }

    #[test]
    fn assert_tenant_match_super_admin_skips_check() {
        super_admin().assert_tenant_match("any").unwrap();
    }

    #[test]
    fn assert_tenant_match_mismatch_is_forbidden() {
        let s = member("org-1");
        assert!(matches!(
            s.assert_tenant_match("org-2"),
            Err(ShaperailError::Forbidden)
        ));
    }

    #[test]
    fn assert_tenant_match_match_ok() {
        member("org-1").assert_tenant_match("org-1").unwrap();
    }

    #[test]
    fn scope_to_tenant_super_admin_is_noop() {
        let mut b = sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT 1");
        super_admin().scope_to_tenant(&mut b, "org_id").unwrap();
        assert_eq!(b.sql(), "SELECT 1");
    }

    #[test]
    fn scope_to_tenant_member_appends_filter() {
        let mut b = sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT 1");
        member("org-1").scope_to_tenant(&mut b, "org_id").unwrap();
        // sqlx renders the bind placeholder as $1, $2, ...; assert structure.
        let sql = b.sql();
        assert!(sql.starts_with("SELECT 1 AND "));
        assert!(sql.contains("org_id = $1"));
    }
}
```

- [ ] **Step 4.2: Re-export from `auth/mod.rs`**

In `shaperail-runtime/src/auth/mod.rs`, add:

```rust
pub mod subject;
pub use subject::Subject;
```

(Add the `pub mod subject;` next to the existing `pub mod` declarations and `pub use subject::Subject;` next to the existing `pub use`s.)

- [ ] **Step 4.3: Verify**

```
cargo build -p shaperail-runtime
cargo test -p shaperail-runtime auth::subject::
```

Expected: all 8 unit tests pass.

- [ ] **Step 4.4: Use `Subject` inside the existing CRUD tenant logic**

Find `enforce_tenant_isolation` (around `crud.rs:144`) and `inject_tenant_filter` (around `:170`). Refactor to use `Subject` accessors so there is one source of truth:

```rust
fn enforce_tenant_isolation(
    resource: &ResourceDefinition,
    data: &serde_json::Value,
    user: Option<&AuthenticatedUser>,
) -> Result<(), ShaperailError> {
    let Some(tenant_key) = resource.tenant_key.as_deref() else { return Ok(()); };
    let Some(user) = user else { return Err(ShaperailError::Forbidden); };
    let subject = super::auth::Subject::from(user);
    let record_tenant = data.get(tenant_key).and_then(|v| v.as_str()).unwrap_or("");
    subject.assert_tenant_match(record_tenant)
}
```

(Adapt names to whatever `crud.rs` actually has. The point is: where existing code branches on role and tenant_id, delegate to `Subject` instead. If the refactor balloons in scope, leave the existing logic in place and JUST surface `Subject` as a new public type — that satisfies the issue.)

- [ ] **Step 4.5: Verify all CRUD tenant tests still pass**

```
cargo test -p shaperail-runtime
```

If existing tenant tests fail, the refactor changed semantics — revert just the refactor and keep `Subject` as the new public type for custom handlers, leaving the CRUD path on the old code. (This is acceptable per the spec's "if the refactor balloons" caveat.)

- [ ] **Step 4.6: Commit**

```
git add shaperail-runtime/src/auth/subject.rs shaperail-runtime/src/auth/mod.rs shaperail-runtime/src/handlers/crud.rs
git commit -m "$(cat <<'EOF'
feat(runtime): add Subject API for tenant-scoped custom handlers

shaperail_runtime::auth::Subject is the authoritative "who is
calling and what tenant" type for custom handlers. It exposes:

- is_super_admin()
- tenant_filter() — Ok(None) for super_admin, Ok(Some(t)) for
  member, Err(Unauthorized) for member with no tenant_id claim.
- assert_tenant_match(record_tenant) — for post-fetch checks.
- scope_to_tenant(query_builder, column) — appends "AND col = $N"
  to a sqlx::QueryBuilder<Postgres>, no-op for super_admin.

CRUD endpoints continue to apply tenant isolation automatically
via the same primitives. Custom handlers must opt in by extracting
Subject from the request and calling the helpers explicitly — the
framework cannot infer a custom handler's data flow.

Closes #3.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Documentation

**Files:**
- Create: `agent_docs/custom-handlers.md`
- Modify: `agent_docs/hooks-system.md`

- [ ] **Step 5.1: Create custom-handlers doc**

Create `agent_docs/custom-handlers.md`:

```markdown
# Custom Handlers

A custom endpoint declares `handler:` (and optionally `method:` / `path:`) instead of using one of the conventional CRUD actions (list / get / create / update / delete / bulk_create / bulk_delete). Custom handlers own their own request parsing AND response generation — the framework gives you the route binding and authentication, but the rest is your code.

## What custom handlers do NOT get for free

Unlike CRUD endpoints, custom handlers do **not** get:

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
    let new_hash: &str = "<the hash>"; // computed earlier
    let mut q = QueryBuilder::<Postgres>::new("UPDATE agents SET mcp_secret_hash = ");
    q.push_bind(new_hash);
    q.push(" WHERE id = ");
    q.push_bind(agent_id);
    if let Err(_) = subject.scope_to_tenant(&mut q, "org_id") {
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

- Is a no-op for `super_admin` (no filter applied — full visibility).
- Appends `" AND <column> = $N"` with the bound `tenant_id` for any other role.
- Returns `Err(Unauthorized)` for a non-`super_admin` subject whose JWT carries no `tenant_id` claim. That case is a config error and must fail loudly.

For post-fetch checks (read-then-validate flows), use `assert_tenant_match(record_tenant_id)` instead.

## Sharing logic across custom handlers

There is no controller pipeline for custom endpoints. Share logic the normal Rust way: extract a helper function in `resources/<name>.handlers.rs` and call it from each handler. The framework's job is to give you `Subject`; your job is to use it.

## What if I want CRUD-style hooks?

Use a CRUD endpoint and the `controller: { before, after }` declaration. The two-phase pipeline plus `Context.session` and `Context.response_extras` cover most "I need to mint a one-time value" cases without a custom handler — see `agent_docs/hooks-system.md`.
```

- [ ] **Step 5.2: Update hooks-system.md**

In `agent_docs/hooks-system.md`, find the section that describes the `before:` / `after:` lifecycle and update it to reflect the preserved-Context behavior. Add (or replace existing equivalent text with):

```markdown
## Lifecycle: before → DB → after

```text
request ─▶ [before-hook(ctx)] ─▶ [DB op fills ctx.data] ─▶ [after-hook(ctx)] ─▶ response
                     │                                              │
                     └── ctx.session, ctx.response_extras shared ──┘
```

The `Context` is **the same struct instance** in both phases. Anything written to `session` in `before:` is visible in `after:`. `response_extras` is merged into the response's `data:` envelope after `after:` returns and before serialization — never persisted, never re-readable, perfect for one-time secrets.
```

(Insert above or below the existing lifecycle prose; do not duplicate. Match the file's existing style.)

- [ ] **Step 5.3: Commit**

```
git add agent_docs/custom-handlers.md agent_docs/hooks-system.md
git commit -m "$(cat <<'EOF'
docs: custom handlers + updated hook lifecycle

- agent_docs/custom-handlers.md (new): explains what custom
  endpoints don't inherit from CRUD (no auto-tenant-scoping, no
  hook pipeline) and shows the Subject-based pattern for
  authenticating and scoping queries explicitly.
- agent_docs/hooks-system.md: lifecycle diagram updated to make
  the preserved-Context behavior explicit (session and
  response_extras shared across before/after).

Refs #1, #2, #3, #11.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: CHANGELOG and final quality gate

- [ ] **Step 6.1: CHANGELOG**

In `CHANGELOG.md`, under the existing `## [Unreleased]` section:

Under `### Breaking`:

```markdown
- **`controller:` declared on a non-CRUD (custom) endpoint is now rejected** at validation time (`shaperail check`). The old behavior was a silent no-op — the runtime dispatched custom endpoints via `handler:` only and never invoked the declared controllers. Move shared logic into the custom handler itself; use `shaperail_runtime::auth::Subject` for auth/tenant scoping (#1).
```

Under `### Added`:

```markdown
- **`Context.response_extras`** — `serde_json::Map<String, Value>` field on `ControllerContext`. Merged into the response body's `data:` envelope after the after-hook returns; never persisted. Perfect for one-time fields like minted plaintext secrets that must reach the client exactly once (#2).
- **`Context.session`** — cross-phase scratch space on `ControllerContext`. Anything written in `before:` is visible in `after:` for the same request. Never persisted, never serialized to the client (#11).
- **`shaperail_runtime::auth::Subject`** — typed wrapper around the authenticated user with role/tenant accessors and `sqlx::QueryBuilder<Postgres>` integration. Use in custom handlers for explicit tenant scoping; CRUD endpoints continue to apply scoping automatically (#3).
```

Under `### Changed`:

```markdown
- **`Context` is preserved across `before:` and `after:` hooks** for the same request. Previously the runtime constructed a new `Context` for each phase, so state set in `before:` was not visible in `after:`. Now both phases share the same struct instance (#11). `input` is still authoritative for the persisted record only at the moment the DB op runs.
```

- [ ] **Step 6.2: Final gate**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --no-fail-fast --features test-support
```

All three must succeed (modulo the 44 pre-existing DB-required failures).

- [ ] **Step 6.3: Commit**

```
git add CHANGELOG.md
git commit -m "$(cat <<'EOF'
docs(changelog): note batch-1 (runtime hooks/context)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Acceptance Checklist

- [ ] A YAML with `controller: { before: foo }` on a custom endpoint causes `shaperail check` to fail with an error naming the endpoint and resource.
- [ ] `Context.session.insert(...)` in `before:` is readable from `Context.session` in `after:`.
- [ ] `Context.response_extras.insert(...)` produces a response whose `data:` object contains the key, while the persisted DB row does not.
- [ ] `shaperail_runtime::auth::Subject::scope_to_tenant(&mut qb, "org_id")` appends `AND org_id = $N` for non-super-admin subjects and is a no-op for super_admin.
- [ ] `cargo test --workspace` passes (modulo pre-existing DB-required failures).
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean.
- [ ] `cargo doc -p shaperail-runtime` shows `Context` with the new lifecycle docs and `Subject` with full method docs.
