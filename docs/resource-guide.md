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
endpoints:
relations:
indexes:
```

Rules:

- `resource`, `version`, and `schema` are required.
- `endpoints` is optional. If you omit it, Shaperail parses the resource but
  generates no HTTP routes.
- `relations` and `indexes` are optional.

## Example resource

```yaml
resource: users
version: 1

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
    method: GET
    path: /users
    auth: [member, admin]
    filters: [role, org_id]
    search: [name, email]
    pagination: cursor
    sort: [created_at, name]

  create:
    method: POST
    path: /users
    auth: [admin]
    input: [email, name, role, org_id]

relations:
  organization: { resource: organizations, type: belongs_to, key: org_id }

indexes:
  - { fields: [org_id, role] }
```

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
| `sensitive` | Redacted in all log output and error payloads |
| `search` | Enables PostgreSQL full-text search via `to_tsvector` on this field |
| `items` | Element type for `type: array` fields (required when type is array) |

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

## Endpoints

Endpoints are explicit. Nothing is generated unless you declare it.

```yaml
endpoints:
  list:
    method: GET
    path: /posts
    auth: public
    filters: [status, created_by]
    search: [title, body]
    pagination: cursor
    sort: [created_at, title]

  create:
    method: POST
    path: /posts
    auth: [admin, member]
    input: [title, slug, body, status, created_by]
```

### Endpoint attributes

| Key | Meaning |
| --- | --- |
| `method` | HTTP method: GET, POST, PATCH, PUT, DELETE |
| `path` | URL path pattern. Use `:id` for path parameters. |
| `auth` | `public`, `owner`, or a list of role names like `[admin, member]` |
| `input` | Fields accepted for writes. Only these fields are allowed in the request body. |
| `filters` | Fields available as query filters: `?filter[role]=admin` |
| `search` | Fields included in full-text search: `?search=term` |
| `pagination` | `cursor` (default) or `offset` |
| `sort` | Fields available for sorting: `?sort=-created_at,name` |
| `cache` | Cache config: `{ ttl: 60 }` or `{ ttl: 60, invalidate_on: [users.updated] }` |
| `hooks` | Hook functions to run before/after the operation |
| `events` | Events to emit on success (e.g., `[user.created]`) |
| `jobs` | Background jobs to enqueue on success (e.g., `[send_welcome_email]`) |
| `upload` | File upload config: `{ field: avatar, storage: s3, max_size: 5mb }` |
| `soft_delete` | When `true`, sets `deleted_at` instead of removing the row |

Important behavior:

- `input` controls which fields are accepted for writes.
- `filters`, `search`, `pagination`, and `sort` only exist when declared.
- `soft_delete: true` changes delete semantics to a `deleted_at` workflow.
  Requires an `updated_at` field in the schema.
- Hooks, jobs, events, and cache behavior are attached per endpoint, not
  inferred globally.
- Every create/update/delete automatically emits an event (`resource.action`)
  regardless of the `events` list.

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
