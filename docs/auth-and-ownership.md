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

No credentials are required.

## Role-based endpoints

```yaml
auth: [admin, member]
```

The authenticated request must map to one of those roles.

## Owner-based endpoints

```yaml
auth: owner
```

or:

```yaml
auth: [admin, owner]
```

Important behavior:

- `owner` compares the authenticated user to the row's `created_by` field
- if `created_by` is missing, the owner check fails closed
- `owner` is usually best paired with a role fallback such as `[admin, owner]`

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

The extractor checks credentials in this order:

1. `Authorization: Bearer <token>`
2. `X-API-Key: <key>`

## JWTs in the scaffolded app

JWT auth works out of the box in the generated app when `JWT_SECRET` is set.

Current limitation: the scaffold currently constructs JWT settings from the
`JWT_SECRET` environment variable only. The `auth:` block in
`shaperail.config.yaml` is parsed by the config schema, but the generated
bootstrap does not currently read `secret_env`, `expiry`, or `refresh_expiry`
from that block.

So today:

- set `JWT_SECRET`
- expect the scaffold to use the built-in 24h access / 30d refresh defaults
- customize bootstrap code yourself if you need different JWT settings

## API keys

The runtime supports API key auth through `X-API-Key`, but it only works when
you inject an `ApiKeyStore` into the Actix app.

The scaffolded app does not create or populate an API key store automatically,
so API keys are currently a manual integration step.

## Rate limiting

Rate limiting is declared per-endpoint in the resource YAML:

```yaml
endpoints:
  list:
    auth: [member, admin]
    rate_limit: { max_requests: 100, window_secs: 60 }
```

Behavior:
- Uses a Redis sliding window — requires Redis to be configured
- Silently skipped when Redis is absent (no error, no enforcement)
- A startup warning is logged when `rate_limit:` is declared on any endpoint but `REDIS_URL` is not set

## What Shaperail does not do automatically

Shaperail does not auto-fill `created_by` from the authenticated user.

You still need to choose one of these patterns:

- send `created_by` explicitly in the create payload
- use a controller that sets or validates it before insert

## Practical policy for first projects

| Route type | Recommended auth |
| --- | --- |
| Public content reads | `public` |
| Authenticated writes | `[admin, member]` |
| User-owned updates | `[admin, owner]` |
| Destructive admin-only operations | `[admin]` |

## Multi-tenancy

When you use `tenant_key`, tenant scoping builds on the authenticated user's
`tenant_id`. See the [Multi-tenancy guide]({{ '/multi-tenancy/' | relative_url }})
for the full behavior.
