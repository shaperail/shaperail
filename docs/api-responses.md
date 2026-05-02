---
title: API responses and query parameters
parent: Reference
nav_order: 4
---

# API responses and query parameters

Every Shaperail endpoint returns a consistent JSON envelope. This page
documents the response shapes, error format, and query parameters available on
list endpoints.

---

## Response envelope

### Single record (GET /v1/resources/:id, PATCH /v1/resources/:id)

HTTP 200 for GET and PATCH. HTTP 201 for POST.

```json
{
  "data": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "email": "alice@example.com",
    "name": "Alice",
    "role": "admin",
    "created_at": "2026-01-15T09:30:00Z"
  }
}
```

### List (GET /v1/resources)

The `meta` object varies by pagination style.

**Cursor pagination** (default):

```json
{
  "data": [
    { "id": "aaa-...", "name": "Alice" },
    { "id": "bbb-...", "name": "Bob" }
  ],
  "meta": {
    "cursor": "NTUwZTg0MDAtZTI5Yi00MWQ0LWE3MTYtNDQ2NjU1NDQwMDAw",
    "has_more": true
  }
}
```

`cursor` is `null` when there are no more pages. `has_more` is `false` on the
last page.

**Offset pagination**:

```json
{
  "data": [
    { "id": "aaa-...", "name": "Alice" },
    { "id": "bbb-...", "name": "Bob" }
  ],
  "meta": {
    "offset": 0,
    "limit": 25,
    "total": 42
  }
}
```

### Bulk create (POST /resources/bulk)

Send a raw JSON array in the request body:

```json
[
  { "email": "alice@example.com", "name": "Alice", "role": "admin" },
  { "email": "bob@example.com", "name": "Bob", "role": "member" }
]
```

Response (HTTP 200):

```json
{
  "data": [
    { "id": "aaa-...", "status": "created" },
    { "id": "bbb-...", "status": "created" }
  ],
  "meta": {
    "total": 2
  }
}
```

Declare the endpoint to enable bulk create:

```yaml
endpoints:
  bulk_create:
    method: POST
    path: /users/bulk
    auth: [admin]
    input: [email, name, role, org_id]
```

All records are inserted in a single transaction — if any record fails
validation, the entire batch is rolled back.

### Bulk delete (DELETE /resources/bulk)

Send a raw JSON array of IDs in the request body:

```json
[
  "550e8400-e29b-41d4-a716-446655440000",
  "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
]
```

Response (HTTP 200):

```json
{
  "data": [
    { "id": "550e8400-e29b-41d4-a716-446655440000", "email": "alice@example.com" },
    { "id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8", "email": "bob@example.com" }
  ],
  "meta": {
    "total": 2
  }
}
```

Declare the endpoint:

```yaml
endpoints:
  bulk_delete:
    method: DELETE
    path: /users/bulk
    auth: [admin]
```

Bulk delete uses the same bulk response envelope as bulk create. For
`soft_delete: true`, the returned items are the soft-deleted rows; for hard
delete, they are the deletion results returned by the store layer.

### Delete (DELETE /v1/resources/:id)

Returns HTTP 204 No Content with an empty body.

---

## Error responses

All errors use the same envelope:

```json
{
  "error": {
    "code": "NOT_FOUND",
    "status": 404,
    "message": "Resource not found",
    "request_id": "req-abc-123",
    "details": null
  }
}
```

### Error codes

| Status | Code               | Meaning                                |
|--------|--------------------|----------------------------------------|
| 401    | `UNAUTHORIZED`     | Missing or invalid authentication.     |
| 403    | `FORBIDDEN`        | Authenticated but insufficient permissions. |
| 404    | `NOT_FOUND`        | Resource does not exist.               |
| 409    | `CONFLICT`         | Unique constraint or state conflict.   |
| 422    | `VALIDATION_ERROR` | One or more fields failed validation.  |
| 429    | `RATE_LIMITED`     | Rate limit exceeded.                   |
| 500    | `INTERNAL_ERROR`   | Unexpected server error.               |

### Validation errors

When the code is `VALIDATION_ERROR`, the `details` field contains an array of
per-field errors:

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "status": 422,
    "message": "Validation failed",
    "request_id": "req-def-456",
    "details": [
      { "field": "email", "message": "is required", "code": "required" },
      { "field": "name", "message": "too short", "code": "too_short" }
    ]
  }
}
```

Each entry in `details` has:

| Key       | Type   | Description                                         |
|-----------|--------|-----------------------------------------------------|
| `field`   | string | The field that failed validation.                   |
| `message` | string | Human-readable description.                         |
| `code`    | string | Machine-readable code (e.g. `required`, `too_short`). |

For all other error codes, `details` is `null`.

---

## Query parameters

All list endpoints accept the parameters below. Which filters and search fields
are available depends on the resource file for that endpoint.

All URLs are prefixed with `/v{version}` based on the resource's `version`
field. The examples below use `/v1` — adjust the prefix if your resource uses a
different version number.

### Filtering

Use bracket syntax on the `filter` key. Only fields declared in the endpoint's
`filters` list are accepted; bracket-form keys for fields not in `filters` are
silently dropped.

```
GET /v1/users?filter[role]=admin&filter[org_id]=550e8400-e29b-41d4-a716-446655440000
```

Filters produce exact-match `WHERE` clauses (`field = value`).

**Bare-field params are rejected.** If you send `?role=admin` (without the
`filter[...]` wrapper) and `role` is declared in `filters:`, the runtime
returns **422** with a `INVALID_FILTER_FORM` error and a "did you mean
`?filter[role]=admin`?" hint. This prevents a footgun where a wrong URL
silently returns unfiltered results. Bare params that don't match any
declared filter are ignored without error (they may be application-defined
or reserved like `sort`, `after`, `limit`, `search`, `fields`, `include`).

### Sorting

Pass a comma-separated list of field names to `sort`. Prefix a field with `-`
for descending order; no prefix means ascending.

```
GET /v1/users?sort=-created_at,name
```

This sorts by `created_at DESC`, then `name ASC`. Only fields declared in the
endpoint's `sort` list are accepted.

### Search

Pass a plain-text term to `search`. The server performs a PostgreSQL full-text
search across the fields declared in the endpoint's `search` list.

```
GET /v1/users?search=alice
```

The generated SQL uses `to_tsvector('english', ...)` and `plainto_tsquery`,
so standard English stemming applies.

### Pagination (cursor)

Cursor pagination is the default when `pagination: cursor` is set (or when no
pagination style is specified).

```
GET /v1/users?limit=10
GET /v1/users?limit=10&after=NTUwZTg0MDAtZTI5Yi00MWQ0LWE3MTYtNDQ2NjU1NDQwMDAw
```

| Parameter | Default | Range   | Description                         |
|-----------|---------|---------|-------------------------------------|
| `limit`   | 25      | 1 -- 100 | Number of records per page.        |
| `after`   | --      | --      | Opaque cursor from previous `meta.cursor`. |

### Pagination (offset)

Available when the endpoint declares `pagination: offset`.

```
GET /v1/users?limit=25&offset=0
GET /v1/users?limit=25&offset=25
```

| Parameter | Default | Range   | Description                         |
|-----------|---------|---------|-------------------------------------|
| `limit`   | 25      | 1 -- 100 | Number of records per page.        |
| `offset`  | 0       | >= 0    | Number of records to skip.          |

### Field selection

Return only specific fields by passing a comma-separated list to `fields`. If
omitted, all fields are returned.

```
GET /v1/users?fields=name,email
```

```json
{
  "data": [
    { "name": "Alice", "email": "alice@example.com" },
    { "name": "Bob", "email": "bob@example.com" }
  ],
  "meta": { "cursor": "...", "has_more": true }
}
```

### Relation loading

Request related resources with `include`. Pass a comma-separated list of
relation names declared in the resource file's `relations` block.

```
GET /v1/users/550e8400-...?include=organization
GET /v1/users?include=organization
```

### Cache bypass

Skip the server-side cache for a single request:

```
GET /v1/users?nocache=1
```

---

## Combining parameters

All query parameters can be used together:

```
GET /v1/users?filter[role]=admin&sort=-created_at&search=alice&limit=10&fields=name,email&include=organization
```
