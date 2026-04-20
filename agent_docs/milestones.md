# Shaperail — All Milestones

## How to use
Run `/milestone <number>` in any Claude Code session.
Example: `/milestone 1` builds Core Types. `/milestone 14` builds Multi-DB support.

## Status Legend
- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete — all deliverables done, checks pass, committed

---

## VERSION 2 — Foundation (M01–M13)
> Single-service REST backend. Must be complete before starting v3.
> End goal: `cargo install shaperail-cli` && `shaperail init myapp && shaperail serve` works.

---

### M01 — Core Types
**Crate:** `shaperail-core` | **Status:** [x]

**Deliverables:**
- [x] `FieldType` enum: uuid, string, integer, bigint, number, boolean, timestamp, date, enum, json, array, file
- [x] `FieldSchema` struct: type, primary, generated, required, unique, nullable, ref, min, max, format, values, default, sensitive, search
- [x] `ResourceDefinition` struct: resource, version, schema, endpoints, relations, indexes
- [x] `EndpointSpec` struct: method, path, auth, input, filters, search, pagination, sort, cache, hooks, events, jobs, upload, soft_delete
- [x] `AuthRule` enum: Public, Roles(Vec<String>), Owner
- [x] `RelationSpec` struct: resource, type (belongs_to/has_many/has_one), key/foreign_key
- [x] `IndexSpec` struct: fields, unique, order
- [x] `CacheSpec` struct: ttl, invalidate_on
- [x] `ShaperailError` enum: NotFound, Unauthorized, Forbidden, Validation(Vec<FieldError>), Conflict, RateLimited, Internal — with Display + HTTP status + From<sqlx::Error>
- [x] `ProjectConfig` struct — matches shaperail.config.yaml format from PRD
- [x] All public types have `///` doc comments
- [x] Unit tests cover every enum variant and struct field

**Acceptance Criteria:**
- `cargo build -p shaperail-core` zero warnings
- `cargo clippy -p shaperail-core -- -D warnings` passes
- Error response JSON shape matches PRD: `{ "error": { "code", "status", "message", "request_id", "details" } }`

---

### M02 — YAML Parser
**Crate:** `shaperail-codegen` | **Status:** [x]

**Deliverables:**
- [x] `parser` module: YAML string → `ResourceDefinition` (exact PRD format: `resource:` key, inline fields)
- [x] `config_parser` module: shaperail.config.yaml → `ProjectConfig`
- [x] `validator` module: semantic checks — enum needs values, soft_delete needs deleted_at, ref field must be uuid type, hooks list must be strings
- [x] Error messages in format: `"resource 'users': field 'role' is type enum but has no values"`
- [x] `shaperail validate <file>` CLI subcommand: reads resource file, prints errors or "✓ valid"
- [x] Snapshot tests (insta): 5 valid resources → snapshot parsed output; 10 invalid → snapshot error messages

**Acceptance Criteria:**
- `cargo test -p shaperail-codegen` all pass
- Parses the exact format in agent_docs/resource-format.md
- Invalid files produce human-readable errors, not raw serde panics

---

### M03 — Database Layer
**Crate:** `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] `db` module: PgPool setup from DATABASE_URL, health check query
- [x] Migration runner: reads migrations/ dir, applies via sqlx
- [x] Query generator: given ResourceDefinition produces typed sqlx queries for find_by_id, find_all, insert, update_by_id, soft_delete_by_id, hard_delete_by_id
- [x] Cursor pagination on find_all (default) and offset pagination (when declared)
- [x] Filter application: `?filter[role]=admin` → `WHERE role = $1`
- [x] Sort application: `?sort=-created_at,name` → `ORDER BY created_at DESC, name ASC`
- [x] Full-text search: `?search=term` via PostgreSQL `to_tsvector` on fields with `search: true`
- [x] Integration tests via `sqlx::test` macro (auto-rollback, isolated DB per test)

**Acceptance Criteria:**
- All queries use `sqlx::query_as!` macro — zero raw `query()` calls
- Zero `.unwrap()` in non-test code
- Integration tests pass against real Postgres (via docker compose)

---

### M04 — REST Handlers
**Crate:** `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] Handler generator: ResourceDefinition → Actix-web handlers for every declared endpoint
- [x] Response envelope: `{ "data": [...], "meta": { "cursor", "has_more", "total" } }` for list
- [x] Single record response: `{ "data": { ... } }` for get/create/update
- [x] Field selection: `?fields=name,email` trims response to declared fields
- [x] Relation loading: `?include=organization` triggers join for belongs_to relations
- [x] Bulk endpoints: bulk_create (up to 500), bulk_delete
- [x] Route registration: ResourceDefinition → Actix ServiceConfig
- [x] Integration tests: 200, 404, 422, 401, 403 for every endpoint type

**Acceptance Criteria:**
- Response shape is consistent — same envelope for every endpoint
- No handler calls `.unwrap()` or `.expect()`

---

### M05 — Auth System
**Crate:** `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] JWT middleware: validates Bearer token, attaches AuthUser to request
- [x] `AuthenticatedUser` Actix extractor: returns 401 if no valid JWT
- [x] RBAC enforcement from EndpointSpec.auth against token claims
- [x] Owner check: `AuthRule::Owner` → resource.created_by == auth_user.id
- [x] API key auth: X-API-Key header as alternative to JWT
- [x] Rate limiting: sliding window per IP + per token via Redis, configurable per endpoint
- [x] JWT issue + refresh token endpoint (used by `shaperail init` auth scaffold)
- [x] Tests: 401 no token, 403 wrong role, 200 correct role, owner allows own, owner blocks other's

**Acceptance Criteria:**
- JWT secret from env var `JWT_SECRET` (never hardcoded)
- Rate limiter state in Redis — survives server restart

---

### M06 — Redis Caching
**Crate:** `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] Redis client via deadpool-redis, pool from REDIS_URL
- [x] Cache middleware: GET endpoints with `cache.ttl` check Redis before DB
- [x] Cache key: `shaperail:<resource>:<endpoint>:<query_hash>:<user_role>`
- [x] Auto-invalidation: create/update/delete deletes all keys for that resource
- [x] `invalidate_on` from endpoint spec controls which operations bust cache
- [x] Cache bypass: `?nocache=1` or admin role
- [x] Tests: cache hit = 0 DB queries on second request, write invalidates, bypass works

**Acceptance Criteria:**
- Cache hit verified by query count (not just response match)
- PRD target: cached reads ≥ 80K req/s, P99 < 2ms

---

### M07 — Background Jobs
**Crate:** `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] `JobQueue` struct: enqueue(name, payload, priority) → job_id
- [x] Priority queues: critical, high, normal, low (separate Redis lists)
- [x] Worker: polls Redis, executes registered job handler, acks on success
- [x] Retry: exponential backoff, configurable max_retries per job
- [x] Dead letter queue: failed jobs move to `shaperail:jobs:dead` after max retries
- [x] Job status: pending/running/completed/failed queryable by job_id
- [x] Job timeout: auto-fail jobs exceeding configured duration
- [x] `ctx.jobs.enqueue()` available in HookContext
- [x] Tests: job executes, retries on failure, reaches dead letter, priority order

**Acceptance Criteria:**
- Worker runs in separate Tokio task — never blocks HTTP server
- All job state in Redis — survives restart

---

### M08 — WebSockets
**Crate:** `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] Channel YAML format: `channels/<name>.channel.yaml` (see PRD format)
- [x] Actix-web WS endpoint: `/ws/<channel>` with JWT auth on upgrade
- [x] Room subscription: client sends `{ "action": "subscribe", "room": "org:123" }`
- [x] Broadcast: `ctx.events` triggers push to subscribed room
- [x] Redis pub/sub backend: all instances receive broadcasts via Redis
- [x] Lifecycle hooks: on_connect, on_disconnect, on_message
- [x] Heartbeat: server ping every 30s, disconnect unresponsive clients
- [x] Tests: connect, subscribe, broadcast received, disconnect cleanup, cross-instance via Redis

**Acceptance Criteria:**
- Two server instances broadcast to each other's clients via Redis
- Auth failure returns 401 before WebSocket upgrade completes

---

### M09 — File Storage
**Crate:** `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] `StorageBackend` trait: upload, download, delete, signed_url
- [x] Local filesystem backend (dev default)
- [x] S3 backend via object_store crate
- [x] GCS backend via object_store crate
- [x] Azure Blob backend via object_store crate
- [x] Upload handler: `file` type fields → multipart endpoint, validates size + mime type
- [x] Image processing: resize + thumbnail via `image` crate
- [x] Signed URL generation: time-limited pre-signed download URLs
- [x] Orphan cleanup: resource delete → enqueue storage cleanup job
- [x] Tests: upload local, retrieve, delete, signed URL, invalid mime type rejected

**Acceptance Criteria:**
- Backend selected via `SHAPERAIL_STORAGE_BACKEND=s3|gcs|azure|local` env var
- File metadata (path, size, mime) stored in DB with resource record

---

### M10 — Events + Webhooks
**Crate:** `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] `EventEmitter`: emit(name, payload) — non-blocking, via job queue
- [x] Auto-emit: every create/update/delete emits `<resource>.<action>` automatically
- [x] Event subscribers in shaperail.config.yaml: job / webhook / channel / hook targets
- [x] Event log table: append-only, stores all emitted events for audit + replay
- [x] Outbound webhooks: POST to URL on event, HMAC-SHA256 signature header
- [x] Webhook retry: 3 attempts exponential backoff via job queue
- [x] Webhook delivery log: status, response_code, latency
- [x] Inbound webhooks: endpoint with signature verification (Stripe/GitHub patterns)
- [x] Tests: event emitted on create, webhook delivered, retry on 5xx, sig verification

**Acceptance Criteria:**
- Events never block HTTP response — always async via job queue
- Webhook signature: `X-Shaperail-Signature: sha256=HMAC(secret, body)`

---

### M11 — CLI
**Crate:** `shaperail-cli` | **Status:** [x]

**Deliverables (exact commands from PRD):**
- [x] `shaperail init <n>` — scaffold project with correct structure (all dirs + shaperail.config.yaml)
- [x] `shaperail generate` — run codegen for all resource files, write to generated/
- [x] `shaperail serve` — start dev server with hot reload via cargo-watch
- [x] `shaperail build` — release binary: `cargo build --release`
- [x] `shaperail build --docker` — generate Dockerfile + build scratch image ≤ 25 MB
- [x] `shaperail validate` — validate all resource files, report errors
- [x] `shaperail test` — run generated + custom tests
- [x] `shaperail migrate` — generate + apply SQL migration from resource diff
- [x] `shaperail migrate --rollback` — rollback last migration batch
- [x] `shaperail seed` — load fixture YAML files into DB
- [x] `shaperail export openapi` — output OpenAPI spec to stdout or --output file
- [x] `shaperail export sdk --lang ts` — generate TypeScript client SDK
- [x] `shaperail doctor` — check system deps: Rust, PostgreSQL, Redis, sqlx-cli
- [x] `shaperail routes` — print all routes with auth requirements
- [x] `shaperail jobs:status` — show job queue depth and recent failures
- [x] Tests: assert_cmd for every command, `shaperail init + shaperail serve` end-to-end test

**Acceptance Criteria:**
- `shaperail init myapp && cd myapp && shaperail serve` works end-to-end (PRD success metric)
- All commands have `--help` output
- `shaperail doctor` catches missing dependencies with clear fix instructions

---

### M12 — Observability
**Crate:** `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] Structured JSON logging via tracing crate, request_id on every line
- [x] Request/response middleware: method, path, status, duration_ms, user_id logged
- [x] PII redaction: `sensitive: true` fields redacted in all log output
- [x] OpenTelemetry spans: HTTP request, DB query, cache op, job execution
- [x] OTLP export to configurable endpoint (Jaeger/Zipkin/Honeycomb)
- [x] Prometheus metrics at `GET /metrics`: req_count, latency_histogram, db_pool_size, cache_hit_ratio, job_queue_depth, error_rate
- [x] `GET /health` — shallow: returns 200 if process running
- [x] `GET /health/ready` — deep: checks DB connection + Redis + storage
- [x] Slow query log: queries exceeding `SHAPERAIL_SLOW_QUERY_MS` threshold
- [x] Tests: /metrics returns valid Prometheus format, /health/ready returns 503 when DB down

**Acceptance Criteria:**
- Every HTTP request produces exactly one structured log line
- Sensitive fields never appear in logs even in error payloads

---

### M13 — OpenAPI Generation
**Crate:** `shaperail-codegen` | **Status:** [x]

**Deliverables:**
- [x] `openapi` module: Vec<ResourceDefinition> → OpenAPI 3.1 spec (JSON + YAML)
- [x] All endpoints documented: path, method, request body, response schemas, auth
- [x] Pagination, filter, sort, search params documented per endpoint
- [x] Standard error responses: 401, 403, 404, 422, 429, 500
- [x] `x-shaperail-hooks` and `x-shaperail-events` vendor extensions
- [x] Deterministic output: same resource files → byte-identical spec every time
- [x] TypeScript SDK generation from spec via openapi-typescript
- [x] `shaperail export openapi` CLI command
- [x] Tests: spec passes OpenAPI 3.1 validation, deterministic (run twice, diff is empty)

**Acceptance Criteria:**
- Generated spec passes OpenAPI 3.1 validation (PRD success metric: 100%)
- TypeScript SDK compiles from generated spec

---

## VERSION 3 — Multi-Everything (M14–M20)
> Start only after ALL v2 milestones are [x] Complete.

---

### M14 — Multi-Database
**Crates:** `shaperail-core`, `shaperail-codegen`, `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] `DatabaseEngine` enum: Postgres, MySQL, SQLite, MongoDB in shaperail-core
- [x] `db:` key in resource YAML: routes resource to named DB connection
- [x] Multi-DB config in shaperail.config.yaml: `databases:` map with named connections
- [x] SeaORM + SeaQuery in runtime; `DatabaseManager` for named SQL connections (Postgres wired)
- [x] Engine-specific migration SQL: `build_create_table_sql_for_engine` (Postgres, MySQL, SQLite)
- [x] MySQL backend: enable sqlx-mysql in SeaORM, migration support, 95% feature coverage
- [x] SQLite backend: enable sqlx-sqlite in SeaORM, WAL mode, 85% feature coverage
- [x] MongoDB backend: mongodb crate, schema validation, 75% feature coverage
- [x] ORM-backed CRUD path: use SeaQuery for dialect-agnostic queries (optional; current sqlx path retained)
- [x] `shaperail migrate` runs all engines in dependency order
- [x] Tests: full CRUD on each engine, same API behaviour, cross-DB project works

---

### M15 — GraphQL
**Crates:** `shaperail-codegen`, `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] `protocols: [rest, graphql]` in shaperail.config.yaml
- [x] GraphQL type generation from resource schema via async-graphql crate
- [x] Query resolvers: list (filter/sort/pagination), get, nested relations
- [x] DataLoader generated for all relations — N+1 impossible
- [x] Mutation resolvers: create, update, delete — same auth as REST
- [x] Subscription resolvers from declared WebSocket events
- [x] `/graphql` endpoint + `/graphql/playground` in dev mode
- [x] Depth limit + complexity limit configurable
- [x] Tests: queries, mutations, auth, N+1 verified absent

---

### M16 — gRPC
**Crates:** `shaperail-codegen`, `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] `protocols: [rest, grpc]` in shaperail.config.yaml
- [x] `.proto` generation from resource schema (auto-generated, never hand-edited)
- [x] Tonic gRPC server implementation
- [x] Streaming RPCs for list endpoints
- [x] JWT via gRPC metadata interceptors
- [x] Server reflection (grpcurl compatible)
- [x] grpc.health.v1 health check service
- [x] Tests: gRPC calls match REST responses, auth enforced

---

### M17 — Multi-Service
**Crates:** `shaperail-core`, `shaperail-cli` | **Status:** [x]

**Deliverables:**
- [x] `shaperail.workspace.yaml` format: declares multiple services
- [x] Service registry via Redis: services register on startup, discover peers
- [x] Typed inter-service clients: auto-generated from peer's resource definitions
- [x] `shaperail serve --workspace` starts all services
- [x] Distributed saga support via saga YAML files
- [x] Tests: Service A calls Service B with typed client, type mismatch = compile error

---

### M18 — Multi-Tenancy
**Crate:** `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] `tenant_key:` field in resource YAML
- [x] Automatic tenant filter on every query (scoped to auth_user.tenant_id)
- [x] `ctx.tenant_id` in HookContext
- [x] Tenant-scoped rate limiting and caching
- [x] `super_admin` role bypasses tenant filter
- [x] Tests: tenant A cannot read B's data, admin reads all, cache does not leak

---

### M19 — WASM Plugins
**Crates:** `shaperail-core`, `shaperail-runtime` | **Status:** [x]

**Deliverables:**
- [x] WasmHook runtime via wasmtime
- [x] Plugin interface: `before_hook(ctx_json) → result_json`
- [x] `controller: { before: "wasm:./plugins/my_validator.wasm" }` in resource YAML
- [x] Plugin sandboxing: no filesystem/network by default
- [x] Example plugins: TypeScript + Python compiled to WASM
- [x] Tests: WASM hook runs and modifies ctx, crash does not crash server

---

### M20 — Embedded AI + Admin Panel
**Crates:** `shaperail-runtime`, `shaperail-cli` | **Status:** [ ]

**Deliverables:**
- [ ] `shaperail.ai.yaml` config for local model or OpenAI-compatible API
- [ ] `POST /ai/query` — natural language → SQL → result, scoped to user permissions
- [ ] Auto-generated admin panel at `/admin` from resource definitions
- [ ] Admin auth: separate `admin_roles` config
- [ ] `shaperail generate --admin` CLI command
- [ ] Tests: AI query returns correct results, admin panel enforces auth

---

## VERSION 4 — Autonomous (M21–M26)
> Start only after ALL v3 milestones are [x] Complete.

---

### M21 — Self-Healing Runtime
**Crate:** `shaperail-runtime` | **Status:** [ ]

**Deliverables:**
- [ ] Anomaly detection: EMA on latency, error rate, throughput per endpoint
- [ ] Configurable thresholds in `shaperail.autopilot.yaml`
- [ ] Diagnosis engine: correlate anomaly with deployments, DB patterns, upstream failures
- [ ] Auto-remediation for: slow query (index), pool exhaustion (scale), cache stampede (stagger TTL), OOM, rate spike, connection leak, upstream timeout (circuit breaker), job backlog
- [ ] Modes: observe / approve / auto
- [ ] Healing log + rollback: every action is reversible with `shaperail autopilot rollback <id>`
- [ ] Tests: inject slow query → index recommended; inject pool exhaustion → pool scaled

---

### M22 — Self-Optimizing Performance
**Crate:** `shaperail-runtime` | **Status:** [ ]

**Deliverables:**
- [ ] Query pattern analysis → index suggestions + auto-apply
- [ ] Adaptive cache TTL based on actual write frequency
- [ ] Cache warming prediction from access patterns
- [ ] Connection pool auto-tuning from concurrency patterns
- [ ] Weekly report: `shaperail autopilot report` — slow endpoints, unused indexes, cache recommendations, cost estimate
- [ ] Tests: generate traffic pattern → correct optimization suggested

---

### M23 — MCP Server
**Crates:** `shaperail-runtime`, `shaperail-cli` | **Status:** [ ]

**Deliverables:**
- [ ] MCP server exposing tools: list_resources, get_schema, create_resource, update_resource, delete_resource, run_migration, get_metrics, get_healing_log, apply_optimization
- [ ] Agent permission tiers: read-only / read-write / admin in `shaperail.agents.yaml`
- [ ] Approval flow: write ops require human approval in approve mode
- [ ] Agent activity log: every MCP call logged with agent identity + result
- [ ] `shaperail serve --mcp` starts MCP alongside HTTP
- [ ] Tests: Claude Code connects via MCP and performs CRUD, permissions enforced

---

### M24 — Self-Evolving Codebase
**Crates:** `shaperail-runtime`, `shaperail-codegen` | **Status:** [ ]

**Deliverables:**
- [ ] Field usage tracking: record which fields are read/written in production
- [ ] Schema evolution suggestions: unused fields, type mismatches, new indexes
- [ ] Relation discovery: frequent cross-resource queries → suggest declared relation
- [ ] `shaperail autopilot suggest` — outputs data-driven suggestions with impact estimates
- [ ] One-command apply: generates resource diff + migration + git commit
- [ ] Tests: simulate access pattern → correct suggestion generated

---

### M25 — Autonomous Testing
**Crate:** `shaperail-runtime` | **Status:** [ ]

**Deliverables:**
- [ ] Auto-generated integration tests per endpoint: happy path, validation, auth, edge cases
- [ ] Property-based testing via proptest
- [ ] Chaos testing: slow DB, cache miss, timeout — verify graceful degradation
- [ ] Regression detection: run tests before + after every self-healing fix
- [ ] Load test generation from actual traffic patterns (k6 scripts)
- [ ] Contract testing for multi-service workspaces
- [ ] Flaky test detection + auto-quarantine
- [ ] Meta-test: generated tests actually catch injected bugs

---

### M26 — Autonomous Security
**Crate:** `shaperail-runtime` | **Status:** [ ]

**Deliverables:**
- [ ] `cargo audit` on schedule + auto-PR for CVE patches
- [ ] Runtime threat detection: SQLi, brute-force, request smuggling → block
- [ ] Secret rotation: JWT secrets, DB passwords, API keys on schedule, zero-downtime
- [ ] TLS cert management: ACME auto-renew, 30-day expiry alerts
- [ ] OWASP Top 10 scanning + auto-remediation
- [ ] PII leak detection in logs + error responses
- [ ] Immutable audit trail: append-only, no DELETE/UPDATE on audit table
- [ ] Tests: inject SQLi → blocked; simulate cert expiry → renewal triggered
