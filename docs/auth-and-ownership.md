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

JWT:

```http
Authorization: Bearer <token>
```

API key:

```http
X-API-Key: <key>
```

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
