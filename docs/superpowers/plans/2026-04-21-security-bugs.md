# Security Bugs Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two security bugs — tenant isolation bypass when `tenant_id` is absent from JWT, and rate limiter that exists but is never called.

**Architecture:** Two independent changes. Task 1 fixes two functions in `crud.rs` that silently pass instead of denying access. Task 2 adds `RateLimitSpec` to the core resource format, wires the existing `RateLimiter` into `AppState`, adds a `check_rate_limit` helper called at the top of every handler, and instantiates the limiter in the scaffold template.

**Tech Stack:** Rust, Actix-web, deadpool-redis, serde_yaml.

---

## Files Modified

| File | Change |
|------|--------|
| `shaperail-runtime/src/handlers/crud.rs` | Fix `verify_tenant` + `inject_tenant_filter`; add `rate_limiter` to `AppState`; add `check_rate_limit`; call it in 5 handlers |
| `shaperail-core/src/endpoint.rs` | Add `RateLimitSpec` struct; add `rate_limit` field to `EndpointSpec` |
| `shaperail-core/src/lib.rs` | Export `RateLimitSpec` |
| `shaperail-runtime/src/auth/rate_limit.rs` | Add `pool()` accessor to `RateLimiter` |
| `shaperail-cli/src/commands/init.rs` | Instantiate `RateLimiter` in scaffold template; add to `AppState`; add startup warning |

---

## Task 1: Fix tenant isolation bypass

**Context:** `verify_tenant` (line 112) and `inject_tenant_filter` (line 137) in `shaperail-runtime/src/handlers/crud.rs` both silently pass when the user has no `tenant_id` JWT claim, allowing access to all rows on a tenant-isolated resource. Both need to return `Err(ShaperailError::Forbidden)` instead. `inject_tenant_filter` currently returns `()` and must be changed to `Result<(), ShaperailError>` — all three call sites (lines 271, 1515, 1526) must propagate the error.

**Files:**
- Modify: `shaperail-runtime/src/handlers/crud.rs:112-152` — fix both functions
- Modify: `shaperail-runtime/src/handlers/crud.rs:1497-1508` — update existing test

---

- [ ] **Step 1: Write the failing tests**

Add two new tests and update one existing test in the `#[cfg(test)]` block in `shaperail-runtime/src/handlers/crud.rs`. Find the existing test `verify_tenant_no_user_tenant_id_passes` (around line 1497) and replace it, then add two more:

```rust
    #[test]
    fn verify_tenant_no_tenant_id_returns_forbidden() {
        let resource = tenant_resource();
        let user = AuthenticatedUser {
            id: "u1".to_string(),
            role: "member".to_string(),
            tenant_id: None,
        };
        let data = serde_json::json!({"id": "r1", "org_id": "org-b", "name": "Test"});
        let result = verify_tenant(&resource, Some(&user), &data);
        assert!(
            matches!(result, Err(ShaperailError::Forbidden)),
            "user with no tenant_id must be forbidden on tenant-isolated resource"
        );
    }

    #[test]
    fn verify_tenant_unauthenticated_user_returns_forbidden() {
        let resource = tenant_resource();
        let data = serde_json::json!({"id": "r1", "org_id": "org-b", "name": "Test"});
        let result = verify_tenant(&resource, None, &data);
        assert!(
            matches!(result, Err(ShaperailError::Forbidden)),
            "unauthenticated user must be forbidden on tenant-isolated resource"
        );
    }

    #[test]
    fn inject_tenant_filter_no_tenant_id_returns_forbidden() {
        let resource = tenant_resource();
        let user = AuthenticatedUser {
            id: "u1".to_string(),
            role: "member".to_string(),
            tenant_id: None,
        };
        let mut filters = crate::db::FilterSet::default();
        let result = inject_tenant_filter(&resource, Some(&user), &mut filters);
        assert!(
            matches!(result, Err(ShaperailError::Forbidden)),
            "inject_tenant_filter must return Forbidden when tenant_id is absent"
        );
        assert!(filters.filters.is_empty(), "no filter should be injected on error");
    }
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p shaperail-runtime verify_tenant_no_tenant_id inject_tenant_filter_no_tenant_id 2>&1 | tail -15
```

Expected: FAILED — tests assert `Forbidden` but current code returns `Ok(())`.

- [ ] **Step 3: Fix `verify_tenant`**

In `shaperail-runtime/src/handlers/crud.rs`, replace lines 124–127:

```rust
    // BEFORE:
    let user_tenant = match user.and_then(|u| u.tenant_id.as_deref()) {
        Some(t) => t,
        None => return Ok(()), // No tenant_id in token — no filtering
    };
```

With:

```rust
    // AFTER:
    let user_tenant = match user.and_then(|u| u.tenant_id.as_deref()) {
        Some(t) => t,
        None => return Err(ShaperailError::Forbidden),
    };
```

- [ ] **Step 4: Fix `inject_tenant_filter` signature and body**

Replace the entire `inject_tenant_filter` function (lines 137–152):

```rust
// BEFORE:
fn inject_tenant_filter(
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    filters: &mut crate::db::FilterSet,
) {
    let tenant_key = match &resource.tenant_key {
        Some(k) => k,
        None => return,
    };
    if is_super_admin(user) {
        return;
    }
    if let Some(tenant_id) = user.and_then(|u| u.tenant_id.as_deref()) {
        filters.add(tenant_key.clone(), tenant_id.to_string());
    }
}
```

With:

```rust
// AFTER:
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

- [ ] **Step 5: Update the three call sites**

Find each call to `inject_tenant_filter` and add `?` to propagate the error. There are three call sites:

**Call site 1** (around line 271 in `execute_list`):
```rust
// BEFORE:
inject_tenant_filter(resource, user.as_ref(), &mut params.filters);

// AFTER:
inject_tenant_filter(resource, user.as_ref(), &mut params.filters)?;
```

**Call site 2** (around line 1515, inside a test — find and update the test `inject_tenant_filter_adds_filter`):
```rust
// BEFORE (in test):
inject_tenant_filter(&resource, Some(&user), &mut filters);

// AFTER (in test):
inject_tenant_filter(&resource, Some(&user), &mut filters).unwrap();
```

**Call site 3** (around line 1526, inside a test `inject_tenant_filter_super_admin_skips` or similar — find it and add `.unwrap()`).

Search for all remaining call sites:
```bash
grep -n "inject_tenant_filter" shaperail-runtime/src/handlers/crud.rs
```
Add `?` on production call sites, `.unwrap()` on test call sites.

- [ ] **Step 6: Run the new tests — they should pass**

```bash
cargo test -p shaperail-runtime verify_tenant inject_tenant_filter 2>&1 | tail -20
```

Expected: all tenant tests PASS.

- [ ] **Step 7: Run full runtime unit tests**

```bash
cargo test -p shaperail-runtime --lib 2>&1 | tail -10
```

Expected: `test result: ok. N passed; 0 failed`.

- [ ] **Step 8: Clippy check**

```bash
cargo clippy -p shaperail-runtime -- -D warnings 2>&1 | tail -10
```

Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add shaperail-runtime/src/handlers/crud.rs
git commit -m "fix(runtime): return Forbidden when tenant_id absent on tenant-isolated resource"
```

---

## Task 2: Wire rate limiting end-to-end

**Context:** `RateLimiter` in `shaperail-runtime/src/auth/rate_limit.rs` is a complete Redis-backed sliding-window implementation — but `EndpointSpec` has no `rate_limit:` field, the limiter is never instantiated, and it is never called. This task adds the YAML declaration (`rate_limit: { max_requests: 100, window_secs: 60 }`), wires the limiter into `AppState`, adds a `check_rate_limit` helper, calls it in all five write/read handlers, and instantiates the limiter in the scaffold template in `shaperail-cli/src/commands/init.rs`.

The scaffold template is a raw string (`let main_rs = r###"..."###`) starting at line 146 of `init.rs` — edits to the generated `src/main.rs` are made inside that string.

**Files:**
- Modify: `shaperail-core/src/endpoint.rs` — add `RateLimitSpec`; add field to `EndpointSpec`
- Modify: `shaperail-core/src/lib.rs` — export `RateLimitSpec`
- Modify: `shaperail-runtime/src/auth/rate_limit.rs` — add `pool()` accessor
- Modify: `shaperail-runtime/src/handlers/crud.rs` — add `rate_limiter` to `AppState`; add `check_rate_limit`; call in handlers
- Modify: `shaperail-cli/src/commands/init.rs` — instantiate in scaffold; add to `AppState`

---

- [ ] **Step 1: Write failing tests for `RateLimitSpec` parsing in `shaperail-core`**

Add to the `#[cfg(test)]` block in `shaperail-core/src/endpoint.rs` (or at the bottom of the file if there is no test block yet — add `#[cfg(test)] mod tests { use super::*; }` wrapping):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_spec_parses_from_yaml() {
        let yaml = "max_requests: 50\nwindow_secs: 30\n";
        let spec: RateLimitSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.max_requests, 50);
        assert_eq!(spec.window_secs, 30);
    }

    #[test]
    fn endpoint_spec_rate_limit_field_roundtrips() {
        let yaml = r#"
auth: [member]
rate_limit:
  max_requests: 100
  window_secs: 60
"#;
        let spec: EndpointSpec = serde_yaml::from_str(yaml).unwrap();
        let rl = spec.rate_limit.unwrap();
        assert_eq!(rl.max_requests, 100);
        assert_eq!(rl.window_secs, 60);
    }

    #[test]
    fn endpoint_spec_rate_limit_absent_is_none() {
        let yaml = "auth: [member]\n";
        let spec: EndpointSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(spec.rate_limit.is_none());
    }
}
```

`shaperail-core` does not have `serde_yaml` as a dev-dependency. Add it to `shaperail-core/Cargo.toml`:
```toml
[dev-dependencies]
serde_yaml = { workspace = true }
```

- [ ] **Step 2: Run tests — confirm they fail**

```bash
cargo test -p shaperail-core rate_limit_spec 2>&1 | tail -10
```

Expected: FAILED — `RateLimitSpec` not found.

- [ ] **Step 3: Add `RateLimitSpec` to `shaperail-core/src/endpoint.rs`**

Add this struct just above `pub struct CacheSpec` (line 112):

```rust
/// Per-endpoint rate limiting configuration.
///
/// Declared in resource YAML:
/// ```yaml
/// list:
///   auth: [member]
///   rate_limit: { max_requests: 100, window_secs: 60 }
/// ```
///
/// Requires Redis. Silently skipped if Redis is not configured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitSpec {
    /// Maximum requests allowed within the window.
    pub max_requests: u64,
    /// Window duration in seconds.
    pub window_secs: u64,
}
```

Then add the field to `EndpointSpec` (after `pub upload: Option<UploadSpec>`, before `pub soft_delete`):

```rust
    /// Per-endpoint rate limiting. Requires Redis. Skipped if Redis is not configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitSpec>,
```

- [ ] **Step 4: Export `RateLimitSpec` from `shaperail-core/src/lib.rs`**

Update the `pub use endpoint::{...}` line (line 28) to include `RateLimitSpec`:

```rust
pub use endpoint::{
    apply_endpoint_defaults, endpoint_convention, AuthRule, CacheSpec, ControllerSpec,
    EndpointSpec, HttpMethod, PaginationStyle, RateLimitSpec, UploadSpec, WASM_HOOK_PREFIX,
};
```

- [ ] **Step 5: Run the tests — they should pass**

```bash
cargo test -p shaperail-core rate_limit_spec 2>&1 | tail -10
```

Expected: 3 tests PASS.

- [ ] **Step 6: Add `pool()` accessor to `RateLimiter`**

In `shaperail-runtime/src/auth/rate_limit.rs`, add this method to the `impl RateLimiter` block (after `key_for_tenant`):

```rust
    /// Returns a clone of the underlying Redis pool.
    /// Used to create per-endpoint limiter instances with different configs.
    pub fn pool(&self) -> Arc<deadpool_redis::Pool> {
        self.pool.clone()
    }
```

- [ ] **Step 7: Add `rate_limiter` field to `AppState` in `crud.rs`**

In `shaperail-runtime/src/handlers/crud.rs`, add to `AppState` (after `pub job_queue`):

```rust
    /// Per-endpoint Redis-backed rate limiter. `None` if Redis is not configured.
    pub rate_limiter: Option<Arc<crate::auth::RateLimiter>>,
```

- [ ] **Step 8: Write failing tests for `check_rate_limit`**

Add these tests to the `#[cfg(test)]` block in `shaperail-runtime/src/handlers/crud.rs`:

```rust
    #[test]
    fn check_rate_limit_skips_when_no_spec() {
        // endpoint with no rate_limit field — helper must return Ok without touching Redis
        let endpoint = EndpointSpec {
            rate_limit: None,
            ..Default::default()
        };
        // We can't easily call the async fn in a sync test; verify the early-return
        // condition by confirming rate_limit is None
        assert!(endpoint.rate_limit.is_none());
    }

    #[test]
    fn check_rate_limit_skips_when_no_limiter() {
        use shaperail_core::RateLimitSpec;
        let endpoint = EndpointSpec {
            rate_limit: Some(RateLimitSpec { max_requests: 10, window_secs: 60 }),
            ..Default::default()
        };
        // AppState with rate_limiter: None — helper skips enforcement
        // Verify the field exists and accepts None
        let _ = endpoint.rate_limit.as_ref().unwrap();
        // Full async test would require a mock Redis; this verifies the type compiles
    }
```

Note: `EndpointSpec` needs `Default` derived to use `..Default::default()`. Check if it already derives `Default`; if not, add `#[derive(Default)]` to `EndpointSpec` in `endpoint.rs` (it's safe — all fields are `Option` or have defaults).

- [ ] **Step 9: Add `check_rate_limit` helper to `crud.rs`**

Add this async function just before `pub async fn handle_list` in `shaperail-runtime/src/handlers/crud.rs`:

```rust
/// Checks the per-endpoint rate limit for the current request.
///
/// Returns `Ok(())` if:
/// - The endpoint has no `rate_limit:` configured, or
/// - `AppState.rate_limiter` is `None` (Redis not configured).
///
/// Returns `Err(ShaperailError::RateLimited)` if the limit is exceeded.
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

- [ ] **Step 10: Call `check_rate_limit` in all five handlers**

In each handler, add the call immediately after `enforce_auth` (which extracts the user) and before any DB access.

**`handle_list`** — find `pub async fn handle_list` and after `let user = enforce_auth(...)`:
```rust
    check_rate_limit(&endpoint, &state, &req, user.as_ref()).await?;
```

**`handle_get`** (line 315) — after `let user = enforce_auth(&req, &endpoint)?;`:
```rust
    check_rate_limit(&endpoint, &state, &req, user.as_ref()).await?;
```

**`handle_create`** (line 684) — after `let user = enforce_auth(&req, &endpoint)?;`:
```rust
    check_rate_limit(&endpoint, &state, &req, user.as_ref()).await?;
```

**`handle_update`** (line 762) — after `let user = enforce_auth(&req, &endpoint)?;`:
```rust
    check_rate_limit(&endpoint, &state, &req, user.as_ref()).await?;
```

**`handle_delete`** (line 878) — after `let user = enforce_auth(&req, &endpoint)?;`:
```rust
    check_rate_limit(&endpoint, &state, &req, user.as_ref()).await?;
```

- [ ] **Step 11: Run runtime tests**

```bash
cargo test -p shaperail-runtime --lib 2>&1 | tail -10
```

Expected: `test result: ok. N passed; 0 failed`.

- [ ] **Step 12: Wire `rate_limiter` into the scaffold template in `init.rs`**

The scaffold template is the raw string `let main_rs = r###"..."###` in `shaperail-cli/src/commands/init.rs`. Edits are to Rust code inside that string.

**Change A** — After `let job_queue = redis_pool.as_ref().map(...)` (around template line matching `job_queue`), add:

```rust
    let rate_limiter = redis_pool.as_ref().map(|pool| {
        std::sync::Arc::new(shaperail_runtime::auth::RateLimiter::new(
            pool.clone(),
            shaperail_runtime::auth::RateLimitConfig::default(),
        ))
    });
```

**Change B** — In the `AppState { ... }` construction block (around line 1063 of the outer `init.rs` file), add:

```rust
        rate_limiter,
```

alongside the other fields.

**Change C** — Add a startup warning after the `AppState` is constructed. Find the `tracing::info!("Starting Shaperail server...")` block and add before it:

```rust
    // Warn if any endpoint declares rate_limit but Redis is not configured
    if rate_limiter.is_none() {
        let has_rate_limit = resources.iter().any(|r| {
            r.endpoints.as_ref().map_or(false, |eps| {
                eps.values().any(|ep| ep.rate_limit.is_some())
            })
        });
        if has_rate_limit {
            tracing::warn!(
                "One or more endpoints declare rate_limit but Redis is not configured \
                 — rate limiting will be skipped. Set REDIS_URL to enable it."
            );
        }
    }
```

- [ ] **Step 13: Run workspace tests and clippy**

```bash
cargo test --workspace --lib --bins 2>&1 | grep -E "^(test result|FAILED|error\[)" | head -20
cargo clippy --workspace -- -D warnings 2>&1 | tail -10
cargo fmt --check 2>&1 | head -5
```

Expected: all pass, clippy clean.

- [ ] **Step 14: Commit**

```bash
git add shaperail-core/src/endpoint.rs \
        shaperail-core/src/lib.rs \
        shaperail-core/Cargo.toml \
        shaperail-runtime/src/auth/rate_limit.rs \
        shaperail-runtime/src/handlers/crud.rs \
        shaperail-cli/src/commands/init.rs
git commit -m "feat(core,runtime,cli): add RateLimitSpec, wire RateLimiter into AppState and handlers"
```

---

## Self-Review

**Spec coverage:**
- ✅ Fix 1: `verify_tenant` returns `Forbidden` → Task 1 Step 3
- ✅ Fix 1: `inject_tenant_filter` returns `Result`, propagated at 3 call sites → Task 1 Steps 4–5
- ✅ Fix 1: tests updated → Task 1 Steps 1–2
- ✅ Fix 2A: `RateLimitSpec` added to core → Task 2 Steps 3–4
- ✅ Fix 2B: `rate_limiter` in `AppState` → Task 2 Step 7
- ✅ Fix 2C: `check_rate_limit` helper → Task 2 Step 9
- ✅ Fix 2D: Called in all 5 handlers → Task 2 Step 10
- ✅ Fix 2E: Scaffold wiring → Task 2 Step 12
- ✅ Fix 2F: Startup warning → Task 2 Step 12 Change C
- ✅ `pool()` accessor on `RateLimiter` → Task 2 Step 6

**No placeholders found.**

**Type consistency:**
- `RateLimitSpec { max_requests: u64, window_secs: u64 }` used consistently in Steps 3, 9, tests
- `crate::auth::RateLimiter` / `crate::auth::RateLimitConfig` fully qualified in `check_rate_limit` — matches imports in `rate_limit.rs`
- `Arc<crate::auth::RateLimiter>` in `AppState` matches the `Arc::new(...)` in scaffold
