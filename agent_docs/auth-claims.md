# JWT Claims and Tenant Isolation

Shaperail mints HS256 JWTs with the following claim shape:

| Claim        | Required for | Notes |
|--------------|--------------|-------|
| `sub`        | always       | User ID (UUID). |
| `role`       | always       | Must match a role in any endpoint's `auth:` list (or be `super_admin` for unrestricted access). |
| `iat` / `exp`| always       | Unix seconds. |
| `token_type` | always       | `"access"` for protected requests; `"refresh"` is valid only against the refresh endpoint. |
| `tenant_id`  | non-`super_admin` accessing tenant-scoped resources | Missing/null → 401. |

The canonical struct is `shaperail_runtime::auth::Claims`, re-exported from the auth module so consumers minting tokens for tests can use it directly.

## Minting a test token

```rust
use shaperail_runtime::auth::JwtConfig;

let config = JwtConfig::new("test-secret-at-least-32-bytes-long!", 3600, 86400);
let token = config
    .encode_access_with_tenant(
        "00000000-0000-0000-0000-000000000001",
        "admin",
        Some("org-1"),
    )
    .unwrap();
// Send as `Authorization: Bearer {token}`.
```

For requests that should not be tied to a tenant (e.g., `super_admin` audit ops), pass `None` for `tenant_id`:

```rust
let token = config.encode_access("00000000-0000-0000-0000-000000000001", "super_admin")?;
```

## Diagnosing 401s

When a request fails authentication, the runtime emits a structured `tracing::warn!` line **before** returning 401 — set `RUST_LOG=shaperail_runtime::auth=warn` (or higher verbosity) to surface them in dev:

| Log message | Meaning |
|-------------|---------|
| `JWT rejected: decode failed` | Signature mismatch, expired, malformed, or wrong algorithm. The `error` field carries the underlying `jsonwebtoken` error. |
| `JWT rejected: token_type must be "access"` | The token decoded but its `token_type` claim is not `"access"` — typically a refresh token sent against a protected endpoint. The `token_type` and `sub` fields identify the rejected token. |

The 401 response body is unchanged across reasons; the audit signal is in the log.

## Tenant claim semantics

For a non-`super_admin` subject hitting a resource that declares `tenant_key:`, the runtime requires `tenant_id` to be present and uses it as the canonical tenant filter. CRUD endpoints inject the filter automatically; custom handlers must extract `Subject` and apply it explicitly (see `agent_docs/custom-handlers.md` once Batch 1 lands).

A `super_admin` subject is exempt from tenant filtering — the runtime treats their `tenant_id` claim (if any) as advisory and applies no implicit `WHERE tenant_id = ...` filter.
