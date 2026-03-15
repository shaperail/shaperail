---
title: GraphQL
parent: Guides
nav_order: 12
---

When you enable GraphQL, Shaperail exposes the same resources as your REST API over a single GraphQL endpoint. The resource YAML is the source of truth for both: types, fields, relations, and auth are derived from the same schema.

## Enabling GraphQL

In `shaperail.config.yaml`, add `graphql` to the `protocols` list:

```yaml
project: my-app
protocols: [rest, graphql]
# ... database, cache, auth, etc.
```

If you omit `protocols`, only REST is enabled. With `protocols: [rest, graphql]`, the server registers:

| URL | Purpose |
| --- | --- |
| `POST /graphql` | GraphQL endpoint. Send queries and mutations as JSON: `{ "query": "...", "variables": { ... } }`. |
| `GET /graphql/playground` | GraphQL Playground — interactive editor and docs (intended for development). |

## Authentication

GraphQL uses the same auth as REST:

- Send a JWT in the `Authorization: Bearer <token>` header on the `POST /graphql` request.
- API keys are supported via `X-API-Key` if configured.
- Each field backed by an endpoint respects that endpoint’s `auth` (roles and owner checks). Unauthorized requests receive GraphQL errors, not data.

## Queries

### List

For each resource with a `list` endpoint, the schema exposes a list query. The type name is the resource name with the first letter of each word capitalized and words joined (e.g. `users` → `Users`; multi-word resources follow the same pattern).

Example:

```graphql
query {
  users(limit: 10, cursor: null) {
    id
    email
    name
    role
  }
}
```

List queries support `limit`, `cursor` (for cursor-based pagination), and filter arguments that match the endpoint’s `filters` (e.g. `role`, `org_id`). The exact arguments are generated from your resource definition.

### Get by ID

For each resource with a `get` (or equivalent) endpoint, a singular query is exposed, e.g.:

```graphql
query {
  user(id: "550e8400-e29b-41d4-a716-446655440000") {
    id
    email
    name
    organization { id name }
  }
}
```

Owner-based auth is enforced: if the endpoint requires ownership, the resolver checks that the authenticated user is allowed to see the requested row.

### Nested relations

Relation fields are exposed on the types. The same `relations` block in your resource YAML drives GraphQL:

- **belongs_to** — One related object (e.g. `organization` on `user`).
- **has_many** — List of related objects (e.g. `orders` on `user`).
- **has_one** — Single related object.

You can nest these in the query:

```graphql
query {
  user(id: "...") {
    id
    name
    organization { id name }
    orders { id total status }
  }
}
```

## Mutations

For each resource with `create`, `update`, or `delete` endpoints, the schema exposes:

- `create<Resource>(input: <Resource>Input)` — Returns the created row.
- `update<Resource>(id: String!, input: <Resource>Input)` — Returns the updated row.
- `delete<Resource>(id: String!)` — Returns the deleted row (or the row with soft-delete fields set). For hard deletes, the returned object is the last state before deletion.

Input types are generated from the endpoint’s `input` list (create/update). Only non-generated, non-primary fields you declare as input are included.

Auth is enforced the same way as REST: role and owner checks run before the mutation. Unauthorized mutations return errors and do not perform the operation.

## Same schema as REST

Resource YAML drives both REST and GraphQL. You do not define types twice. Changes to schema, endpoints, relations, or auth are reflected in both APIs after you regenerate and restart.

## Summary

| Feature | Supported |
| --- | --- |
| Queries: list (filters, pagination) | Yes |
| Queries: get by id | Yes |
| Queries: nested relations (belongs_to, has_many, has_one) | Yes |
| Mutations: create, update, delete | Yes |
| Auth: JWT, API key, RBAC, owner checks | Yes (same as REST) |
| Playground | Yes (`/graphql/playground`) |

Depth and complexity limits are configurable via the `graphql:` section in `shaperail.config.yaml`:

```yaml
graphql:
  depth_limit: 10        # default: 16
  complexity_limit: 200   # default: 256
```

See [Configuration > graphql]({{ '/configuration/' | relative_url }}) for details.
