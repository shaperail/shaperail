# Batch 1 — Runtime Hooks & Context

**Date:** 2026-05-02
**Issues:** #1 (custom endpoints silently ignore `controller:`), #2 (no `response_extras` for one-time fields), #3 (custom handlers must hand-roll tenant scoping), #11 (`Context` lifecycle between before/after is undocumented)
**Risk:** Medium. Touches the CRUD hook plumbing and adds public API. Not breaking — additive on `Context` and on `auth` module, plus a new validation error for a previously silently-broken config.

## Goal

Close the four hook/context gaps that conspire to make non-trivial controllers painful:

1. Make config validation loud about hooks declared in places they will never run.
2. Give controllers a first-class way to surface one-time fields (e.g., minted secrets) in the response.
3. Give custom handlers the same tenant-isolation guarantees that CRUD endpoints get for free.
4. Stop discarding state between before and after hooks — and document the lifecycle so users do not have to read `crud.rs` to understand it.

## Non-Goals

- Refactoring custom handlers to share the CRUD `Context` shape (different request/response model, different design).
- Replacing `serde_json::Map<String, Value>` on `Context` with a typed builder.
- Auto-generating a Subject from custom-handler signatures via macros. The Subject is opt-in; the handler asks for it explicitly.

## Change A — Reject `controller:` on custom endpoints (#1)

### A1 — Validation rule

**File:** `shaperail-codegen/src/validator.rs` (or wherever endpoint-spec validation lives — locate via `grep "EndpointSpec" shaperail-codegen/src`).

Add a rule that runs over every `(action, EndpointSpec)` pair after `apply_endpoint_defaults`:

```rust
fn validate_controller_only_on_crud(
    resource: &str,
    action: &str,
    endpoint: &EndpointSpec,
) -> Result<(), ValidationError> {
    const CRUD_ACTIONS: &[&str] = &[
        "list", "get", "create", "update", "delete",
        "bulk_create", "bulk_delete",
    ];
    if !CRUD_ACTIONS.contains(&action) && endpoint.controller.is_some() {
        return Err(ValidationError::custom_endpoint_with_controller(
            resource, action,
        ));
    }
    Ok(())
}
```

Error message text (verbatim, surfaced by `shaperail check`):

```
error[E0312]: endpoint `<action>` on resource `<resource>` declares `controller:`
  but is a custom endpoint (dispatched via `handler:`).
  
  `controller` is only valid on conventional CRUD endpoints
  (list / get / create / update / delete / bulk_create / bulk_delete).
  
  For shared logic on custom handlers, use the typed `Subject` and tenant
  helpers from `shaperail_runtime::auth` directly inside your handler.
  See: https://shaperail.io/docs/custom-handlers#sharing-logic
```

### A2 — Why reject rather than support

Custom handlers own request parsing and response generation; their lifecycle does not match the `input -> DB -> data` shape that `Context` is built around. Two options were considered and rejected:

- **"Limited" before-hook on custom endpoints** — populate `Context.input` with the parsed body, leave `data` as `None`. This creates two flavors of `Context` (CRUD-shaped and custom-shaped) that look identical at the type level but behave differently, violating "ONE WAY". Authors would have to remember which fields are populated when.
- **Wrap custom handlers with hooks transparently** — same problem plus a new lifecycle question (does the after-hook see the response body? what if the handler streamed?).

Loud rejection plus the `Subject` helpers from Change C is the AI-First answer: one canonical way to share logic, immediately surfaced when the user takes the wrong path.

### A3 — Tests

- Unit test in `shaperail-codegen/src/validator.rs`: feed in a fixture with a custom endpoint declaring `controller:`, assert validation fails with the new error code.
- Unit test: same fixture without `controller:` validates clean.
- Update `shaperail-codegen/tests/golden_*` snapshots if any include the now-rejected pattern.

## Change B — Preserve `Context` across before and after (#11)

### B1 — Refactor hook dispatch

**File:** `shaperail-runtime/src/handlers/crud.rs` (~lines 678–777)

Today:

- `run_before_controller(...) -> Result<serde_json::Map<...>, _>` — discards everything except `input` on return.
- `run_after_controller(...)` builds a brand-new `Context` with `input: serde_json::Map::new()` and `data: Some(persisted)`.

Refactor signatures to thread the `Context` through:

```rust
async fn run_before_controller(
    state: &AppState,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    input: serde_json::Map<String, serde_json::Value>,
    user: Option<&AuthenticatedUser>,
    req: &HttpRequest,
) -> Result<Context, ShaperailError>;

async fn run_after_controller(
    state: &AppState,
    resource: &ResourceDefinition,
    endpoint: &EndpointSpec,
    mut ctx: Context,
    data: serde_json::Value,
) -> Result<Context, ShaperailError>;
```

`handle_create`, `handle_update`, etc. now keep ownership of the `Context` between phases:

```rust
let mut ctx = run_before_controller(...).await?;
let persisted = state.store.insert(&ctx.input).await?;
let ctx = run_after_controller(state, resource, endpoint, ctx, persisted).await?;
```

If no before-controller is declared, `run_before_controller` still returns a fresh `Context` (with `data: None` and `session`/`response_extras` empty) so the call sites do not branch.

### B2 — Document the new lifecycle on `Context`

**File:** `shaperail-runtime/src/handlers/controller.rs`

Replace the existing rustdoc on `pub struct Context` with:

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
///         ctx.input.insert("mcp_secret_hash".into(), json!(hash));
///         ctx.session.insert("plaintext".into(), json!(plaintext));
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

## Change C — `response_extras` (#2)

### C1 — Add the field

**File:** `shaperail-runtime/src/handlers/controller.rs`

Add to `Context`:

```rust
/// Fields to inject into the JSON response body without persisting them.
///
/// Merged into the response under the `data:` envelope key after the after-hook
/// returns. Useful for one-time values like minted secrets, server-computed URLs,
/// or anything else the client should see exactly once.
///
/// Keys here will **shadow** any same-named field on the persisted record.
pub response_extras: serde_json::Map<String, serde_json::Value>,
```

Initialize to `serde_json::Map::new()` everywhere `Context` is constructed.

### C2 — Merge at response build time

**File:** `shaperail-runtime/src/handlers/crud.rs`

After `run_after_controller` returns, in every CRUD handler that returns a single record (`handle_create`, `handle_update`, `handle_get`):

```rust
let mut data = ctx.data.unwrap_or(serde_json::Value::Null);
if !ctx.response_extras.is_empty() {
    if let Some(obj) = data.as_object_mut() {
        for (k, v) in std::mem::take(&mut ctx.response_extras) {
            obj.insert(k, v);
        }
    } else {
        // Non-object data and response_extras present → log a warn, drop extras.
        tracing::warn!(
            resource = %resource.resource,
            "response_extras set but record data is not a JSON object; dropping extras"
        );
    }
}
return response::created(data);  // or single(data) etc.
```

For list endpoints, `response_extras` is currently undefined behavior — there is no per-record context. Document that `response_extras` is ignored for `list` and `bulk_*` endpoints, and either (a) emit a warn at request time or (b) skip silently. Pick (a) for consistency with the AI-First "loud failure" rule, but log at `warn`, not error.

### C3 — Tests

- `crud_create_response_extras_are_merged` — register a controller that sets `response_extras["mcp_secret"]`, hit create, assert the response JSON contains both the persisted columns and `mcp_secret`.
- `crud_create_response_extras_shadow_record_field` — extras key matches a column; assert the extras value wins.
- `crud_create_response_extras_not_persisted` — after the request, fetch the row from DB directly, assert `mcp_secret` is **not** stored.
- `crud_list_response_extras_logs_warn` — set extras on a list endpoint, assert a warn line is recorded (using `tracing-test`).

## Change D — `Subject` API for tenant scoping (#3)

### D1 — New public type

**File:** `shaperail-runtime/src/auth/subject.rs` (new), re-exported from `auth/mod.rs`.

```rust
/// The authenticated subject of a request, with role and tenant accessors.
///
/// Use this in custom handlers as the authoritative source of "who is calling
/// and what tenant are they in." It wraps `AuthenticatedUser` with helpers that
/// match the tenant-isolation logic the CRUD path applies automatically.
#[derive(Debug, Clone)]
pub struct Subject {
    pub id: String,
    pub role: String,
    pub tenant_id: Option<String>,
}

impl Subject {
    /// Extracts the subject from an authenticated request. Returns `Err(Unauthorized)`
    /// if there is no valid JWT/API key.
    pub fn from_request(req: &HttpRequest) -> Result<Self, ShaperailError> { ... }

    /// Returns true for the global `super_admin` role, which is exempt from
    /// tenant isolation.
    pub fn is_super_admin(&self) -> bool {
        self.role == "super_admin"
    }

    /// Returns the tenant filter to apply on queries: `Some(tenant_id)` for normal
    /// users, `None` for `super_admin` (no filter — full visibility).
    ///
    /// Returns `Err(Unauthorized)` for non-super-admin subjects whose JWT did not
    /// carry a `tenant_id` claim — that case is a configuration error and must be
    /// rejected loudly, not silently scoped.
    pub fn tenant_filter(&self) -> Result<Option<&str>, ShaperailError> { ... }

    /// Asserts that a record's tenant column matches this subject's tenant.
    /// Returns `Ok(())` for super_admin (no check), `Err(Forbidden)` otherwise.
    pub fn assert_tenant_match(&self, record_tenant_id: &str) -> Result<(), ShaperailError> { ... }

    /// Appends a tenant filter to a sqlx `QueryBuilder` for tenant-scoped queries.
    ///
    /// No-op for super_admin. For non-super-admin, appends `AND <column> = $N`
    /// with the bound tenant_id.
    pub fn scope_to_tenant<'q>(
        &self,
        builder: &mut sqlx::QueryBuilder<'q, sqlx::Postgres>,
        column: &str,
    ) -> Result<(), ShaperailError> { ... }
}
```

### D2 — Reuse the CRUD tenant logic

The functions at `shaperail-runtime/src/handlers/crud.rs:170-200` (`enforce_tenant_isolation`, `inject_tenant_filter`, `auto_inject_tenant_on_create`) already implement the canonical rules. Refactor those to delegate to `Subject` methods so there is one source of truth:

```rust
fn enforce_tenant_isolation(resource, data, user) -> Result<(), _> {
    let Some(tenant_key) = &resource.tenant_key else { return Ok(()); };
    let Some(user) = user else { return Err(Forbidden); };
    let subject = Subject::from(user);
    if let Some(record_tenant) = data.get(tenant_key).and_then(|v| v.as_str()) {
        subject.assert_tenant_match(record_tenant)
    } else {
        // record has no tenant column populated — preserve current behavior
        Ok(())
    }
}
```

Add an `impl From<&AuthenticatedUser> for Subject` so the conversion is one line.

### D3 — Document the custom-handler responsibility

**File:** `agent_docs/custom-handlers.md` (new) and `docs/custom-handlers.md` (new user-facing).

Section: "Tenant isolation — custom handlers must opt in". Code example:

```rust
pub async fn regenerate_secret(
    req: HttpRequest,
    state: Arc<AppState>,
    resource: Arc<ResourceDefinition>,
    endpoint: Arc<EndpointSpec>,
) -> HttpResponse {
    let subject = match Subject::from_request(&req) {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };

    let agent_id = match parse_id_from_path(&req) {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };

    let mut q = sqlx::QueryBuilder::new(
        "UPDATE agents SET mcp_secret_hash = ",
    );
    q.push_bind(new_hash);
    q.push(" WHERE id = ");
    q.push_bind(agent_id);
    if let Err(e) = subject.scope_to_tenant(&mut q, "org_id") {
        return e.into_response();
    }
    // ... execute, build response
}
```

The doc opens with: "Custom handlers do not get automatic tenant isolation. You **must** call `Subject::scope_to_tenant` (or `assert_tenant_match` after a fetch) on every query — the framework cannot infer your data flow. CRUD endpoints handle this for you because they own the query construction; custom endpoints own theirs."

### D4 — Tests

- `subject_from_request_extracts_jwt_user` — happy path.
- `subject_tenant_filter_super_admin_returns_none` — super_admin gets full visibility.
- `subject_tenant_filter_normal_user_returns_tenant` — normal user gets their tenant.
- `subject_tenant_filter_missing_tenant_id_errors` — non-super-admin without `tenant_id` claim → Unauthorized.
- `subject_assert_tenant_match_mismatch_returns_forbidden`.
- `subject_scope_to_tenant_appends_filter` — query builder integration test using `sqlx::QueryBuilder::sql()`.
- `subject_scope_to_tenant_super_admin_no_op` — super_admin builder unchanged.

## Change E — Add `session` field for cross-phase scratch (#11)

### E1 — Add the field

**File:** `shaperail-runtime/src/handlers/controller.rs`

```rust
/// Cross-phase scratch space. Anything written here in a `before:` controller
/// is visible in the matching `after:` controller for the same request. Never
/// persisted, never sent to the client.
pub session: serde_json::Map<String, serde_json::Value>,
```

Initialize to empty everywhere. The lifecycle change in Change B is what actually preserves it across phases — without B, `session` would be reset.

### E2 — Tests

- `session_round_trips_before_to_after` — set in before, assert visible in after.
- `session_does_not_appear_in_response` — round-trip through the request, fetch the response, assert no `session` key.
- `session_does_not_persist_to_db` — same, query the DB row, assert nothing leaked.

## Documentation

- `agent_docs/hooks-system.md` — rewrite the "Lifecycle" section to match the new behavior. Diagram:
  ```
  request ─▶ [before-hook(ctx)] ─▶ [DB op fills ctx.data] ─▶ [after-hook(ctx)] ─▶ response
                       │                                              │
                       └── ctx.session, ctx.response_extras shared ──┘
  ```
- `agent_docs/custom-handlers.md` (new) — Subject API, tenant scoping rules.
- `docs/custom-handlers.md` (new, user-facing) — same content shaped for README/site.
- `docs/recipes/one-time-secret.md` (new) — full mcp_secret example using `session` + `response_extras`.
- `CHANGELOG.md` under `[Unreleased]`:
  - **Added:** `Context.response_extras` for one-time response fields (#2).
  - **Added:** `Context.session` for cross-phase controller state (#11).
  - **Added:** `shaperail_runtime::auth::Subject` for tenant-scoped custom handlers (#3).
  - **Changed:** `Context` is now preserved across before/after hooks. `input`, `session`, `response_extras`, and `response_headers` written in `before:` are visible/respected in `after:` (#11).
  - **Changed (breaking):** `controller:` declared on a non-CRUD (custom) endpoint is now rejected at validation time. Move shared logic into the custom handler using `Subject` helpers (#1).

## Acceptance Criteria

1. A YAML with `controller: { before: foo }` on a custom endpoint causes `shaperail check` to fail with error code `E0312` and a message naming the endpoint and resource.
2. A controller that does `ctx.session.insert("k", json!("v"))` in before-phase can read `ctx.session.get("k")` in after-phase.
3. A controller that does `ctx.response_extras.insert("plaintext_secret", json!("…"))` in after-phase produces a response whose `data:` object contains `plaintext_secret`, while the persisted DB row does not.
4. A custom handler using `Subject::scope_to_tenant(&mut qb, "org_id")` issues a query that includes `AND org_id = $N` for non-super-admin subjects and is unchanged for super-admin.
5. `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` pass.
6. `cargo doc -p shaperail-runtime --open` shows `Context` with the new lifecycle docs and `Subject` with full method docs.

## Rollout

Single PR. The validation rejection is a breaking change for projects whose YAML currently declares `controller:` on a custom endpoint — those declarations were no-ops, so no behavior is lost, but the build now fails. Document this prominently in the v0.11 release notes alongside the migration: "remove `controller:` from custom endpoints; use `Subject` directly in your handler."

## Follow-ups (out of scope here)

- Async hook variants that take ownership of the response body (would change the `Context` shape — separate design).
- Macro-based `Subject` extractor (`async fn handler(req: HttpRequest, subject: Subject)`) — Actix `FromRequest` integration.
- Move the tenant-key inference into a richer typed RBAC layer.
