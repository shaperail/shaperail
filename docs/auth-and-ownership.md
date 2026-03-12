---
title: Auth and ownership
parent: Guides
nav_order: 2
---

Shaperail lets you declare auth at the endpoint layer so the access contract
stays next to the route definition.

## Public endpoints

```yaml
auth: public
```

No token is required.

## Role-based endpoints

```yaml
auth: [admin, member]
```

The request must carry credentials that map to one of those roles.

## Owner-based endpoints

```yaml
auth: owner
```

or:

```yaml
auth: [admin, owner]
```

Important behavior:

- `owner` checks the authenticated user against the record's `created_by` field
- if the record does not have `created_by`, the ownership check fails
- owner checks are best paired with role fallbacks such as `[admin, owner]`

## Recommended schema pattern

```yaml
schema:
  created_by: { type: uuid, required: true }
```

Recommended endpoint pattern:

```yaml
endpoints:
  create:
    input: [title, body, created_by]

  update:
    auth: [admin, owner]
    input: [title, body]
```

## Request headers

Shaperail checks credentials in this order:

1. **JWT** — `Authorization: Bearer <token>`
2. **API key** — `X-API-Key: <key>`

If both are absent, the request is unauthenticated. Protected endpoints return
401.

### JWT configuration

Set these in `shaperail.config.yaml` or via environment variables:

```yaml
auth:
  provider: jwt
  secret_env: JWT_SECRET
  expiry: 3600          # access token TTL in seconds (default: 24h)
  refresh_expiry: 86400 # refresh token TTL (default: 30d)
```

The JWT payload carries `sub` (user ID), `role`, and `token_type` (access or
refresh). Only `access` tokens are accepted for API requests.

### API key authentication

API keys are an alternative to JWT for service-to-service calls. Each key maps
to a user ID and role. Keys are checked via the `X-API-Key` header when no
Bearer token is present.

## Rate limiting

Shaperail enforces sliding-window rate limits per IP or per authenticated user,
backed by Redis.

Default limits: 100 requests per 60-second window.

When the limit is exceeded, the response is `429 Rate Limited`. Rate limit
state is stored in Redis and survives server restarts.

Rate limiting keys:
- Unauthenticated: `ip:<address>`
- Authenticated: `user:<user_id>`

## What Shaperail does not do automatically

Shaperail does not currently auto-fill `created_by` from the token.

You need to choose one of these patterns:

- send `created_by` explicitly in the create payload
- use a hook that sets or validates it before insert

## Practical policy for first projects

| Route type | Recommended auth |
| --- | --- |
| Public content reads | `public` |
| Authenticated writes | `[admin, member]` |
| User-owned updates | `[admin, owner]` |
| Destructive admin-only operations | `[admin]` |

## Example

The [Blog API example]({{ '/blog-api-example/' | relative_url }}) shows `owner`-based updates for
both posts and comments using a shared `created_by` pattern.
