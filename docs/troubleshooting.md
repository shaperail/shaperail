---
title: Troubleshooting
parent: Reference
nav_order: 6
---

Common errors and how to fix them.

## YAML parsing errors

### Unknown field

```
unknown field `cache_ttl`, expected one of `method`, `path`, `auth`, ...
```

You used a field name that doesn't exist. The error lists all valid fields.
Common mistakes:

| Wrong | Correct |
| --- | --- |
| `cache_ttl: 60` | `cache: { ttl: 60 }` |
| `hooks: [validate]` | `controller: { before: validate }` |
| `type: str` | `type: string` |
| `method: get` | `method: GET` |
| `name: users` | `resource: users` |
| `fields:` | `schema:` |
| `routes:` | `endpoints:` |

### Missing field

```
missing field `resource`
```

A required top-level key is missing. Every resource file must have `resource`,
`version`, and `schema`.

### Wrong type

```
invalid value: string "get", expected one of `GET`, `POST`, `PATCH`, `PUT`, `DELETE`
```

Enum values are case-sensitive. Use uppercase HTTP methods.

### Invalid YAML syntax

```
while parsing a block mapping ... did not find expected key
```

This usually means incorrect indentation or a missing colon. Check:
- Consistent indentation (2 spaces, no tabs)
- Colons after every key
- Proper flow syntax: `{ type: string }` not `{type: string}` (space after `{`)

### Duplicate keys

```
duplicate key `email` in schema
```

Each field name must be unique within `schema`. Check for copy-paste errors.

### Invalid field type

```
unknown type `varchar`, expected one of `uuid`, `string`, `integer`, `float`, `boolean`, `timestamp`, `date`, `json`, `enum`
```

Use the Shaperail type names, not SQL types. The codegen layer handles the
mapping to SQL types.

### Invalid enum values

```
enum field `status` must have a `values` array
```

Every field with `type: enum` must include a `values` array:

```yaml
status: { type: enum, values: [draft, published, archived], default: draft }
```

### Invalid ref format

```
invalid ref format `organizations`, expected `resource.field`
```

The `ref` field must use dot notation: `ref: organizations.id`.

### Min greater than max

```
field `name`: min (200) must be less than or equal to max (100)
```

Check that `min` and `max` values are in the correct order.

## Database connection errors

### Cannot connect to Postgres

```
error communicating with database: Connection refused (os error 111)
```

1. Check Postgres is running: `docker compose ps`
2. Verify `DATABASE_URL` in `.env` matches the Compose port mapping
3. Test connectivity: `psql "$DATABASE_URL" -c "SELECT 1"`
4. If using Docker, make sure the container is healthy:
   `docker compose logs postgres`

### Connection pool exhausted

```
error: timed out waiting for connection from pool
```

Too many concurrent requests for the pool size. Increase the pool in
`shaperail.config.yaml`:

```yaml
databases:
  default:
    engine: postgres
    url: ${DATABASE_URL}
    pool_size: 20   # default is 10
```

Or reduce the number of Actix workers so each worker gets enough connections.

### SSL required but not configured

```
error: SSL connection is required
```

Your production Postgres requires SSL. Update the connection URL:

```text
DATABASE_URL=postgresql://user:pass@host:5432/db?sslmode=require
```

### Migration version mismatch

```
error: migration 0003 has already been applied but the file contents differ
```

Someone changed an already-applied migration file. Never edit migration files
after they have been applied. Write a new migration to make the correction.

### Relation does not exist

```
ERROR: relation "users" does not exist
```

The migration that creates the table has not been applied yet. Run:

```bash
shaperail serve   # applies pending migrations on startup
```

Or apply manually:

```bash
DATABASE_URL=postgresql://user:pass@host:5432/db shaperail migrate
```

## JWT and auth errors

### Token expired

```
{ "error": "token_expired", "message": "JWT has expired" }
```

The JWT `exp` claim is in the past. Generate a new token.

### Invalid signature

```
{ "error": "invalid_token", "message": "Invalid JWT signature" }
```

The token was signed with a different `JWT_SECRET` than the server is using.
Check that `JWT_SECRET` in `.env` matches the secret used to sign the token.

### Missing auth header

```
{ "error": "unauthorized", "message": "Missing Authorization header" }
```

The endpoint requires auth. Send the header:

```
Authorization: Bearer <token>
```

### Insufficient permissions

```
{ "error": "forbidden", "message": "Role 'viewer' is not authorized for this endpoint" }
```

The JWT role claim does not match the `auth` array on the endpoint. Check the
resource YAML:

```yaml
endpoints:
  create:
    auth: [admin]   # only admin can create
```

### Owner check failed

```
{ "error": "forbidden", "message": "Not the resource owner" }
```

The endpoint uses `owner` in its `auth` array, and the requesting user's ID
does not match the resource's owner field. This is expected behavior for
ownership-based access control.

## Migration errors

### sqlx-cli not installed

```
error: shaperail migrate requires sqlx-cli
```

Install it:

```bash
cargo install sqlx-cli
```

### Migration file syntax error

```
ERROR: syntax error at or near "ALTERR"
```

The generated SQL has a typo (possibly from a manual edit). Fix the SQL file in
`migrations/` and re-run.

### Cannot add NOT NULL column without default

```
ERROR: column "status" of relation "users" contains null values
```

You added a `required: true` field without a `default` to a table that already
has rows. Fix by adding a default to the schema or manually editing the
migration to backfill data first. See
[Migrations guide]({{ '/migrations-and-schema-changes/' | relative_url }}).

### Foreign key constraint violation during migration

```
ERROR: insert or update on table "posts" violates foreign key constraint
```

A migration added a foreign key to a column that has values not present in the
referenced table. Backfill or clean the data before adding the constraint.

## Job queue issues

### Jobs not running

Check the following in order:

1. **Redis is running:** `docker compose ps` shows Redis healthy
2. **REDIS_URL is correct:** matches the Compose port mapping in `.env`
3. **Jobs are declared:** the endpoint has a `jobs:` array

```yaml
endpoints:
  create:
    jobs: [send_welcome_email]
```

4. **A worker is actually running:** the scaffold does not start a job worker
   automatically
5. **A matching handler is registered:** the worker's `JobRegistry` includes
   `send_welcome_email`

### Jobs stuck in pending

```bash
shaperail jobs:status
```

If jobs stay in `pending`:
- No worker may have been started at all. The default scaffold only enqueues.
- The worker may have crashed. Check application logs.
- Redis may be unreachable. Test with `redis-cli ping`.
- The job may be scheduled for a future time.

### Jobs failing repeatedly

Jobs that fail are retried according to their retry policy. After exhausting
retries, they move to the dead letter queue. Check:

```bash
shaperail jobs:status
```

There is no built-in `jobs:retry` command today. Fix the handler or dependency
problem, then re-enqueue the job from the application path that created it or
manually inspect the Redis dead-letter payload.

### Job timeout

```
error: job `process_report` exceeded timeout of 30s
```

Increase the timeout in the resource YAML or optimize the job handler. Long
running jobs should be broken into smaller steps.

## Cache issues

### Cache not working (stale responses)

1. Verify Redis is running
2. Check the endpoint declares cache:

```yaml
endpoints:
  list:
    cache: { ttl: 60 }
```

3. Check `REDIS_URL` is set correctly in `.env`

### Cache not invalidating

By default, cache is invalidated when the same resource is mutated (create,
update, delete). If you need cross-resource invalidation, use `invalidate_on`:

```yaml
cache: { ttl: 60, invalidate_on: [create, update, delete] }
```

### Redis out of memory

```
OOM command not allowed when used memory > 'maxmemory'
```

Configure Redis eviction policy or increase memory:

```bash
redis-cli CONFIG SET maxmemory-policy allkeys-lru
redis-cli CONFIG SET maxmemory 256mb
```

## Generated code issues

### Files in `generated/` have compile errors

Files in `generated/` are overwritten on every `shaperail generate` and
`shaperail serve`. Never edit them by hand. If you see compile errors:

1. Run `shaperail validate` to check your resource YAML
2. Run `shaperail generate` to regenerate
3. If errors persist, check that your `Cargo.toml` has the correct
   `shaperail-runtime` version

### Controller not found

```
error: controller function `validate_org` not found
```

The resource declares a controller, but the function is not registered in the
controller map or the signature does not match. Create a controller module with
the expected signature and register it with `ControllerMap::register(...)`:

```rust
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

pub async fn validate_org(ctx: &mut Context) -> ControllerResult {
    // your logic here
    Ok(())
}
```

### WASM plugin not loading

```
error: failed to load WASM plugin: plugins/hook.wasm
```

1. Check the file exists at the specified path
2. Check the `wasm-plugins` feature is enabled (see Feature flags below)
3. Rebuild the WASM module if it was compiled for a different target

## Feature flag mismatches

### GraphQL / gRPC not working

If you added `protocols: [graphql]` to your config but the endpoint doesn't
appear, check your `Cargo.toml`:

```toml
# This won't work -- graphql feature is not enabled:
shaperail-runtime = { version = "0.7.0", default-features = false }

# This will:
shaperail-runtime = { version = "0.7.0", default-features = false, features = ["graphql"] }
```

### WASM plugins silently ignored

If your resource declares `controller: { before: "wasm:plugins/hook.wasm" }`
but the hook doesn't run, enable the feature:

```toml
shaperail-runtime = { version = "0.7.0", default-features = false, features = ["wasm-plugins"] }
```

Without the feature, WASM prefixed controllers return an error at runtime.

### Multi-database not working

If you enable the `multi-db` feature and route resources through named
connections, remember that the generated bootstrap only wires SQL engines
(`postgres`, `mysql`, `sqlite`) automatically:

```toml
shaperail-runtime = { version = "0.7.0", features = ["multi-db"] }
```

If you configure a named database with `engine: mongodb`, the runtime has
Mongo-backed store primitives behind the feature flag, but the scaffolded app
does not build the mixed SQL + Mongo store registry for you yet.

Use named SQL connections out of the box, or add manual bootstrap code for
Mongo-backed stores.

## Available features

| Feature | What it enables |
| --- | --- |
| `graphql` | `POST /graphql` endpoint via async-graphql |
| `grpc` | gRPC server via tonic on a separate port |
| `wasm-plugins` | WASM controller hooks via wasmtime |
| `multi-db` | Named multi-database runtime support; scaffolded apps wire SQL engines automatically |
| `observability-otlp` | OpenTelemetry OTLP span export |

All features are enabled by default when you don't specify `default-features = false`.

## Performance issues

### High latency on list endpoints

1. Check if the endpoint has appropriate indexes:

```yaml
indexes:
  - { fields: [org_id, status] }   # matches your common filter
```

2. Enable caching for read-heavy endpoints:

```yaml
cache: { ttl: 60 }
```

3. Check the database with `EXPLAIN ANALYZE` on the generated query

### High memory usage

1. Check Actix worker count -- each worker uses memory. Reduce with:

```yaml
# shaperail.config.yaml
workers: 4   # default is number of CPU cores
```

2. Check for unbounded query results -- always use pagination
3. Monitor with `shaperail doctor` for common misconfigurations

### Slow cold start

Shaperail targets cold start under 100ms. If startup is slow:
1. Check migration count -- many migrations take time to verify
2. Check database connectivity -- DNS resolution or TLS handshake may be slow
3. Run `shaperail doctor` to identify startup bottlenecks

## Connection errors (quick reference)

| Problem | Fix |
| --- | --- |
| Cannot connect to Postgres | Run `docker compose ps`, confirm the service is healthy |
| Cannot connect to Redis | Same -- check `docker compose ps` and `.env` `REDIS_URL` |
| Port already in use | Change the port in `docker-compose.yml` and update `.env` |
| `shaperail migrate` fails | Install `sqlx-cli`: `cargo install sqlx-cli` |
