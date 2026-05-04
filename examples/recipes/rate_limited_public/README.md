# Recipe: Rate-Limited Public Endpoint

## WHEN to use this

Use this recipe when you need a **public-create endpoint that is open to unauthenticated callers** but must be guarded against abuse:
- Contact forms, newsletter signups, feedback submissions — anything where requiring a login would reduce conversion.
- You want automatic per-IP rate limiting without writing Redis middleware yourself.
- Admin staff need to read and triage submissions, but submissions must be writable by the public.

## What this gives you

```
GET    /v1/contact_requests          → list (admin) — filterable by email
GET    /v1/contact_requests/:id      → get (admin)
POST   /v1/contact_requests          → create (public) — max 5 per IP per 60 seconds
DELETE /v1/contact_requests/:id      → delete (admin)
```

### Public create with rate limiting

```yaml
create:
  auth: public
  input: [email, name, message]
  rate_limit: { max_requests: 5, window_secs: 60 }
```

`auth: public` means no JWT or session token is required. Any caller can POST.

`rate_limit` adds a per-IP Redis sliding-window counter. When a single IP exceeds 5 requests within 60 seconds, the runtime returns **429 Too Many Requests**. Requires Redis; silently passes through if Redis is not configured (fail-open).

### source_ip is server-side only

```yaml
source_ip: { type: string, max: 45, generated: true }
```

`source_ip` is declared `generated: true` and is **not** in `create.input`. The runtime captures the client IP from the request headers and injects it during the write. Callers cannot supply or spoof this field.

Do not put `source_ip` in `input:`. If you do, any caller can claim any IP address, making rate limiting and audit trails meaningless.

## When NOT to use this

- **Authenticated endpoints**: don't add `rate_limit:` to endpoints that already require a JWT. You have other tools (per-user quota, plan-level throttling) for authenticated rate limiting. `rate_limit:` is optimized for per-IP anonymous traffic.
- **High-throughput public APIs**: 5 requests per 60 seconds is appropriate for contact forms. Adjust `max_requests` and `window_secs` for your actual use case. A rate of 1000/60s is not appropriate for anonymous public APIs.
- **Hard blocking**: `rate_limit:` returns 429; it does not ban IPs. For actual IP banning, use a CDN or WAF layer in front of Shaperail.
- **Captcha bypass**: `rate_limit:` counts requests; it does not verify humans. Use a `before:` controller to call a captcha service if you need to distinguish humans from bots.

## Key design notes for LLM authors

1. `auth: public` (scalar string, not an array) is the canonical syntax for unauthenticated endpoints. `auth: []` is NOT valid — the validator rejects an empty auth array. Use `auth: public`.

2. `source_ip` uses `generated: true` — this signals to the runtime that the value comes from server-side context, not the request body. For contact forms, the runtime derives this from `X-Forwarded-For` or the connection remote address.

3. The admin-only read endpoints (`list`, `get`, `delete`) pair naturally with a public-write endpoint. Never expose `source_ip` or internal metadata in the public-facing create response — consider a `sensitive: true` annotation if you want to redact it from all responses including admin reads.

4. The `rate_limit:` key uses `max_requests` and `window_secs` — not `max:` and `window:`. The resource-format doc (resource-format.md) has the canonical names; always check there.
