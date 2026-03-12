---
title: Observability
parent: Guides
nav_order: 10
---

# Observability

Shaperail provides built-in structured logging, Prometheus metrics, health
checks, and OpenTelemetry distributed tracing. Everything is wired
automatically when you run `shaperail serve` -- no manual setup required.

---

## Structured logging

All log output is JSON, one object per line, produced by the `tracing` crate.
Every log line includes a `request_id` field so you can correlate entries across
a single request.

Example output:

```json
{"timestamp":"2026-03-13T12:00:00Z","level":"INFO","request_id":"abc-123","target":"shaperail_runtime::handlers","message":"GET /users 200 12ms"}
```

### Log level control

Set the `RUST_LOG` environment variable to control verbosity. The default level
is `info`.

```bash
# Show only warnings and errors
RUST_LOG=warn shaperail serve

# Debug output for the runtime crate, info for everything else
RUST_LOG=info,shaperail_runtime=debug shaperail serve

# Trace-level output (very verbose)
RUST_LOG=trace shaperail serve
```

### PII redaction

Any schema field marked `sensitive: true` is automatically redacted in all log
output. The value is replaced with `"[REDACTED]"` before it reaches the log
layer. Redaction applies recursively to nested objects and arrays.

```yaml
schema:
  email:    { type: string, format: email, sensitive: true }
  password: { type: string, sensitive: true }
```

With the schema above, any log line that would include `email` or `password`
values will show `"[REDACTED]"` instead.

### Slow query logging

Set the `SHAPERAIL_SLOW_QUERY_MS` environment variable to log a warning for any
database query that exceeds the given threshold in milliseconds.

```bash
# Warn on queries slower than 50ms
SHAPERAIL_SLOW_QUERY_MS=50 shaperail serve
```

---

## Health endpoints

Two health check endpoints are registered automatically.

### `GET /health` -- shallow

Returns `200 OK` if the process is running. Does not check any dependencies.

```json
{ "status": "ok" }
```

Use this for liveness probes in Kubernetes or any orchestrator that just needs
to know the process is alive.

### `GET /health/ready` -- deep

Checks database (Postgres) and cache (Redis) connectivity. Returns `200 OK` if
all checks pass, or `503 Service Unavailable` if any check fails.

Healthy response:

```json
{
  "status": "ok",
  "checks": {
    "database": { "status": "ok" },
    "redis": { "status": "ok" }
  }
}
```

Degraded response (503):

```json
{
  "status": "degraded",
  "checks": {
    "database": { "status": "ok" },
    "redis": { "status": "error", "message": "Redis PING failed: ..." }
  }
}
```

Use this for readiness probes. Route traffic to the instance only when
`/health/ready` returns 200.

---

## Prometheus metrics

Metrics are exposed in Prometheus text format at `GET /metrics`.

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `shaperail_http_requests_total` | counter | `method`, `path`, `status` | Total HTTP requests |
| `shaperail_http_request_duration_seconds` | histogram | `method`, `path` | Request duration in seconds |
| `shaperail_db_pool_size` | gauge | -- | Current DB connection pool size |
| `shaperail_cache_total` | counter | `result` (`hit` / `miss`) | Cache operations |
| `shaperail_job_queue_depth` | gauge | -- | Current job queue depth |
| `shaperail_errors_total` | counter | `error_type` | Total errors by type |

The histogram uses the following bucket boundaries (in seconds):
`0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0`.

### Prometheus scrape config

```yaml
# prometheus.yml
scrape_configs:
  - job_name: shaperail
    scrape_interval: 15s
    static_configs:
      - targets: ["localhost:8080"]
```

---

## OpenTelemetry tracing

Shaperail creates spans for four categories of operations:

- **HTTP requests** -- one span per incoming request
- **Database queries** -- `db.query` spans with `db.operation`, `db.table`, and `db.statement` attributes
- **Cache operations** -- `cache.op` spans with `cache.operation` and `cache.key` attributes
- **Job execution** -- `job.execute` spans with `job.name` and `job.id` attributes

### OTLP export configuration

Telemetry is opt-in. If `OTEL_EXPORTER_OTLP_ENDPOINT` is not set, tracing is
disabled (no-op) and adds zero overhead.

```bash
# Enable OTLP export (gRPC)
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 shaperail serve

# Set a custom service name (defaults to "shaperail")
OTEL_SERVICE_NAME=my-app shaperail serve
```

The exporter uses gRPC via Tonic. Point it at any OTLP-compatible collector
(Jaeger, Grafana Tempo, Honeycomb, Datadog Agent, etc.).

On shutdown, Shaperail flushes all pending spans before the process exits.

---

## Configuration in `shaperail.config.yaml`

You can also control logging behavior in the project configuration file:

```yaml
logging:
  level: info           # default log level (overridden by RUST_LOG)
  format: json          # always JSON; included for explicitness
  slow_query_ms: 100    # slow query threshold (overridden by SHAPERAIL_SLOW_QUERY_MS)
```

Environment variables take precedence over config file values.

---

## Environment variable summary

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level filter |
| `SHAPERAIL_SLOW_QUERY_MS` | none | Slow query warning threshold (ms) |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | none | OTLP gRPC endpoint; unset disables tracing |
| `OTEL_SERVICE_NAME` | `shaperail` | Service name reported in spans |
