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

### Single record (GET /resources/:id, PATCH /resources/:id)

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

### List (GET /resources)

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

### Bulk operations

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

### Delete (DELETE /resources/:id)

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

### Filtering

Use bracket syntax on the `filter` key. Only fields declared in the endpoint's
`filters` list are accepted; all others are silently ignored.

```
GET /users?filter[role]=admin&filter[org_id]=550e8400-e29b-41d4-a716-446655440000
```

Filters produce exact-match `WHERE` clauses (`field = value`).

### Sorting

Pass a comma-separated list of field names to `sort`. Prefix a field with `-`
for descending order; no prefix means ascending.

```
GET /users?sort=-created_at,name
```

This sorts by `created_at DESC`, then `name ASC`. Only fields declared in the
endpoint's `sort` list are accepted.

### Search

Pass a plain-text term to `search`. The server performs a PostgreSQL full-text
search across the fields declared in the endpoint's `search` list.

```
GET /users?search=alice
```

The generated SQL uses `to_tsvector('english', ...)` and `plainto_tsquery`,
so standard English stemming applies.

### Pagination (cursor)

Cursor pagination is the default when `pagination: cursor` is set (or when no
pagination style is specified).

```
GET /users?limit=10
GET /users?limit=10&after=NTUwZTg0MDAtZTI5Yi00MWQ0LWE3MTYtNDQ2NjU1NDQwMDAw
```

| Parameter | Default | Range   | Description                         |
|-----------|---------|---------|-------------------------------------|
| `limit`   | 25      | 1 -- 100 | Number of records per page.        |
| `after`   | --      | --      | Opaque cursor from previous `meta.cursor`. |

### Pagination (offset)

Available when the endpoint declares `pagination: offset`.

```
GET /users?limit=25&offset=0
GET /users?limit=25&offset=25
```

| Parameter | Default | Range   | Description                         |
|-----------|---------|---------|-------------------------------------|
| `limit`   | 25      | 1 -- 100 | Number of records per page.        |
| `offset`  | 0       | >= 0    | Number of records to skip.          |

### Field selection

Return only specific fields by passing a comma-separated list to `fields`. If
omitted, all fields are returned.

```
GET /users?fields=name,email
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
GET /users/550e8400-...?include=organization
GET /users?include=organization
```

### Cache bypass

Skip the server-side cache for a single request:

```
GET /users?nocache=1
```

---

## Combining parameters

All query parameters can be used together:

```
GET /users?filter[role]=admin&sort=-created_at&search=alice&limit=10&fields=name,email&include=organization
```
