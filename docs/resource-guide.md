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

Schema fields use compact inline objects. Common flags:

| Key | Meaning |
| --- | --- |
| `type` | Data type such as `uuid`, `string`, `integer`, `enum`, `timestamp`, `json`, or `file` |
| `primary` | Marks the primary key |
| `generated` | The runtime/database fills the value automatically |
| `required` | Field must be present on writes |
| `unique` | Adds uniqueness expectations and matching SQL index behavior |
| `nullable` | Field may be null |
| `ref` | Declares a relation target such as `organizations.id` |
| `min` / `max` | String or numeric bounds |
| `format` | Validation hint such as `email`, `url`, or `uuid` |
| `values` | Allowed enum values |
| `default` | Default value |
| `sensitive` | Marks fields that should be treated carefully in logs and output |

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

Important behavior:

- `input` controls which fields are accepted for writes.
- `filters`, `search`, `pagination`, and `sort` only exist when declared.
- `soft_delete: true` changes delete semantics to a `deleted_at` workflow.
- Hooks, jobs, events, and cache behavior are attached per endpoint, not
  inferred globally.

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
