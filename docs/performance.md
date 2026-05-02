---
title: Performance tuning
parent: Guides
nav_order: 9
---

# Performance tuning

Shaperail is built on Actix-web and Tokio, which provide a high-performance
async foundation. This guide covers the framework's performance targets, how to
measure your app against them, and practical tuning strategies.

---

## Performance targets

These targets are mandated by the Shaperail PRD and must pass benchmarks before
any release:

| Metric | Target | How to measure |
| --- | --- | --- |
| Simple JSON response | 150,000+ req/s | `cargo bench -p shaperail-runtime` |
| DB read (cached) | 80,000+ req/s, P99 < 2ms | Bench with Redis running |
| DB write | 20,000+ req/s, P99 < 10ms | Bench with Postgres running |
| Idle memory | <= 60 MB | `ps -o rss` on a running instance |
| Release binary size | < 20 MB | `ls -lh target/release/shaperail` |
| Cold start | < 100ms | Time from process start to first `/health` 200 |

If your app falls short of these numbers, the sections below cover the most
common causes and fixes.

---

## Database optimization

### Connection pool sizing

The `pool_size` setting in `shaperail.config.yaml` controls the maximum number
of connections in the sqlx pool:

```yaml
databases:
  default:
    engine: postgres
    url: ${DATABASE_URL:postgresql://localhost/my_db}
    pool_size: 20
```

Guidelines for sizing:

- **Start with `pool_size: 20`** (the default). This handles most workloads.
- **CPU-bound apps** -- keep the pool close to the number of CPU cores. Extra
  connections sit idle and waste Postgres memory.
- **IO-bound apps** (many concurrent slow queries) -- increase to 2-4x the core
  count, but never exceed `max_connections` on your Postgres server.
- **Multi-database setups** -- each named database in `databases:` has its own
  pool. Size each independently based on its query load.

A pool that is too large wastes Postgres memory (~10 MB per connection). A pool
that is too small causes requests to queue waiting for a free connection.

### Indexes

Declare indexes in the resource YAML for new resources, and mirror them in
manual follow-up SQL when you add indexes to an existing table:

```yaml
indexes:
  - fields: [org_id, role]
  - fields: [created_at], order: desc
```

When to add indexes:

- **Filter fields** -- every field listed in `filters:` on an endpoint should be
  indexed, either individually or as part of a composite index.
- **Search fields** -- fields in `search:` often need database-specific tuning.
  Shaperail uses PostgreSQL full-text search clauses, but it does not
  auto-generate GIN/trigram indexes for you.
- **Sort fields** -- if you sort by `created_at` descending, an index with
  `order: desc` avoids a sequential scan plus sort.
- **Foreign keys** -- any `ref:` field (e.g., `org_id: { type: uuid, ref: organizations.id }`)
  should be indexed if you filter or join on it frequently. Declare those
  indexes explicitly under `indexes:`.

When NOT to add indexes:

- Tables with fewer than a few thousand rows. Postgres will seq-scan them
  regardless.
- Write-heavy tables where every insert must update many indexes. Each index
  adds overhead to writes.

### Query analysis

Enable slow query logging to find expensive queries:

```bash
SHAPERAIL_SLOW_QUERY_MS=50 shaperail serve
```

Any query exceeding 50ms will produce a warning in the log output with the full
SQL statement. Use `EXPLAIN ANALYZE` in `psql` to inspect the query plan:

```sql
EXPLAIN ANALYZE SELECT * FROM users WHERE org_id = '...' AND role = 'admin';
```

Look for:

- **Seq Scan** on large tables -- add an index on the filtered columns.
- **Sort** with high cost -- add an index with the correct `order`.
- **Nested Loop** with many rows -- check that join columns are indexed.

---

## Caching strategies

### TTL tuning

The `cache: { ttl: N }` value on GET endpoints controls how long responses stay
in Redis:

```yaml
endpoints:
  list:
    method: GET
    path: /products
    cache: { ttl: 300 }
```

TTL guidelines:

| Data pattern | Suggested TTL | Rationale |
| --- | --- | --- |
| Rarely changes (categories, config) | 300-3600s | Low invalidation rate, high cache hit ratio |
| Changes a few times per hour (listings) | 60-300s | Balance between freshness and hit ratio |
| Changes frequently (dashboards, feeds) | 10-30s | Short enough to feel fresh, still offloads DB |
| User-specific or real-time data | No cache | Omit the `cache` block entirely |

### Invalidation patterns

Shaperail uses **cache-aside with auto-invalidation**. The flow is:

1. GET request arrives. Framework checks Redis for a cached response.
2. Cache hit: return the cached response (no DB query).
3. Cache miss: query the database, store the result in Redis, return to client.
4. Any write (POST/PATCH/DELETE) deletes all cache keys for that resource.

This is the only caching pattern Shaperail supports. There is no write-through
or write-behind mode. The design is intentional: one canonical pattern means
fewer bugs and predictable invalidation.

For finer control, use `invalidate_on` to limit which write operations clear the
cache:

```yaml
cache:
  ttl: 300
  invalidate_on: [create, delete]
```

With this configuration, PATCH (update) operations do not invalidate the cache.
Use this when updates are frequent but the cached list view does not need to
reflect every change immediately.

### Monitoring cache effectiveness

Check the `shaperail_cache_total` Prometheus metric at `GET /metrics`:

```
shaperail_cache_total{result="hit"} 12450
shaperail_cache_total{result="miss"} 830
```

A healthy cache hit ratio for list endpoints is above 80%. If the miss rate is
high, either the TTL is too short or writes are invalidating too aggressively.

---

## Pagination

Shaperail supports two pagination strategies: `cursor` and `offset`.

### Cursor pagination

```yaml
pagination: cursor
```

Performance characteristics:

- **Constant time** regardless of page depth. Page 1 and page 1000 have the
  same query cost.
- Uses an indexed column (typically `created_at` + `id`) as the cursor.
- Best for infinite scroll, feeds, and any endpoint where users page through
  large datasets.

### Offset pagination

```yaml
pagination: offset
```

Performance characteristics:

- **Linear degradation** with depth. `OFFSET 10000` requires Postgres to scan
  and discard 10,000 rows before returning the page.
- Simpler for clients that need "go to page N" behavior.
- Acceptable for small datasets (under ~10,000 rows) or when users rarely go
  past page 5.

### When to use which

| Use case | Recommendation |
| --- | --- |
| API consumed by mobile/SPA with infinite scroll | `cursor` |
| Admin dashboard with "page 1 of 50" UI | `offset` (if dataset is small) |
| Public API with unknown access patterns | `cursor` (safest default) |
| Reports or exports | `cursor` (datasets are often large) |

If in doubt, use `cursor`. It performs well in all cases.

---

## Worker count tuning

The `workers` setting controls the number of Actix-web worker threads:

```yaml
workers: auto
```

### Auto mode (default)

`auto` sets the worker count to the number of logical CPU cores. This is correct
for most workloads because Shaperail handlers are async and non-blocking.

### Fixed worker count

Set a fixed number when you need predictable resource usage:

```yaml
workers: 4
```

Guidelines:

- **CPU-bound workloads** (heavy JSON serialization, complex validation) --
  match the core count. More workers than cores causes contention.
- **IO-bound workloads** (most CRUD apps waiting on Postgres/Redis) -- the
  default `auto` is optimal. Tokio handles thousands of concurrent connections
  per worker thread.
- **Memory-constrained environments** (small containers) -- reduce workers to
  lower memory usage. Each worker thread adds ~5-10 MB.

Do not set workers higher than your CPU core count unless you have measured a
specific benefit. Extra threads add scheduling overhead without improving
throughput for async workloads.

---

## Benchmarking

### Running benchmarks

Shaperail includes Criterion benchmarks in the runtime crate:

```bash
cargo bench -p shaperail-runtime
```

This runs without a database or Redis connection. The benchmarks measure raw
handler throughput, serialization speed, and routing overhead.

Results are written to `target/criterion/` and include HTML reports with
statistical analysis.

### Interpreting results

Criterion reports look like this:

```
simple_json_response    time:   [6.21 us 6.28 us 6.35 us]
                        thrpt:  [157.48 Kreq/s 159.24 Kreq/s 161.03 Kreq/s]
```

Key values:

- **time** -- the [lower bound, estimate, upper bound] for a single request.
- **thrpt** -- throughput in thousands of requests per second. This is the
  inverse of time.

Compare against the targets:

| Benchmark | Target | What to check if below target |
| --- | --- | --- |
| `simple_json_response` | 150K req/s | Check that you built with `--release` |
| `cached_db_read` | 80K req/s | Check Redis connectivity and pool size |
| `db_write` | 20K req/s | Check Postgres pool size and index overhead |

### Load testing a running server

For end-to-end benchmarks with a live database, use a tool like `wrk` or `oha`:

```bash
# Start the server in release mode
cargo run --release -p shaperail-cli -- serve

# In another terminal, run a load test
wrk -t4 -c100 -d30s http://localhost:3000/v1/health
```

For endpoint-specific tests:

```bash
wrk -t4 -c100 -d30s -H "Authorization: Bearer <token>" \
  http://localhost:3000/v1/users
```

---

## Common performance antipatterns

### 1. Missing indexes on filter columns

**Symptom**: list endpoints slow down as the table grows.

**Fix**: add an index for every field used in `filters:` or `search:`:

```yaml
indexes:
  - fields: [org_id, role]
```

### 2. Offset pagination on large tables

**Symptom**: deep pages (page 50+) take several seconds.

**Fix**: switch to `cursor` pagination:

```yaml
pagination: cursor
```

### 3. Cache TTL too short

**Symptom**: high cache miss rate, database load not reduced.

**Fix**: increase the TTL. A 5-second TTL on a list endpoint that changes hourly
wastes Redis operations without meaningful freshness gain.

### 4. Oversized connection pool

**Symptom**: Postgres memory usage climbs; `idle in transaction` connections
accumulate.

**Fix**: reduce `pool_size` to match your actual concurrency. Start at 20 and
increase only if you see connection wait times in the metrics.

### 5. No cache on frequently-read endpoints

**Symptom**: database handles the same query thousands of times per minute.

**Fix**: add a `cache` block to GET endpoints that serve repeated queries:

```yaml
cache: { ttl: 60 }
```

### 6. Too many workers on a small container

**Symptom**: high memory usage, threads competing for CPU.

**Fix**: set `workers` to match the container's CPU limit:

```yaml
workers: 2
```

### 7. Not building in release mode

**Symptom**: benchmark numbers are 5-10x below targets.

**Fix**: always benchmark and deploy with release builds:

```bash
cargo build --release --workspace
cargo bench -p shaperail-runtime
```

Debug builds disable all compiler optimizations and are not representative of
production performance.
