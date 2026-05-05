# Recipe: Paginated List with Filters

## WHEN to use this

Use this recipe when you need a **list endpoint that is genuinely browseable**:
- The resource has more rows than fit in a single response (>100 expected rows in production).
- Callers need to filter by foreign-key or enum values (e.g. "all orders for customer X in status Y").
- The list result should be cached in Redis and invalidated automatically on any write.

This recipe shows cursor pagination, multi-column filtering, sort, and cache-with-invalidate-on-write — all using existing Shaperail primitives. No custom code is required.

## What this gives you

```
GET /v1/orders                                  → list (member, admin)
GET /v1/orders/:id                              → get (member, admin)
POST /v1/orders                                 → create (admin, member)
PATCH /v1/orders/:id                            → update (admin)
DELETE /v1/orders/:id                           → soft delete (admin)
```

### Cursor pagination

Declare `pagination: cursor` on the list endpoint. The runtime returns a `next_cursor` field in the response envelope. Callers pass `?cursor=<value>` on the next request. There is no page number, no total count — both are expensive on large tables.

### Multi-column filtering

```yaml
filters: [status, customer_id]
```

The runtime exposes `?filter[status]=shipped&filter[customer_id]=<uuid>`. Filters combine with AND. Attempting `?status=shipped` (without the bracket form) returns 422 `INVALID_FILTER_FORM`.

### Sort

```yaml
sort: [placed_at, total_cents]
```

Callers pass `?sort=placed_at` or `?sort=-placed_at` (descending). The runtime validates against the declared list; unknown sort fields are rejected.

### Cache with invalidate-on-write

```yaml
cache: { ttl: 60, invalidate_on: [create, update, delete] }
```

Redis caches the list response for 60 seconds. Any write (create/update/delete) to the `orders` resource clears the cache key automatically. Requires Redis to be configured; silently passes through on cache miss if Redis is unavailable.

## When NOT to use this

- **Single-row lookups**: use `get` only; list + cursor pagination has overhead.
- **Aggregates / summaries**: cursor pagination returns rows, not aggregates. For `SUM(total_cents)` style queries, use a custom handler.
- **Realtime feeds**: cursor-paginated lists are not a substitute for webhooks or SSE; the cache TTL adds latency to fresh rows.
- **Very large exports** (>50k rows): use a background job + file export instead of paginating through the list endpoint.

## Key design notes for LLM authors

1. The `cache.invalidate_on` array must reference the action names that write to the same table. For orders that is `[create, update, delete]`.
2. `soft_delete: true` on `delete` sets `deleted_at` — the list endpoint filters them out automatically.
3. `total_cents` is typed `integer` (i64) — Shaperail removed `bigint` in v0.13; use `integer` for all 64-bit signed values.
4. `currency` has a `default: USD` — the runtime injects this if the field is omitted from the input.
5. Do not add `total_cents` and `currency` to `update.input` if you do not want callers to change the order value after creation. The example intentionally omits `currency` from update.
