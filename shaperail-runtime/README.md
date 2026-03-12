# shaperail-runtime

The Actix-web runtime for [Shaperail](https://github.com/shaperail/shaperail) — handles everything from HTTP to database to background jobs.

## Modules

| Module | Purpose |
|--------|---------|
| `db` | PostgreSQL connection pool, query generation, migrations, filtering, sorting, pagination, search |
| `handlers` | Actix-web route registration, CRUD handlers, response envelopes, field selection, relation loading |
| `auth` | JWT middleware, RBAC enforcement, API key auth, rate limiting, token issuance |
| `cache` | Redis connection pool, response caching, automatic invalidation |
| `jobs` | Redis-backed job queue, priority queues, worker, retry with backoff, dead letter queue |
| `ws` | WebSocket sessions, room subscriptions, Redis pub/sub for multi-instance broadcast |
| `storage` | File storage backends (local, S3, GCS, Azure), upload handling, image processing, signed URLs |
| `events` | Event emitter, outbound webhooks with HMAC signing, event log, inbound webhook verification |
| `observability` | Structured logging, Prometheus metrics, OpenTelemetry tracing, health checks |

## Usage

This crate is used by generated Shaperail applications. You typically don't import it directly — the `shaperail generate` command produces code that uses it.

```toml
[dependencies]
shaperail-runtime = "0.2"
```

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE).
