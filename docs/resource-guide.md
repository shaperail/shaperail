---
title: Resource guide
parent: Reference
nav_order: 2
---

Resource files are the source of truth in Shaperail. Routes, validation,
OpenAPI, query behavior, auth checks, and migration generation all start from
the same YAML definition.

## File location

Place resources in:

```text
resources/<resource-name>.yaml
```

Use `.yaml`, not `.yml`, for the canonical format.

## Top-level contract

```yaml
resource:
version:
schema:
db:          # optional — named database connection
tenant_key:  # optional — schema field for multi-tenancy isolation
endpoints:
relations:
indexes:
```

Rules:

- `resource`, `version`, and `schema` are required.
- `db` is optional. When your project uses **multi-database** (`databases:` in
  `shaperail.config.yaml`), set `db` to a connection name (e.g. `analytics`) to
  route this resource’s data to that connection. Omit `db` or set it to a name
  that is not in `databases` to use the **default** connection.
- `tenant_key` is optional. When set, enables automatic multi-tenancy isolation.
  See [Multi-tenancy](#multi-tenancy) below.
- `endpoints` is optional. If you omit it, Shaperail parses the resource but
  generates no HTTP routes.
- `relations` and `indexes` are optional.

## Example resource

```yaml
resource: users
version: 1
# db: default   # optional; use when multi-database is configured

schema:
  id:         { type: uuid, primary: true, generated: true }
  email:      { type: string, format: email, unique: true, required: true }
  name:       { type: string, min: 1, max: 200, required: true }
  role:       { type: enum, values: [admin, member, viewer], default: member }
  org_id:     { type: uuid, ref: organizations.id, required: true }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }

endpoints:
  list:
    auth: [member, admin]
    filters: [role, org_id]
    search: [name, email]
    pagination: cursor
    sort: [created_at, name]

  create:
    auth: [admin]
    input: [email, name, role, org_id]
    controller:
      before: validate_org
    events: [user.created]
    jobs: [send_welcome_email]

relations:
  organization: { resource: organizations, type: belongs_to, key: org_id }

indexes:
  - { fields: [org_id, role] }
```

## API versioning

The `version` field on each resource drives URL path prefixing. All endpoints
for a resource are registered under `/v{version}/...`:

```yaml
resource: users
version: 1    # endpoints register under /v1/users, /v1/users/{id}, etc.
```

```yaml
resource: orders
version: 2    # endpoints register under /v2/orders, /v2/orders/{id}, etc.
```

The version prefix appears in:
- All HTTP routes at runtime
- The OpenAPI spec paths
- The output of `shaperail routes`

Each resource carries its own version independently. When you scaffold a new
project with `shaperail init`, resources default to `version: 1`.

## Multi-database (optional)

When your project config defines **`databases:`** (see
[Configuration reference]({{ '/configuration/' | relative_url }}#databases-multi-database)),
you can route a resource to a specific connection with the top-level **`db`** key:

```yaml
resource: events
version: 1
db: analytics    # use the "analytics" connection from databases: in config

schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
  # ...
```

- Omit `db` (or use a name that resolves to the default) to use the **default**
  connection. Migrations always run against the `default` connection.
- All endpoints for that resource (list, get, create, update, delete) use the
  same connection. Cross-database relations are not supported.

## Schema fields

Schema fields use compact inline objects. Every attribute:

| Key | Meaning |
| --- | --- |
| `type` | **Required.** Data type (see table below) |
| `primary` | Marks the primary key. Exactly one field must be primary. |
| `generated` | The runtime/database fills the value automatically (UUIDs, timestamps) |
| `required` | Field must be present on writes (adds NOT NULL in SQL) |
| `unique` | Adds a uniqueness constraint and matching SQL index |
| `nullable` | Field may be null |
| `ref` | Foreign key target in `resource.field` format (field must be `type: uuid`) |
| `min` / `max` | String length or numeric bounds. Validated at runtime. |
| `format` | Validation hint: `email`, `url`, or `uuid` |
| `values` | Allowed values for `type: enum` fields (required when type is enum) |
| `default` | Default value applied when the field is omitted |
| `sensitive` | Omitted from all responses; redacted in logs and error messages |
| `transient` | Input-only field. Validated and exposed to the `before:` controller via `ctx.input`, but never persisted (no migration column, no SQL reference) and never returned in responses. Stripped from `ctx.input` after the before-controller runs. Must appear in some endpoint's `input:` list. |
| `search` | Enables PostgreSQL full-text search via `to_tsvector` on this field |
| `items` | Element type for `type: array` fields (required when type is array). Accepts a bare type name (`items: string`) or a constraint map (`{ type: string, min: 3, max: 3 }`) — see [Array element constraints](#array-element-constraints) below. |

### Supported field types

| Type | SQL type | Rust type | Notes |
| --- | --- | --- | --- |
| `uuid` | `UUID` | `uuid::Uuid` | Use for primary keys and foreign keys |
| `string` | `VARCHAR(max)` or `TEXT` | `String` | Supports `min`, `max`, `format` |
| `integer` | `INTEGER` | `i32` | 32-bit signed |
| `bigint` | `BIGINT` | `i64` | 64-bit signed |
| `number` | `NUMERIC` | `f64` | 64-bit floating point |
| `boolean` | `BOOLEAN` | `bool` | |
| `timestamp` | `TIMESTAMPTZ` | `chrono::DateTime<Utc>` | Use `generated: true` for auto timestamps |
| `date` | `DATE` | `chrono::NaiveDate` | Date without time |
| `enum` | `TEXT` + CHECK | `String` | Requires `values` list |
| `json` | `JSONB` | `serde_json::Value` | Arbitrary JSON |
| `array` | varies | `Vec<T>` | Requires `items` for element type |
| `file` | `TEXT` | `String` | Stores file URL. Use with `upload:` on endpoints |

### Array element constraints

The `items:` key on an `array` field accepts either a bare type name (shorthand)
or a constraint map that applies to every element:

```yaml
schema:
  tags:       { type: array, items: string }                              # shorthand
  currencies: { type: array, items: { type: string, min: 3, max: 3 } }  # element constraints
  scores:     { type: array, items: { type: integer, min: 0, max: 100 } }
  flags:      { type: array, items: { type: enum, values: [a, b, c] } }
  org_ids:    { type: array, items: { type: uuid, ref: organizations.id } }
```

Constraint rules:

- All element constraints (`min`, `max`, `values`, `format`) are enforced on
  every write. A violation surfaces as `<field>[<index>]` in the error response
  — for example, `currencies[0]` if the first currency string is too short.
- `items.ref` performs a runtime existence check. On Postgres, the runtime runs
  `SELECT … WHERE id = ANY($1::uuid[])` and rejects the write with code
  `invalid_reference` if any element ID does not exist. This check is Postgres
  only; non-Postgres databases do not support it.
- Nested arrays are not supported. Use `type: json` for nested or hierarchical
  structure.

## Endpoints

Endpoints are explicit. Nothing is generated unless you declare it.

### Convention-based defaults

For the five standard CRUD action names, `method` and `path` are **optional**.
Shaperail infers them from the resource name:

| Action name | Default method | Default path |
| --- | --- | --- |
| `list` | GET | `/<resource>` |
| `get` | GET | `/<resource>/:id` |
| `create` | POST | `/<resource>` |
| `update` | PATCH | `/<resource>/:id` |
| `delete` | DELETE | `/<resource>/:id` |

For any **custom** endpoint name (e.g. `bulk_create`, `archive`), `method` and
`path` are **required** — the parser cannot guess them.

Example:

```yaml
endpoints:
  list:
    auth: public
    filters: [status, created_by]
    search: [title, body]
    pagination: cursor
    sort: [created_at, title]

  create:
    auth: [admin, member]
    input: [title, slug, body, status, created_by]
```

### Supported endpoint actions

| Action | Method | Typical path | Description |
| --- | --- | --- | --- |
| `list` | GET | `/resources` | List with pagination, filters, sort, search |
| `get` | GET | `/resources/:id` | Fetch a single record by ID |
| `create` | POST | `/resources` | Create a single record |
| `update` | PATCH | `/resources/:id` | Update a single record |
| `delete` | DELETE | `/resources/:id` | Delete (or soft-delete) a single record |
| `bulk_create` | POST | `/resources/bulk` | Create multiple records in one request |
| `bulk_delete` | DELETE | `/resources/bulk` | Delete multiple records by ID list |

### Endpoint attributes

| Key | Meaning |
| --- | --- |
| `method` | HTTP method: GET, POST, PATCH, PUT, DELETE. Optional for standard CRUD names (list, get, create, update, delete). |
| `path` | URL path pattern. Use `:id` for path parameters. Optional for standard CRUD names. |
| `auth` | `public`, `owner`, or a list of role names like `[admin, member]` |
| `input` | Fields accepted for writes. Only these fields are allowed in the request body. |
| `filters` | Fields available as query filters: `?filter[role]=admin` |
| `search` | Fields included in full-text search: `?search=term` |
| `pagination` | `cursor` (default) or `offset` |
| `sort` | Fields available for sorting: `?sort=-created_at,name` |
| `cache` | Cache config: `{ ttl: 60 }` or `{ ttl: 60, invalidate_on: [users.updated] }` |
| `controller` | Synchronous business logic: `{ before: fn, after: fn }`. Use a function name for Rust or `"wasm:./path.wasm"` for WASM plugins. See [Controllers]({{ '/controllers/' | relative_url }}). |
| `events` | Events to emit on success (e.g., `[user.created]`) |
| `jobs` | Background jobs to enqueue on success (e.g., `[send_welcome_email]`) |
| `upload` | Multipart file upload config: `{ field: avatar, storage: s3, max_size: 5mb }` |
| `rate_limit` | Per-endpoint rate limiting: `{ max_requests: 100, window_secs: 60 }`. Requires Redis. Silently skipped if Redis is not configured. |
| `soft_delete` | When `true`, sets `deleted_at` instead of removing the row |

Important behavior:

- `input` controls which fields are accepted for writes.
- `filters`, `search`, `pagination`, and `sort` only exist when declared.
- `soft_delete: true` changes delete semantics to a `deleted_at` workflow.
  Requires `deleted_at: { type: timestamp, nullable: true }` in the schema.
- Controllers, jobs, events, and cache behavior are attached per endpoint, not
  inferred globally.
- Every create/update/delete automatically emits an event (`resource.action`)
  regardless of the `events` list.
- Upload endpoints read `multipart/form-data`. The declared `upload.field` must
  also appear in `input`.

## Multi-tenancy

Add `tenant_key` at the top level of a resource to enable automatic tenant
isolation. The value must be the name of a `uuid` field in the schema:

```yaml
resource: projects
version: 1
tenant_key: org_id

schema:
  id:         { type: uuid, primary: true, generated: true }
  org_id:     { type: uuid, ref: organizations.id, required: true }
  name:       { type: string, min: 1, max: 200, required: true }
  created_at: { type: timestamp, generated: true }
```

When `tenant_key` is set, Shaperail enforces these rules automatically:

| Operation | Behavior |
| --- | --- |
| **List** | Adds `WHERE org_id = <tenant_id>` to every query |
| **Get** | Fetches the record, then verifies it belongs to the user's tenant |
| **Create** | Auto-injects the tenant_key value from the user's JWT claim |
| **Update** | Pre-fetches the record to verify tenant ownership before writing |
| **Delete** | Pre-fetches the record to verify tenant ownership before deleting |

### How the tenant ID is resolved

The `tenant_id` is read from the JWT `tenant_id` claim. Include it when
issuing tokens:

```json
{
  "sub": "user-123",
  "role": "member",
  "tenant_id": "org-abc-456"
}
```

### super_admin bypass

Users with the role `super_admin` bypass all tenant filtering. They can read,
update, and delete records across all tenants. This is useful for platform-level
admin dashboards and support tools.

### Cache and rate limit isolation

Cache keys automatically include the tenant ID, and rate-limit keys do too when
the runtime rate limiter is wired, so that:

- Cached responses are never shared across tenants
- Rate limits are enforced independently per tenant

### Validation rules

The validator checks that:

- `tenant_key` references a field that exists in the schema
- That field has `type: uuid`

If either check fails, `shaperail validate` reports an error.

## WASM plugins

Controllers support WASM plugins alongside Rust functions. Use the `wasm:`
prefix to point to a compiled `.wasm` file:

```yaml
endpoints:
  create:
    auth: [admin]
    input: [name, email]
    controller:
      before: "wasm:./plugins/validate_input.wasm"
```

WASM plugins run in a sandboxed environment with no filesystem, network, or
system access. They receive the controller context as JSON and return a modified
context or an error. See the [Controllers guide]({{ '/controllers/' | relative_url }}#wasm-plugins) for the full
plugin interface, compilation instructions, and example code.

## Relations

Relations are declared, not inferred:

```yaml
relations:
  comments: { resource: comments, type: has_many, foreign_key: post_id }
  author:   { resource: users, type: belongs_to, key: created_by }
```

Supported relation types:

- `belongs_to`
- `has_many`
- `has_one`

## Indexes

Indexes are also explicit:

```yaml
indexes:
  - { fields: [slug], unique: true }
  - { fields: [created_at], order: desc }
```

## Recommended resource shape

For resources with writes, sorting, and owner-based access, this pattern holds
up well:

```yaml
schema:
  id:         { type: uuid, primary: true, generated: true }
  title:      { type: string, min: 1, max: 200, required: true }
  created_by: { type: uuid, required: true }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }
```

Why:

- `id` is generated automatically
- `created_by` works with `owner` auth rules
- timestamp fields support sorting, audit trails, and update tracking

## See a complete example

Use the [Blog API example]({{ '/blog-api-example/' | relative_url }}) for a two-resource app with
public reads, protected writes, relations, and checked-in migrations.
