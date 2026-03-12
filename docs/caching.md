---
title: Caching
parent: Guides
nav_order: 5
---

Shaperail caches GET endpoint responses in Redis. You declare caching in the
resource YAML and the framework handles key generation, storage, and
invalidation. Write endpoints (POST, PATCH, PUT, DELETE) are never cached.

## Declaring cache on an endpoint

Add a `cache` block to any GET endpoint:

```yaml
endpoints:
  list:
    method: GET
    path: /users
    auth: [member, admin]
    pagination: cursor
    cache: { ttl: 60 }
```

`ttl` is the time-to-live in seconds. This is the only required field.

## Cache key format

Every cached response is stored under a key with this structure:

```
shaperail:<resource>:<endpoint>:<query_hash>:<user_role>
```

For example, a `GET /users?filter[role]=admin` request made by a user with
the `member` role on the `users` resource's `list` endpoint produces a key
like:

```
shaperail:users:list:a1b2c3d4e5f60718:member
```

The `query_hash` is a truncated SHA-256 of the sorted query parameters. This
means the same filters in any order produce the same cache key. Different
roles see separate cached responses.

## Auto-invalidation

By default, any write to a resource invalidates all cached keys for that
resource. When a POST, PATCH, PUT, or DELETE handler completes successfully,
the framework deletes every key matching `shaperail:<resource>:*`.

This means you get correct cache behavior without any extra configuration.

## Selective invalidation with `invalidate_on`

If you want finer control, use `invalidate_on` to list which actions should
clear the cache:

```yaml
endpoints:
  list:
    method: GET
    path: /products
    auth: [member, admin]
    pagination: cursor
    cache:
      ttl: 300
      invalidate_on: [create, delete]
```

With this configuration, only `create` and `delete` operations on the
`products` resource clear the cache. A `PATCH` (update) will not trigger
invalidation.

If `invalidate_on` is omitted, all writes invalidate. If it is present, only
the listed actions do.

## Cache bypass

Two mechanisms skip the cache and fetch a fresh response:

1. **Query parameter** -- append `?nocache=1` to any cached GET endpoint.
2. **Admin role** -- requests authenticated with the `admin` role bypass the
   cache automatically.

In both cases the response is still stored back into the cache so subsequent
requests benefit from the fresh value.

## Redis setup

Shaperail connects to Redis using the `REDIS_URL` environment variable:

```bash
export REDIS_URL=redis://localhost:6379
```

The URL follows standard Redis URI format: `redis://host:port/db`.

You can also set it in `shaperail.config.yaml`:

```yaml
redis:
  url: redis://localhost:6379
```

For local development, `docker compose up -d` starts both Postgres and Redis.

The framework uses `deadpool-redis` for connection pooling. If Redis is
unavailable, cache operations fail open -- requests proceed without caching
and no errors are returned to the client.

## Full example

A resource file with caching on the list endpoint and selective invalidation:

```yaml
resource: products
version: 1

schema:
  id:          { type: uuid, primary: true, generated: true }
  name:        { type: string, min: 1, max: 200, required: true }
  price:       { type: integer, required: true }
  category:    { type: string, required: true }
  created_at:  { type: timestamp, generated: true }
  updated_at:  { type: timestamp, generated: true }

endpoints:
  list:
    method: GET
    path: /products
    auth: [member, admin]
    filters: [category]
    search: [name]
    pagination: cursor
    cache:
      ttl: 300
      invalidate_on: [create, delete]

  get:
    method: GET
    path: /products/:id
    auth: [member, admin]
    cache: { ttl: 120 }

  create:
    method: POST
    path: /products
    auth: [admin]
    input: [name, price, category]

  update:
    method: PATCH
    path: /products/:id
    auth: [admin]
    input: [name, price, category]

  delete:
    method: DELETE
    path: /products/:id
    auth: [admin]
```

In this setup:

- `list` caches for 5 minutes and only invalidates on create or delete.
- `get` caches for 2 minutes and invalidates on any write (default behavior).
- `create`, `update`, and `delete` are never cached.

## What is NOT cached

Shaperail only caches GET endpoints that declare a `cache` block. The
following are never cached:

- **POST endpoints** -- create operations always hit the database.
- **PATCH / PUT endpoints** -- updates always hit the database.
- **DELETE endpoints** -- deletes always hit the database.
- **GET endpoints without `cache`** -- if you omit the `cache` block, the
  endpoint returns a fresh response on every request.
