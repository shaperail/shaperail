---
title: Security
parent: Guides
nav_order: 7
---

# Security best practices

This guide covers what Shaperail protects automatically, what you need to
configure, and how to harden a production deployment.

---

## 1. Security by default

Shaperail applies several security measures without any configuration from you.

### SQL injection prevention

All database queries go through sqlx with parameterized queries. User input
never touches raw SQL strings. The generated code uses `$1`, `$2`, etc. bind
parameters for every value -- there is no string interpolation path.

### Input validation

Every field declared in the resource schema is validated before reaching the
database:

- `required: true` -- rejects missing fields
- `min` / `max` -- enforces length or value bounds
- `format: email` -- validates email format
- `type: enum` with `values: [...]` -- rejects values outside the enum set
- `type: uuid` -- validates UUID format

Invalid input returns a `422 VALIDATION_ERROR` response with per-field details:

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "status": 422,
    "message": "Validation failed",
    "request_id": "abc-123",
    "details": [
      { "field": "email", "message": "is required", "code": "required" },
      { "field": "name", "message": "too short", "code": "too_short" }
    ]
  }
}
```

### Strict schema parsing with deny_unknown_fields

All Shaperail configuration and resource types use `#[serde(deny_unknown_fields)]`.
Typos and unsupported fields produce immediate, clear errors at startup or
validation time rather than being silently ignored.

```
error: unknown field `databse`, expected one of `project`, `port`, `workers`, `database`, ...
```

This prevents configuration drift and catches security-relevant mistakes like
misspelling `auth:` on an endpoint.

### Sensitive field redaction

Fields marked `sensitive: true` are automatically redacted in all log output
and error payloads:

```yaml
schema:
  email: { type: string, format: email, unique: true, required: true, sensitive: true }
```

Sensitive fields still work normally in API responses -- they are only redacted
from server-side logs and tracing spans.

### Structured error responses

Internal error details (database messages, stack traces) are never exposed to
clients. All errors follow the PRD-mandated envelope format with a
machine-readable `code`, HTTP `status`, and a safe `message`. The
`ShaperailError::Internal` variant logs the real error server-side and returns
only `"Internal server error"` to the caller.

---

## 2. JWT configuration

### Minimal setup

```yaml
auth:
  provider: jwt
  secret_env: JWT_SECRET
  expiry: 1h
  refresh_expiry: 7d
```

The `secret_env` field names an environment variable -- the secret itself never
appears in the config file or source control.

Current implementation note: the scaffolded app currently reads JWT settings
from the `JWT_SECRET` environment variable directly and uses built-in 24h
access / 30d refresh defaults. The `auth:` block above is parsed and validated,
but its `secret_env`, `expiry`, and `refresh_expiry` values are not consumed by
the generated bootstrap unless you wire that yourself.

### Secret strength

Use a cryptographically random secret of at least 256 bits (32 bytes). Generate
one with:

```bash
openssl rand -base64 32
```

Store it in your secrets manager (AWS Secrets Manager, Vault, etc.) and inject
it as `JWT_SECRET` at deploy time.

### Token expiry guidelines

| Token type    | Recommended expiry | Rationale |
| ------------- | ------------------ | --------- |
| Access token  | 15 minutes -- 1 hour | Short-lived tokens limit the blast radius of a stolen token |
| Refresh token | 7 -- 30 days       | Allows re-authentication without passwords; revoke on logout |

If you customize the bootstrap to read `auth.expiry`, keep access tokens as
short-lived as your UX allows.

### Secret rotation

To rotate the JWT secret without downtime:

1. Deploy new instances with the new `JWT_SECRET` value.
2. Keep old instances running until all existing access tokens expire (based on
   your `expiry` setting).
3. Drain and terminate old instances.

For zero-downtime rotation, implement a controller that accepts tokens signed
with either the old or new secret during the transition window.

### JWT Claims

Shaperail mints HS256 JWTs with this claim shape (`shaperail_runtime::auth::Claims`,
re-exported from the auth module):

| Claim | Required for | Notes |
| --- | --- | --- |
| `sub` | always | Opaque subject identifier per RFC 7519. For tenant roles this is conventionally a `users.id` UUID; for `super_admin` it is a routable identity that does NOT exist in `users`. **Do NOT bind `sub` to a foreign-key column without verifying.** Exposed in custom handlers as `AuthenticatedUser.sub` and `Subject.sub` (renamed from `.id` in v0.13.0). |
| `role` | always | Must match a role in any endpoint's `auth:` list, or be `super_admin` for unrestricted access. |
| `iat` / `exp` | always | Unix seconds. |
| `token_type` | always | `"access"` for protected requests; `"refresh"` is only valid against the refresh endpoint. |
| `tenant_id` | non-`super_admin` accessing tenant-scoped resources | Missing/null → 401. |

**Minting a token for tests:**

```rust
use shaperail_runtime::auth::JwtConfig;

let config = JwtConfig::new("test-secret-at-least-32-bytes-long!", 3600, 86400);
let token = config
    .encode_access_with_tenant("user-uuid", "admin", Some("org-uuid"))
    .unwrap();
```

**Diagnosing 401s:** the runtime emits `tracing::warn!` lines when JWTs are
rejected. Set `RUST_LOG=shaperail_runtime::auth=warn` to surface them. Two
common messages:

- `JWT rejected: decode failed` — signature mismatch, expired, or malformed.
- `JWT rejected: token_type must be "access"` — typically a refresh token sent
  against a protected endpoint.

### Refresh token handling

- The JWT payload includes a `token_type` field (`access` or `refresh`). Only
  `access` tokens are accepted for API requests.
- Store refresh tokens securely on the client (HTTP-only cookies or secure
  native storage).
- Implement refresh token rotation: when a refresh token is used, issue a new
  refresh token and invalidate the old one.

---

## 3. API key management

### When to use API keys vs JWT

| Use case                | Credential type |
| ----------------------- | --------------- |
| Browser / mobile users  | JWT             |
| Service-to-service      | API key         |
| CI/CD pipelines         | API key         |
| Third-party integrations| API key         |

API keys are sent via the `X-API-Key` header and map to a user ID and role.
They are checked only when no Bearer token is present.

Current implementation note: API key auth is a runtime primitive. It works only
when you inject an `ApiKeyStore` into the Actix app. The scaffolded app does
not do this automatically.

### Key rotation

- Give each API key a human-readable label and creation timestamp.
- Issue a new key before revoking the old one. Overlap by at least one
  deployment cycle.
- Audit API key usage regularly. Revoke keys that have not been used in 90+
  days.
- Never embed API keys in client-side code, Git repositories, or Docker images.

### Principle of least privilege

Assign each API key the narrowest role required for its function. A reporting
service that only reads data should have a `viewer` or `member` role, not
`admin`.

---

## 4. Rate limiting

### How it works

Shaperail includes a Redis-backed sliding window rate limiter primitive. When
wired into request handling, the default is 100 requests per 60-second window
and over-limit requests return `429 Rate Limited`.

Rate limit keys follow this priority:

| Condition              | Key format                           |
| ---------------------- | ------------------------------------ |
| Authenticated + tenant | `t:<tenant_id>:user:<user_id>`       |
| Authenticated          | `user:<user_id>`                     |
| Unauthenticated        | `ip:<address>`                       |

Rate limit state is stored in Redis and survives server restarts.

Current implementation note: the scaffolded app does not enable the rate
limiter automatically. If you need application-level rate limiting today, you
must wire the `RateLimiter` into your server bootstrap yourself.

### Tuning limits

Adjust `max_requests` and `window_secs` based on your workload:

- **Public APIs**: 30--60 requests per 60 seconds per IP.
- **Authenticated users**: 100--300 requests per 60 seconds per user.
- **Internal services using API keys**: higher limits or a separate rate limit
  tier.

### Abuse prevention strategies

- Use per-IP rate limits for unauthenticated endpoints (login, registration)
  to slow credential stuffing.
- Apply stricter limits to write endpoints (`POST`, `PATCH`, `DELETE`) than
  read endpoints.
- Monitor `429` response rates in your observability stack. A spike indicates
  either a legitimate traffic burst or an attack.
- Consider adding a reverse proxy (nginx, Cloudflare) in front of Shaperail
  for connection-level rate limiting and IP reputation filtering.

---

## 5. CORS and origin validation

Shaperail does not currently generate CORS middleware automatically. For
browser-facing APIs, configure CORS at the reverse proxy or add Actix-web CORS
middleware in a controller.

### Recommended CORS policy

- **Never** use `Access-Control-Allow-Origin: *` on authenticated endpoints.
- Allowlist specific origins that need browser access.
- Restrict `Access-Control-Allow-Methods` to the HTTP methods your API uses.
- Set `Access-Control-Allow-Credentials: true` only when you need cookie-based
  auth.
- Keep `Access-Control-Max-Age` reasonable (e.g., 3600 seconds) to reduce
  preflight traffic without caching stale policies too long.

### Reverse proxy example (nginx)

```nginx
location /v1/ {
    if ($request_method = 'OPTIONS') {
        add_header 'Access-Control-Allow-Origin' 'https://app.example.com';
        add_header 'Access-Control-Allow-Methods' 'GET, POST, PATCH, DELETE';
        add_header 'Access-Control-Allow-Headers' 'Authorization, Content-Type, X-API-Key';
        add_header 'Access-Control-Max-Age' 3600;
        return 204;
    }
    add_header 'Access-Control-Allow-Origin' 'https://app.example.com';
    proxy_pass http://127.0.0.1:3000;
}
```

---

## 6. Input validation

### Built-in schema validation

Declare validation constraints directly in the resource schema:

```yaml
schema:
  email:  { type: string, format: email, unique: true, required: true }
  name:   { type: string, min: 1, max: 200, required: true }
  role:   { type: enum, values: [admin, member, viewer], default: member }
  age:    { type: integer, min: 0, max: 150 }
```

All constraints are enforced before the request reaches the database or any
controller logic.

### Custom validation in controllers

For validation that goes beyond field-level constraints (cross-field checks,
external lookups), use a `before` controller:

```yaml
endpoints:
  create:
    method: POST
    path: /users
    auth: [admin]
    input: [email, name, role, org_id]
    controller:
      before: validate_org
```

```rust
// resources/users.controller.rs
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

pub async fn validate_org(ctx: &mut Context) -> ControllerResult {
    let org_id = ctx.input.get("org_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShaperailError::Validation(vec![
            FieldError {
                field: "org_id".into(),
                message: "must be a valid organization".into(),
                code: "invalid_reference".into(),
            }
        ]))?;

    // Verify the organization exists and is active
    let org = sqlx::query!("SELECT active FROM organizations WHERE id = $1", org_id)
        .fetch_optional(&*ctx.db)
        .await?;

    match org {
        Some(row) if row.active => Ok(()),
        _ => Err(ShaperailError::Validation(vec![
            FieldError {
                field: "org_id".into(),
                message: "organization not found or inactive".into(),
                code: "invalid_reference".into(),
            }
        ])),
    }
}
```

### What to validate in controllers

- Cross-field consistency (e.g., `end_date` must be after `start_date`)
- Foreign key existence and status checks beyond simple `ref:` constraints
- Business rules (e.g., a free-tier org cannot have more than 5 members)
- Rate or quota checks tied to application state

---

## 7. Sensitive data handling

### Marking fields as sensitive

Add `sensitive: true` to any field containing PII or secrets:

```yaml
schema:
  email:        { type: string, format: email, sensitive: true, required: true }
  phone:        { type: string, sensitive: true }
  ssn_last_four: { type: string, sensitive: true }
```

Effects:

- The field value is replaced with `[REDACTED]` in all `tracing` log output.
- The field is excluded from search indexes.
- Error payloads that reference the field do not include its value.

### Logging discipline

- Set `logging.level: info` in production. Avoid `debug` level, which may log
  full request and response bodies.
- Use `logging.format: json` for structured logs that are easier to audit and
  filter.
- Send logs to an OTLP collector (`logging.otlp_endpoint`) rather than writing
  to disk, so you get centralized, searchable, access-controlled log storage.

### Data at rest

Shaperail does not encrypt individual columns. For highly sensitive data:

- Use PostgreSQL column-level encryption (pgcrypto) or transparent data
  encryption.
- Encrypt at the application layer in a `before` controller and decrypt in an
  `after` controller.
- Consider storing sensitive data in a dedicated secrets vault and keeping only
  references in the database.

---

## 8. Multi-tenancy security

### How tenant isolation works

When a resource declares `tenant_key`, every database query is automatically
scoped to the authenticated user's `tenant_id` JWT claim:

- **List**: adds `WHERE <tenant_key> = $tenant_id`
- **Get / Update / Delete**: fetches the record and verifies it belongs to the
  tenant before proceeding

A user in tenant A will never see, modify, or delete records belonging to
tenant B. Cross-tenant access attempts return `404 Not Found` (not `403
Forbidden`) to avoid leaking information about other tenants' data.

### Cache and rate limit isolation

Cache keys and rate limit keys include the tenant ID, so tenants never share
cached data or rate limit budgets:

```
shaperail:projects:list:<hash>:org-abc:member   # tenant A
shaperail:projects:list:<hash>:org-xyz:member   # tenant B (separate)
```

### The super_admin role

Users with the `super_admin` role bypass all tenant filtering. Restrict this
role to:

- Platform admin dashboards
- Support and debugging tools
- Data migration scripts

Never issue `super_admin` tokens to end users or external API consumers. Audit
`super_admin` usage with the event log.

### Tenant isolation checklist

- Every tenant-scoped resource has `tenant_key` set
- Your auth service includes `tenant_id` in every JWT for tenant users
- Non-tenant resources (e.g., the tenants table itself) use strict role-based
  auth (`auth: [admin]` or `auth: [super_admin]`)
- You test cross-tenant access by requesting records from tenant B with a
  tenant A token and verifying a 404 response

---

## 9. Webhook security

### Outbound webhook signing

Shaperail includes an outbound webhook signing helper that produces
`X-Shaperail-Signature: sha256=<hex>`:

```
X-Shaperail-Signature: sha256=<hex-encoded HMAC-SHA256 digest>
```

Configure the signing secret via an environment variable:

```yaml
events:
  webhooks:
    secret_env: WEBHOOK_SECRET
    timeout_secs: 30
    max_retries: 3
```

Current implementation note: the runtime can build signed webhook requests, but
the scaffolded app does not register a real delivery handler for queued webhook
jobs. Actual HTTP delivery is still a manual worker integration step.

### Verifying outbound webhooks (receiver side)

On the receiving end, verify the signature before processing the payload:

```python
# Example: Python receiver
import hmac, hashlib

def verify_shaperail_webhook(body: bytes, secret: str, signature_header: str) -> bool:
    expected = "sha256=" + hmac.new(
        secret.encode(), body, hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(expected, signature_header)
```

Always use constant-time comparison (`hmac.compare_digest` in Python,
`ring::constant_time::verify_slices_are_equal` in Rust) to prevent timing
attacks.

### Inbound webhook verification

The runtime includes inbound webhook verification helpers. Configure each source
with its own secret:

```yaml
events:
  inbound:
    - path: /webhooks/stripe
      secret_env: STRIPE_WEBHOOK_SECRET
      events: ["payment.completed", "subscription.updated"]
    - path: /webhooks/github
      secret_env: GITHUB_WEBHOOK_SECRET
      events: []
```

Supported signature formats:

| Service    | Header                  | Format                         |
| ---------- | ----------------------- | ------------------------------ |
| Shaperail  | `X-Shaperail-Signature` | `sha256=<hex>`                 |
| GitHub     | `X-Hub-Signature-256`   | `sha256=<hex>`                 |
| Stripe     | `Stripe-Signature`      | `t=<timestamp>,v1=<signature>` |

Requests with invalid or missing signatures return `401 Unauthorized` once you
register the inbound route helper in your app.

### Webhook security tips

- Use a unique secret per inbound webhook source. Never reuse your outbound
  `WEBHOOK_SECRET` for inbound verification.
- Rotate webhook secrets periodically. Most providers support having two active
  secrets during rotation.
- Filter inbound events with the `events:` list so your handler only processes
  expected event types.
- If you implement delivery logging, monitor it for failed deliveries, which
  may indicate a misconfigured secret or a replay attempt.

---

## 10. Production security checklist

Use this checklist before deploying to production.

### Secrets and configuration

- [ ] `JWT_SECRET` is a random 256-bit (32-byte) value stored in a secrets
  manager
- [ ] `WEBHOOK_SECRET` and all `*_WEBHOOK_SECRET` values are unique, random,
  and stored securely
- [ ] No secrets appear in `shaperail.config.yaml`, source control, or Docker
  images
- [ ] All secret references use `secret_env:` (environment variable indirection)

### Authentication and authorization

- [ ] `auth:` is declared on every endpoint that is not intentionally public
- [ ] If you customized JWT TTLs, access token `expiry` is 1 hour or less
- [ ] Refresh token rotation is implemented (new refresh token on each use)
- [ ] If API keys are enabled, they use the narrowest role required
- [ ] If API keys are enabled, unused keys are revoked

### Input validation

- [ ] All user-facing string fields have `min` and `max` constraints
- [ ] Enum fields have explicit `values` lists
- [ ] Cross-field and business-rule validation is handled in `before`
  controllers
- [ ] Resource files pass `shaperail validate` with no warnings

### Rate limiting

- [ ] If you enabled the runtime rate limiter, Redis is configured and the
  limiter is wired into requests
- [ ] If you enabled application-level rate limiting, limits are tuned for your
  traffic profile
- [ ] A reverse proxy provides connection-level rate limiting in addition to
  application-level limits

### Data protection

- [ ] PII fields are marked `sensitive: true`
- [ ] Logging level is `info` (not `debug`) in production
- [ ] Logs are shipped to a centralized, access-controlled system via OTLP
- [ ] Database connections use TLS (`sslmode=require` in the connection URL)
- [ ] Soft delete is enabled (`soft_delete: true`) for resources with
  compliance requirements

### Multi-tenancy

- [ ] Every tenant-scoped resource has `tenant_key` set
- [ ] JWTs include `tenant_id` for all tenant users
- [ ] `super_admin` tokens are issued only to platform operators
- [ ] Cross-tenant access is tested and verified to return 404

### Network and infrastructure

- [ ] Shaperail is behind a reverse proxy that terminates TLS
- [ ] CORS is configured to allowlist specific origins (no wildcard on
  authenticated endpoints)
- [ ] Database and Redis are not exposed to the public internet
- [ ] Docker images are built from scratch base (`shaperail build --docker`)
  to minimize attack surface
- [ ] Dependencies are audited regularly (`cargo audit`)

### Monitoring

- [ ] `429 Rate Limited` responses are tracked and alerted on
- [ ] `401` and `403` error rates are monitored for brute-force attempts
- [ ] Webhook delivery failures are monitored via the delivery log
- [ ] The `shaperail_event_log` table is used as an audit trail for data
  mutations
