---
title: Debugging
parent: Guides
nav_order: 10
---

# Debugging

This guide covers how to diagnose issues in a Shaperail application, from log
verbosity to CLI inspection tools.

---

## Debug logging

Shaperail uses the `tracing` crate for structured logging. Control verbosity
with the `RUST_LOG` environment variable.

### Setting log levels

```bash
# Default: info-level logs
shaperail serve

# Debug output for all Shaperail crates
RUST_LOG=debug shaperail serve

# Trace-level for the runtime, debug for codegen, info for everything else
RUST_LOG=info,shaperail_runtime=trace,shaperail_codegen=debug shaperail serve

# Only errors
RUST_LOG=error shaperail serve
```

### Per-module debugging

Target specific modules to reduce noise:

```bash
# Debug only the handler layer
RUST_LOG=info,shaperail_runtime::handlers=debug shaperail serve

# Debug only the cache layer
RUST_LOG=info,shaperail_runtime::cache=debug shaperail serve

# Debug only the job queue
RUST_LOG=info,shaperail_runtime::jobs=debug shaperail serve
```

### Reading tracing spans

At `debug` level and above, Shaperail emits span context with each log line.
A typical request produces output like:

```json
{"timestamp":"2026-03-17T10:00:00Z","level":"DEBUG","request_id":"req-a1b2c3","span":"http_request{method=GET path=/v1/users}","target":"shaperail_runtime::handlers","message":"handler started"}
{"timestamp":"2026-03-17T10:00:00Z","level":"DEBUG","request_id":"req-a1b2c3","span":"http_request{method=GET path=/v1/users} > db_query{table=users}","target":"shaperail_runtime::db","message":"SELECT * FROM users WHERE org_id = $1 LIMIT 25"}
{"timestamp":"2026-03-17T10:00:00Z","level":"DEBUG","request_id":"req-a1b2c3","span":"http_request{method=GET path=/v1/users}","target":"shaperail_runtime::handlers","message":"handler completed status=200 duration_ms=4"}
```

The `span` field shows the nesting: the `db_query` span is a child of the
`http_request` span. Use the `request_id` to correlate all log lines from a
single request.

---

## Database query debugging

### Enable sqlx query logging

Set the `sqlx` module to `debug` or `trace` to see every query:

```bash
RUST_LOG=info,sqlx=debug shaperail serve
```

At `debug` level, sqlx logs each query with its parameters:

```json
{"timestamp":"2026-03-17T10:00:01Z","level":"DEBUG","target":"sqlx::query","message":"SELECT id, email, name, role, org_id FROM users WHERE org_id = $1 ORDER BY created_at DESC LIMIT 25; rows affected: 25, rows returned: 25, elapsed: 1.204ms","parameters":["550e8400-e29b-41d4-a716-446655440000"]}
```

### Slow query detection

Set a threshold to log warnings for slow queries:

```bash
SHAPERAIL_SLOW_QUERY_MS=50 shaperail serve
```

Any query exceeding 50ms produces a warning:

```json
{"timestamp":"2026-03-17T10:00:02Z","level":"WARN","request_id":"req-d4e5f6","target":"shaperail_runtime::db","message":"slow query: 87ms","sql":"SELECT * FROM orders WHERE user_id = $1 AND status = $2 ORDER BY created_at DESC","parameters":["...","pending"]}
```

You can also set this in `shaperail.config.yaml`:

```yaml
logging:
  slow_query_ms: 100
```

The environment variable takes precedence over the config file value.

---

## Redis debugging

### Inspecting cache state

Use `redis-cli` to inspect cached keys:

```bash
# List all Shaperail cache keys
redis-cli KEYS "shaperail:*"

# Inspect a specific key
redis-cli GET "shaperail:users:list:a1b2c3d4e5f60718:org-abc:member"

# Check TTL remaining on a key
redis-cli TTL "shaperail:users:list:a1b2c3d4e5f60718:org-abc:member"

# Delete all cache keys for a resource
redis-cli KEYS "shaperail:users:*" | xargs redis-cli DEL
```

### Checking the job queue

Job queue state is stored in Redis under `shaperail:jobs:*` keys:

```bash
# View pending jobs
redis-cli LRANGE "shaperail:jobs:normal" 0 -1

# View dead letter queue
redis-cli LRANGE "shaperail:jobs:dead" 0 -1

# Check queue depth per priority
redis-cli LLEN "shaperail:jobs:critical"
redis-cli LLEN "shaperail:jobs:high"
redis-cli LLEN "shaperail:jobs:normal"
redis-cli LLEN "shaperail:jobs:low"
```

### Debug-level Redis logging

```bash
RUST_LOG=info,shaperail_runtime::cache=debug shaperail serve
```

This logs every cache operation (get, set, delete, invalidate) with the key
name and result:

```json
{"timestamp":"2026-03-17T10:00:03Z","level":"DEBUG","request_id":"req-g7h8i9","target":"shaperail_runtime::cache","message":"cache hit","key":"shaperail:products:list:f1e2d3c4b5a60908:_:member","ttl_remaining_s":245}
```

---

## Request debugging

### Request IDs

Every incoming request is assigned a unique `request_id`. This ID appears in
every log line for that request and is returned to the client in the
`x-request-id` response header.

To trace a specific request through the logs, filter by its ID:

```bash
# Using jq to filter JSON logs
shaperail serve 2>&1 | jq 'select(.request_id == "req-a1b2c3")'
```

### Tracing headers

If your infrastructure passes a trace ID (e.g., `x-trace-id` or
`traceparent`), Shaperail propagates it into the OpenTelemetry span context.
The trace ID appears in log output when OTLP export is enabled:

```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 shaperail serve
```

### Correlating logs across services

In a multi-service workspace, the `request_id` is propagated between services
via the `x-request-id` header. Filter your log aggregator by this value to see
the full request path across services.

---

## Controller debugging

### Adding tracing to before/after hooks

One common convention is `resources/<name>.controller.rs`, but current
generated apps still require manual controller registration. Add `tracing`
instrumentation to the registered function to see when hooks fire:

```rust
use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use tracing::{debug, info};

pub async fn validate_org(ctx: &mut Context) -> ControllerResult {
    debug!(org_id = %ctx.input["org_id"], "validate_org: checking org exists");

    let org_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM organizations WHERE id = $1)"
    )
        .bind(ctx.input["org_id"].as_str().unwrap_or_default())
        .fetch_one(&ctx.pool)
        .await?
        .unwrap_or(false);

    if !org_exists {
        info!(org_id = %ctx.input["org_id"], "validate_org: org not found, rejecting");
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "org_id".into(),
            message: "organization does not exist".into(),
            code: "invalid_reference".into(),
        }]));
    }

    debug!("validate_org: passed");
    Ok(())
}
```

With `RUST_LOG=debug`, this produces:

```json
{"timestamp":"2026-03-17T10:00:04Z","level":"DEBUG","request_id":"req-j1k2l3","target":"my_app::controllers","message":"validate_org: checking org exists","org_id":"550e8400-..."}
{"timestamp":"2026-03-17T10:00:04Z","level":"DEBUG","request_id":"req-j1k2l3","target":"my_app::controllers","message":"validate_org: passed"}
```

### Common controller issues

- **Controller not running** -- verify the resource YAML declares it:
  `controller: { before: validate_org }`.
- **Controller not registered** -- run `shaperail generate`, verify the function
  is `pub` in `resources/<resource>.controller.rs`, and confirm its name exactly
  matches the YAML declaration. Generated registration appears in
  `generated/mod.rs`.
- **WASM controller not loading** -- check that the `wasm-plugins` feature is
  enabled in `Cargo.toml`. See [Troubleshooting]({{ '/troubleshooting/' | relative_url }}).

---

## Job debugging

### Inspecting failed jobs

Use the CLI to check job queue status:

```bash
# Summary view: queue depth per priority and dead letter count
shaperail jobs:status
```

Example output:

```
Job Queue Status
──────────────────────────────
Priority    Pending   Processing
critical    0         0
high        2         1
normal      15        4
low         3         0

Dead letter queue: 2 jobs
Recent failures (last 24h): 5
```

### Inspecting a specific job

```bash
shaperail jobs:status job-abc-123
```

Example output:

```
Job: job-abc-123
──────────────────────────────
Name:       send_welcome_email
Status:     failed
Priority:   normal
Created:    2026-03-17T09:55:00Z
Failed:     2026-03-17T09:55:02Z
Attempts:   3 / 3
Last error: Connection refused: smtp://localhost:587
Payload:    {"user_id":"550e8400-...","email":"alice@example.com"}
```

### Viewing the dead letter queue

Jobs that exhaust all retry attempts move to the dead letter queue. Inspect
them with Redis:

```bash
redis-cli LRANGE "shaperail:jobs:dead" 0 -1
```

Or with debug logging:

```bash
RUST_LOG=info,shaperail_runtime::jobs=debug shaperail serve
```

This logs each job pickup, completion, retry, and dead-letter event:

```json
{"timestamp":"2026-03-17T10:00:05Z","level":"DEBUG","target":"shaperail_runtime::jobs","message":"job picked up","job_id":"job-abc-123","job_name":"send_welcome_email","attempt":3}
{"timestamp":"2026-03-17T10:00:05Z","level":"WARN","target":"shaperail_runtime::jobs","message":"job failed, moved to dead letter queue","job_id":"job-abc-123","error":"Connection refused: smtp://localhost:587"}
```

---

## YAML validation debugging

### `shaperail check`

The `check` command validates resource files and provides structured error
messages with fix suggestions:

```bash
shaperail check resources/users.yaml
```

Example output:

```
resources/users.yaml
  ERROR [E001] Unknown field `cache_ttl` on endpoint `list`
    → Did you mean: cache: { ttl: 60 }
  WARN  [W003] Field `email` is used in `search` but has no index
    → Add: indexes: [{ fields: [email] }]
```

### JSON output for tooling

Use `--json` for machine-readable output (useful for editor integrations and
LLM-assisted debugging):

```bash
shaperail check --json resources/users.yaml
```

```json
{
  "file": "resources/users.yaml",
  "errors": [
    {
      "code": "E001",
      "severity": "error",
      "message": "Unknown field `cache_ttl` on endpoint `list`",
      "line": 14,
      "suggestion": "cache: { ttl: 60 }"
    }
  ],
  "warnings": [
    {
      "code": "W003",
      "severity": "warning",
      "message": "Field `email` is used in `search` but has no index",
      "line": 5,
      "suggestion": "indexes: [{ fields: [email] }]"
    }
  ]
}
```

### Validating all resources at once

```bash
shaperail check
```

This checks every file in the `resources/` directory and reports all issues in a
single pass.

---

## Using `shaperail explain`

The `explain` command dry-runs a resource file and shows what it will produce
without generating any code:

```bash
shaperail explain resources/users.yaml
```

Example output:

```
Resource: users (v1)

Table: users
  id          uuid        PRIMARY KEY, generated
  email       varchar     UNIQUE, NOT NULL
  name        varchar     NOT NULL
  role        enum        DEFAULT 'member' (admin, member, viewer)
  org_id      uuid        NOT NULL, REFERENCES organizations(id)
  created_at  timestamptz generated
  updated_at  timestamptz generated

Routes:
  GET    /v1/users      auth: [member, admin]  cache: 60s  pagination: cursor
  POST   /v1/users      auth: [admin]          controller: before=validate_org
  PATCH  /v1/users/:id  auth: [admin, owner]
  DELETE /v1/users/:id  auth: [admin]          soft_delete: true

Relations:
  organization  belongs_to  organizations  via org_id
  orders        has_many    orders         via user_id

Indexes:
  (org_id, role)
  (created_at DESC)

Events:
  user.created → send_welcome_email (job)
```

Use `explain` to verify that your resource YAML produces the routes, table
schema, and relations you expect before running `shaperail generate` or
`shaperail serve`.

---

## Using `shaperail diff`

The `diff` command shows what codegen would change without writing any files:

```bash
shaperail diff
```

Example output:

```
--- generated/mod.rs
+++ generated/mod.rs (regenerated)
18 lines changed (+9 -9)

--- generated/users.rs
+++ generated/users.rs (regenerated)
42 lines changed (+21 -21)

--- /dev/null
+++ generated/orders.rs
(new file, 118 lines)
```

This is useful for:

- Verifying that a YAML change produces the expected code change.
- Reviewing generated code before committing.
- Debugging cases where the generated code does not match expectations.

---

## Common issues and solutions

### Server starts but endpoints return 404

**Cause**: resource files are not in the `resources/` directory, or
`shaperail generate` has not been run.

**Fix**:

```bash
ls resources/       # verify YAML files exist
shaperail validate  # check for parse errors
shaperail generate  # regenerate code
shaperail serve     # restart
```

### Cache always misses

**Cause**: Redis is not running, or `REDIS_URL` is not set.

**Fix**:

```bash
docker compose ps          # check Redis is healthy
redis-cli PING             # should return PONG
echo $REDIS_URL            # should be set
```

If Redis is running but `REDIS_URL` is unset, the cache layer silently fails
open (requests proceed without caching). Set the URL:

```bash
export REDIS_URL=redis://localhost:6379
```

### Controller hook not executing

**Cause**: the function is not registered in the controller map, or the
registered name does not match the YAML.

**Fix**:

```bash
# Check the resource declares the controller
shaperail explain resources/users.yaml | grep controller

# Check the function name matches what the YAML declares
rg "pub async fn validate_org" src resources

# Check your bootstrap registers the function name
rg 'register\\("users", "validate_org"' src generated
```

### Jobs stuck in pending

**Cause**: no worker is consuming the queue, or Redis connectivity issues.

**Fix**:

```bash
shaperail jobs:status         # check queue depth
redis-cli PING                # verify Redis connectivity
RUST_LOG=debug shaperail serve  # confirm jobs are being enqueued
```

The scaffolded app does not start a job worker automatically. `shaperail serve`
can enqueue jobs, but processing requires a separately wired worker plus a job
handler registry. If you added that worker yourself, start it with
`RUST_LOG=debug` and inspect the consumer logs there.

### Generated code does not compile

**Cause**: resource YAML references a relation or field that does not exist.

**Fix**:

```bash
shaperail check --json        # get structured error report
shaperail diff                # see what codegen wants to produce
shaperail validate            # catch YAML-level issues
```

Fix the reported issues, then re-run `shaperail generate`.

### Slow startup

**Cause**: database migrations running on startup, or large number of
resources.

**Fix**:

```bash
# Run migrations separately
shaperail migrate

# Then start without migration overhead
shaperail serve
```

Check cold start time by timing the `/health` endpoint:

```bash
time curl -s http://localhost:3000/health
```

The target is under 100ms from process start to first successful health
response.
