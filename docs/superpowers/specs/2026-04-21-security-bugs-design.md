# Security Bugs Fix — Design Spec
**Date:** 2026-04-21
**Status:** Approved
**Scope:** Fix two security bugs: tenant isolation bypass when `tenant_id` is absent from JWT, and rate limiter that is implemented but never called.

---

## Problem

### Bug 1 — Tenant isolation bypass
`verify_tenant` and `inject_tenant_filter` in `shaperail-runtime/src/handlers/crud.rs` both silently pass when the authenticated user has no `tenant_id` claim in their JWT. A resource that declares `tenant_key: org_id` should be fully isolated — a user without a `tenant_id` claim is not in any tenant and must be denied access. The current `None => return Ok(())` path lets them read or mutate all rows.

### Bug 2 — Rate limiter never called
`RateLimiter` in `shaperail-runtime/src/auth/rate_limit.rs` is a complete Redis-backed sliding-window implementation, but:
- `EndpointSpec` has no `rate_limit:` field — there is no YAML syntax to declare it
- `RateLimiter` is never instantiated or called anywhere in handlers or middleware
- The feature effectively does not exist at runtime

---

## Out of Scope

- Global server-level rate limiting (belongs in a reverse proxy)
- Rate limiting for WebSocket or gRPC endpoints
- Rate limit headers (`X-RateLimit-Remaining`, `Retry-After`) in responses
- Changing the `RateLimiter` algorithm (the existing sliding window is correct)

---

## Fix 1 — Tenant isolation enforcement

**File:** `shaperail-runtime/src/handlers/crud.rs`

### `verify_tenant` (line 112)

Change the `None` arm for missing `tenant_id` from `return Ok(())` to `return Err(ShaperailError::Forbidden)`:

```rust
let user_tenant = match user.and_then(|u| u.tenant_id.as_deref()) {
    Some(t) => t,
    None => return Err(ShaperailError::Forbidden), // no tenant claim → deny
};
```

### `inject_tenant_filter` (line 137)

Change the return type from `()` to `Result<(), ShaperailError>` and return `Err(ShaperailError::Forbidden)` when `tenant_id` is absent:

```rust
fn inject_tenant_filter(
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    filters: &mut crate::db::FilterSet,
) -> Result<(), ShaperailError> {
    let tenant_key = match &resource.tenant_key {
        Some(k) => k,
        None => return Ok(()),
    };
    if is_super_admin(user) {
        return Ok(());
    }
    match user.and_then(|u| u.tenant_id.as_deref()) {
        Some(tenant_id) => {
            filters.add(tenant_key.clone(), tenant_id.to_string());
            Ok(())
        }
        None => Err(ShaperailError::Forbidden),
    }
}
```

Update every call site (lines 271, 1515, 1526) to propagate the error with `?`.

### Tests to add

```rust
#[test]
fn verify_tenant_no_tenant_id_returns_forbidden() { ... }

#[test]
fn inject_tenant_filter_no_tenant_id_returns_forbidden() { ... }
```

Update the existing test `verify_tenant_no_user_tenant_id_passes` to `verify_tenant_no_user_tenant_id_forbidden` asserting `Err(ShaperailError::Forbidden)`.

---

## Fix 2 — Wire rate limiting end-to-end

### Step A — Add `RateLimitSpec` to core

**File:** `shaperail-core/src/endpoint.rs`

Add a new type:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitSpec {
    /// Maximum requests allowed in the window.
    pub max_requests: u64,
    /// Window duration in seconds.
    pub window_secs: u64,
}
```

Add field to `EndpointSpec`:

```rust
/// Per-endpoint rate limiting. Requires Redis. Skipped if Redis is not configured.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub rate_limit: Option<RateLimitSpec>,
```

YAML syntax (same pattern as `cache:`):
```yaml
list:
  auth: [member]
  rate_limit: { max_requests: 100, window_secs: 60 }
```

### Step B — Add `rate_limiter` to `AppState`

**File:** `shaperail-runtime/src/handlers/crud.rs`

Add field to `AppState`:

```rust
pub rate_limiter: Option<Arc<crate::auth::RateLimiter>>,
```

### Step C — Add `check_rate_limit` helper

**File:** `shaperail-runtime/src/handlers/crud.rs`

```rust
async fn check_rate_limit(
    endpoint: &EndpointSpec,
    state: &AppState,
    req: &HttpRequest,
    user: Option<&AuthenticatedUser>,
) -> Result<(), ShaperailError> {
    let Some(ref spec) = endpoint.rate_limit else {
        return Ok(());
    };
    let Some(ref limiter) = state.rate_limiter else {
        // Redis not configured — skip enforcement, warn once at startup
        return Ok(());
    };
    let ip = req
        .connection_info()
        .peer_addr()
        .unwrap_or("unknown")
        .to_string();
    let user_id = user.map(|u| u.id.as_str());
    let tenant_id = user.and_then(|u| u.tenant_id.as_deref());
    let key = crate::auth::RateLimiter::key_for_tenant(&ip, user_id, tenant_id);

    // Override the limiter's config with the endpoint-level spec
    let endpoint_limiter = crate::auth::RateLimiter::new(
        limiter.pool(),
        crate::auth::RateLimitConfig {
            max_requests: spec.max_requests,
            window_secs: spec.window_secs,
        },
    );
    endpoint_limiter.check(&key).await.map(|_| ())
}
```

This requires adding a `pool()` accessor to `RateLimiter`:
```rust
pub fn pool(&self) -> Arc<deadpool_redis::Pool> {
    self.pool.clone()
}
```

### Step D — Call `check_rate_limit` in handlers

At the top of each handler (`handle_list`, `handle_get`, `handle_create`, `handle_update`, `handle_delete`), after auth and before any DB work:

```rust
check_rate_limit(endpoint, &state, &req, user.as_ref()).await?;
```

### Step E — Wire `rate_limiter` into scaffold

**File:** `shaperail-cli/src/commands/init.rs` (scaffold template)

After the Redis pool is set up, instantiate the limiter:

```rust
let rate_limiter = redis_pool.as_ref().map(|pool| {
    Arc::new(shaperail_runtime::auth::RateLimiter::new(
        pool.clone(),
        shaperail_runtime::auth::RateLimitConfig::default(),
    ))
});
```

Add to `AppState` construction:
```rust
rate_limiter,
```

### Step F — Warn at startup when rate_limit declared but Redis absent

In the scaffold template, after building routes, log a warning if any endpoint declares `rate_limit:` but `redis_pool` is `None`. This surfaces the misconfiguration early.

### Tests to add

```rust
#[test]
fn rate_limit_spec_parses_from_yaml() { ... }

#[test]
fn endpoint_spec_with_rate_limit_roundtrips() { ... }

// In crud.rs:
#[test]
fn check_rate_limit_skips_when_no_spec() { ... }

#[test]
fn check_rate_limit_skips_when_no_limiter() { ... }
```

---

## Files Changed

| File | Change |
|------|--------|
| `shaperail-core/src/endpoint.rs` | Add `RateLimitSpec`; add `rate_limit` field to `EndpointSpec` |
| `shaperail-runtime/src/handlers/crud.rs` | Fix `verify_tenant` and `inject_tenant_filter`; add `rate_limiter` to `AppState`; add `check_rate_limit` helper; call it in all handlers |
| `shaperail-runtime/src/auth/rate_limit.rs` | Add `pool()` accessor to `RateLimiter` |
| `shaperail-cli/src/commands/init.rs` | Instantiate `RateLimiter` in scaffold; add to `AppState`; add startup warning |

---

## Testing

| Test | Location |
|------|----------|
| `verify_tenant_no_tenant_id_forbidden` | `crud.rs` tests |
| `inject_tenant_filter_no_tenant_id_forbidden` | `crud.rs` tests |
| `rate_limit_spec_parses_from_yaml` | `shaperail-core` tests |
| `check_rate_limit_skips_when_no_spec` | `crud.rs` tests |
| `check_rate_limit_skips_when_no_limiter` | `crud.rs` tests |
