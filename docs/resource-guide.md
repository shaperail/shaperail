# Resource Guide

Shaperail’s resource YAML files are the source of truth. The runtime, routes,
OpenAPI spec, validation, and migrations all start from these files.

## File Location

Canonical location:

```text
resources/<resource-name>.yaml
```

Use `.yaml`, not `.yml`, for the canonical resource format.

## Top-Level Keys

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

## Schema

Schema fields use compact inline objects:

```yaml
schema:
  id:         { type: uuid, primary: true, generated: true }
  title:      { type: string, min: 1, max: 200, required: true }
  created_by: { type: uuid, required: true }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }
```

Common constraints:

- `primary: true`
- `generated: true`
- `required: true`
- `unique: true`
- `nullable: true`
- `ref: other_resource.id`
- `min` / `max`
- `format: email|url|uuid`
- `values: [...]` for enums
- `default: value`
- `sensitive: true`

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

Important rules:

- `input` controls which fields are accepted on write endpoints.
- `filters`, `search`, `pagination`, and `sort` only work if you declare them.
- `soft_delete: true` adds a `deleted_at` workflow for delete endpoints.

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

## Recommended Field Pattern

For resources with writes and ownership checks, this shape works well:

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
- `created_at` and `updated_at` support sorting and history

## Use The Example

For a full reference, read:

- [examples/blog-api/resources/posts.yaml](../examples/blog-api/resources/posts.yaml)
- [examples/blog-api/resources/comments.yaml](../examples/blog-api/resources/comments.yaml)
