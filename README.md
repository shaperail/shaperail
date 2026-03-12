# Shaperail

**The AI-native Rust backend framework.** Define your API in YAML, get a production-ready Rust server with zero boilerplate.

```bash
cargo install shaperail-cli
shaperail init my-app
cd my-app
docker compose up -d
shaperail serve
```

With Docker running, Shaperail scaffolds a working CRUD API with preconfigured Postgres + Redis, health checks, browser docs, OpenAPI export, auth rules, Redis-backed cache/jobs plumbing, and observability endpoints from a single YAML file.

---

## Main Goal

Shaperail exists to make backend development predictable enough that both humans
and LLMs can produce correct Shaperail projects with very low mistake rates. The
framework should turn a small, explicit schema into a working Rust API without
hidden behavior, alias-heavy syntax, or mismatched docs.

## Core Value

Shaperail compresses backend intent into one deterministic schema.

- One canonical way to define resources, endpoints, relations, and config
- Much lower token cost than hand-written Express or FastAPI CRUD
- Fail-closed validation so invalid or hallucinated keys error loudly
- Production-ready Rust runtime generated from the same source of truth

## What AI-First Means

For Shaperail, AI-first does not mean adding more magic. It means reducing
ambiguity until the correct answer is the easiest answer for both a developer
and a model.

- A model reading the docs should see one obvious valid way to do a task
- Examples, scaffolds, parser behavior, codegen, and runtime behavior must match
- Unsupported shapes should be rejected clearly instead of being silently ignored
- `shaperail init my-app && cd my-app && shaperail serve` must stay the shortest correct path

---

## Why Shaperail?

| Problem | Shaperail Solution |
|---------|-------------------|
| Writing CRUD endpoints is tedious | Define once in YAML, generate everything |
| Rust backends need too much boilerplate | Zero boilerplate — schema is the source of truth |
| Frameworks are opinionated but inflexible | Explicit over implicit — nothing runs unless you declare it |
| Performance requires manual optimization | 150K+ req/s out of the box, Redis caching built-in |
| Auth/jobs/events are always custom | JWT, RBAC, background jobs, webhooks — all declarative |

## Quick Start

### Install

```bash
# Via cargo
cargo install shaperail-cli

# Or via install script (macOS/Linux)
curl -fsSL https://shaperail.dev/install.sh | sh
```

### Prerequisites

```bash
shaperail doctor  # checks Rust + Docker, plus optional local tools
```

You need:
- **Rust** 1.85+
- **Docker** with Compose support

Optional:
- **sqlx-cli** if you use `shaperail migrate`
- **psql** and **redis-cli** for manual inspection/debugging

### Create a project

```bash
shaperail init my-app
cd my-app
```

This scaffolds:
```
my-app/
├── README.md           # Quickstart + local docs URLs
├── resources/          # Your API definitions (YAML)
├── migrations/         # Auto-generated SQL migrations
├── channels/           # WebSocket channel definitions
├── generated/          # Auto-generated Rust code (don't edit)
├── shaperail.config.yaml   # Project configuration
├── docker-compose.yml  # Postgres + Redis for local dev
└── Cargo.toml
```

Endpoints are explicit. If a resource omits `endpoints:`, Shaperail parses the file
but generates no HTTP routes for that resource.

### Define a resource

Create `resources/users.yaml`:

```yaml
resource: users
version: 1

schema:
  id:         { type: uuid, primary: true, generated: true }
  email:      { type: string, format: email, unique: true, required: true }
  name:       { type: string, min: 1, max: 200, required: true }
  role:       { type: enum, values: [admin, member, viewer], default: member }
  org_id:     { type: uuid, ref: organizations.id, required: true }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }

endpoints:
  list:
    method: GET
    path: /users
    auth: [member, admin]
    filters: [role, org_id]
    search: [name, email]
    pagination: cursor
    sort: [created_at, name]
    cache: { ttl: 60 }

  get:
    method: GET
    path: /users/:id
    auth: [member, admin]

  create:
    method: POST
    path: /users
    auth: [admin]
    input: [email, name, role, org_id]
    hooks: [validate_org]
    events: [user.created]
    jobs: [send_welcome_email]

  update:
    method: PATCH
    path: /users/:id
    auth: [admin, owner]
    input: [name, role]

  delete:
    method: DELETE
    path: /users/:id
    auth: [admin]
    soft_delete: true

relations:
  organization: { resource: organizations, type: belongs_to, key: org_id }
  orders:       { resource: orders, type: has_many, foreign_key: user_id }

indexes:
  - { fields: [email], unique: true }
  - { fields: [org_id, role] }
  - { fields: [created_at], order: desc }
```

### Generate and run

```bash
docker compose up -d    # start Postgres + Redis and create the app database
shaperail generate          # generate Rust code from YAML
shaperail migrate           # create new migration files after schema changes
shaperail serve             # apply existing migrations and start the dev server
```

Your API is live at `http://localhost:3000`:

- Browser docs: `http://localhost:3000/docs`
- OpenAPI JSON: `http://localhost:3000/openapi.json`

```bash
# List users (with cursor pagination)
curl http://localhost:3000/users

# Create a user
curl -X POST http://localhost:3000/users \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"email": "alice@example.com", "name": "Alice", "org_id": "..."}'

# Filter + search + sort
curl "http://localhost:3000/users?filter[role]=admin&search=alice&sort=-created_at"

# Field selection
curl "http://localhost:3000/users?fields=name,email"

# Include relations
curl "http://localhost:3000/users?include=organization"
```

---

## User Guide

Public user-facing docs now live in `docs/`. Start with:

- [docs/README.md](./docs/README.md)
- [docs/getting-started.md](./docs/getting-started.md)
- [docs/resource-guide.md](./docs/resource-guide.md)
- [docs/auth-and-ownership.md](./docs/auth-and-ownership.md)
- [docs/migrations-and-schema-changes.md](./docs/migrations-and-schema-changes.md)
- [docs/docker-deployment.md](./docs/docker-deployment.md)

The first complete example app files live in:

- [examples/blog-api/README.md](./examples/blog-api/README.md)

These docs should remain the canonical framework guide because they version with
the code. The GitHub wiki can still be useful, but it should stay secondary for
FAQs, release-independent notes, and community-maintained walkthroughs.

`agent_docs/` remains maintainer documentation for building the framework
itself.

---

## Features

### Schema Types

| Type | Postgres | Notes |
|------|----------|-------|
| `uuid` | `UUID` | Auto-generated with `generated: true` |
| `string` | `TEXT` | Supports `min`, `max`, `format` (email, url) |
| `integer` | `INTEGER` | Supports `min`, `max` |
| `bigint` | `BIGINT` | For large numbers |
| `number` | `DOUBLE PRECISION` | Floating point |
| `boolean` | `BOOLEAN` | |
| `timestamp` | `TIMESTAMPTZ` | Auto-set with `generated: true` |
| `date` | `DATE` | |
| `enum` | Custom TYPE | Requires `values: [...]` |
| `json` | `JSONB` | Arbitrary JSON |
| `array` | `TEXT[]` | Array of strings |
| `file` | metadata | Stored in object storage |

### Field Constraints

```yaml
schema:
  email:    { type: string, format: email, unique: true, required: true, sensitive: true }
  name:     { type: string, min: 1, max: 200, required: true }
  role:     { type: enum, values: [admin, member, viewer], default: member }
  org_id:   { type: uuid, ref: organizations.id, required: true }
  avatar:   { type: file, nullable: true }
  id:       { type: uuid, primary: true, generated: true }
```

| Constraint | Description |
|------------|-------------|
| `primary` | Primary key |
| `generated` | Auto-generated (UUID v4 or timestamps) |
| `required` | NOT NULL, must be provided on create |
| `unique` | Unique constraint |
| `nullable` | Allows NULL |
| `ref` | Foreign key reference (`table.column`) |
| `min` / `max` | Length (strings) or value (numbers) |
| `format` | Validation: `email`, `url` |
| `values` | Enum variants |
| `default` | Default value |
| `sensitive` | Redacted in logs, excluded from search |
| `search` | Enables full-text search on this field |

### Authentication & Authorization

Shaperail supports JWT and API key authentication out of the box.

```yaml
endpoints:
  list:
    auth: [member, admin]      # role-based access
  update:
    auth: [admin, owner]       # owner = resource.created_by matches token
  public_endpoint:
    auth: public               # no authentication required
```

**JWT**: Pass `Authorization: Bearer <token>` header. Configure with `JWT_SECRET` env var.

**API Keys**: Pass `X-API-Key` header. Alternative to JWT for service-to-service calls.

**Rate Limiting**: Sliding window per IP + per token via Redis. Configurable per endpoint.

### Caching

Redis-backed response caching with automatic invalidation:

```yaml
endpoints:
  list:
    cache:
      ttl: 60                              # seconds
      invalidate_on: [create, update, delete]  # auto-bust on writes
```

- Cache key: `shaperail:<resource>:<endpoint>:<query_hash>:<user_role>`
- Bypass: `?nocache=1` or admin role
- Zero DB queries on cache hit

### Background Jobs

Redis-backed job queue with priorities and retries:

```yaml
endpoints:
  create:
    jobs: [send_welcome_email]   # enqueued after successful create
```

- **Priority queues**: critical, high, normal, low
- **Retry**: exponential backoff, configurable `max_retries`
- **Dead letter queue**: failed jobs preserved for inspection
- **Job status**: query by job ID (pending/running/completed/failed)
- **Timeout**: auto-fail jobs exceeding configured duration

Monitor with: `shaperail jobs:status`

### Events & Webhooks

Declarative event system with webhook delivery:

```yaml
endpoints:
  create:
    events: [user.created]     # emitted after successful create
```

Configure subscribers in `shaperail.config.yaml`:

```yaml
events:
  subscribers:
    - event: user.created
      targets:
        - type: webhook
          url: https://example.com/hooks/user-created
        - type: job
          name: sync_to_crm
        - type: channel
          name: notifications
          room: "org:{org_id}"
```

- Events never block HTTP responses (async via job queue)
- Outbound webhooks signed with HMAC-SHA256: `X-Shaperail-Signature: sha256=...`
- Webhook retry: 3 attempts with exponential backoff
- Full event log for audit and replay
- Inbound webhook verification (Stripe/GitHub patterns)

### WebSockets

Real-time channels with room-based subscriptions:

Create `channels/notifications.channel.yaml`:

```yaml
channel: notifications
auth: [member, admin]
rooms: true
hooks:
  on_connect: [log_connect]
  on_disconnect: [log_disconnect]
  on_message: [validate_message]
```

Client connects to `ws://localhost:3000/ws/notifications` with JWT, then:

```json
{ "action": "subscribe", "room": "org:123" }
```

- Redis pub/sub backend for multi-instance broadcast
- Heartbeat with auto-disconnect for unresponsive clients
- Lifecycle hooks: `on_connect`, `on_disconnect`, `on_message`

### File Storage

Multi-backend file storage with image processing:

```yaml
schema:
  avatar: { type: file }

endpoints:
  create:
    upload: { field: avatar, storage: local, max_size: 5mb, types: [png, jpg] }
```

Backends (set via `SHAPERAIL_STORAGE_BACKEND`):
- `local` — filesystem (dev default)
- `s3` — Amazon S3
- `gcs` — Google Cloud Storage
- `azure` — Azure Blob Storage

Features: signed URLs, image resize/thumbnails, orphan cleanup on delete.

### Observability

Built-in structured logging, metrics, and tracing:

```bash
GET /health        # shallow health check
GET /health/ready  # deep check (DB + Redis + storage)
GET /metrics       # Prometheus format
```

- **Structured JSON logging** with request IDs on every line
- **PII redaction**: `sensitive: true` fields never appear in logs
- **OpenTelemetry**: spans for HTTP, DB, cache, and job execution
- **Prometheus metrics**: request count, latency histogram, DB pool size, cache hit ratio, job queue depth, error rate
- **Slow query log**: configurable via `SHAPERAIL_SLOW_QUERY_MS`

### OpenAPI & SDK Generation

Auto-generated API documentation:

```bash
shaperail export openapi                       # print to stdout
shaperail export openapi --output api.yaml     # write to file
shaperail export sdk --lang ts                 # TypeScript client SDK
```

- OpenAPI 3.1 spec with all endpoints, schemas, auth, pagination, filters
- Deterministic output (same input = byte-identical spec)
- TypeScript SDK generated from spec

### Relations

Declare relationships between resources:

```yaml
relations:
  organization: { resource: organizations, type: belongs_to, key: org_id }
  orders:       { resource: orders, type: has_many, foreign_key: user_id }
  profile:      { resource: profiles, type: has_one, foreign_key: user_id }
```

Load relations via query parameter:

```bash
curl "http://localhost:3000/users/123?include=organization"
```

### Indexes

```yaml
indexes:
  - { fields: [email], unique: true }
  - { fields: [org_id, role] }
  - { fields: [created_at], order: desc }
```

### Pagination

```yaml
endpoints:
  list:
    pagination: cursor    # or: offset
```

**Cursor pagination** (default, recommended):
```json
{
  "data": [...],
  "meta": { "cursor": "eyJpZCI6...", "has_more": true }
}
```

**Offset pagination**:
```json
{
  "data": [...],
  "meta": { "page": 1, "per_page": 25, "total": 150 }
}
```

### Soft Delete

```yaml
endpoints:
  delete:
    soft_delete: true    # sets deleted_at instead of removing row
```

Soft-deleted records are automatically excluded from queries.

---

## CLI Reference

| Command | Description |
|---------|-------------|
| `shaperail init <name>` | Scaffold a new project |
| `shaperail generate` | Generate Rust code from resource YAML files |
| `shaperail serve` | Start dev server with hot reload |
| `shaperail build` | Build release binary |
| `shaperail build --docker` | Build scratch-based Docker image |
| `shaperail validate` | Validate all resource files |
| `shaperail test` | Run generated + custom tests |
| `shaperail migrate` | Generate + apply SQL migrations |
| `shaperail migrate --rollback` | Rollback last migration batch |
| `shaperail seed` | Load fixture YAML files into database |
| `shaperail export openapi` | Export OpenAPI 3.1 spec |
| `shaperail export sdk --lang ts` | Generate TypeScript client SDK |
| `shaperail doctor` | Check system dependencies |
| `shaperail routes` | Print all routes with auth requirements |
| `shaperail jobs:status` | Show job queue depth and recent failures |

---

## Configuration

### shaperail.config.yaml

```yaml
project: my-app
port: 3000
workers: auto

database:
  type: postgresql
  host: ${SHAPERAIL_DB_HOST:localhost}
  port: 5432
  name: my_app_db
  pool_size: 20

cache:
  type: redis
  url: ${REDIS_URL:redis://localhost:6379}

auth:
  provider: jwt
  secret_env: JWT_SECRET
  expiry: 24h
  refresh_expiry: 30d

storage:
  provider: local

logging:
  level: info
  format: json

events:
  subscribers: []
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | required |
| `REDIS_URL` | Redis connection string | `redis://localhost:6379` |
| `JWT_SECRET` | Secret for signing JWTs | required |
| `SHAPERAIL_STORAGE_BACKEND` | File storage backend | `local` |
| `SHAPERAIL_SLOW_QUERY_MS` | Slow query threshold | `100` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OpenTelemetry collector endpoint | disabled |

---

## Response Format

All endpoints return consistent JSON envelopes:

**Single record** (get, create, update):
```json
{
  "data": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "email": "alice@example.com",
    "name": "Alice",
    "role": "admin"
  }
}
```

**List** (with pagination meta):
```json
{
  "data": [...],
  "meta": {
    "cursor": "eyJpZCI6...",
    "has_more": true
  }
}
```

**Error**:
```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "status": 422,
    "message": "Validation failed",
    "request_id": "req_abc123",
    "details": [
      { "field": "email", "message": "must be a valid email address" }
    ]
  }
}
```

---

## Docker

### Local development

```bash
docker compose up -d   # starts Postgres + Redis and creates the app database
shaperail serve            # start your app
```

### Production build

```bash
shaperail build --docker   # produces scratch-based image
```

The generated image cross-compiles to `x86_64-unknown-linux-musl` and runs from
`scratch` for minimal size (target: under 25 MB). Container platforms should use
their native HTTP checks instead of shell-based `HEALTHCHECK` commands.

---

## Project Structure

Shaperail is a workspace of four crates:

| Crate | Purpose |
|-------|---------|
| [`shaperail-core`](./shaperail-core) | Shared types: `ResourceDefinition`, `FieldType`, `ShaperailError` |
| [`shaperail-codegen`](./shaperail-codegen) | YAML parser, validator, Rust/SQL/OpenAPI code generation |
| [`shaperail-runtime`](./shaperail-runtime) | Actix-web server, handlers, DB, cache, auth, jobs, events, storage |
| [`shaperail-cli`](./shaperail-cli) | The `shaperail` binary — developer-facing CLI tool |

Dependency graph (flat — max depth 2):
```
shaperail-cli ──→ shaperail-codegen ──→ shaperail-core
                                    ↑
shaperail-runtime ──────────────────────┘
```

---

## Performance

Shaperail is designed to meet these targets:

| Metric | Target |
|--------|--------|
| Simple JSON response | 150,000+ req/s |
| DB read (cached) | 80,000+ req/s, P99 < 2ms |
| DB write | 20,000+ req/s, P99 < 10ms |
| Idle memory | ≤ 60 MB |
| Release binary | < 20 MB |
| Cold start | < 100ms |

Current tracked smoke baselines live in `BENCHMARKS.md`. Tagged releases should
refresh that report from the current commit.

---

## Design Principles

1. **One Way** — No aliases, no alternative syntax, no shortcuts
2. **Explicit Over Implicit** — Nothing executes unless declared in the resource file
3. **Flat Abstraction** — Resource (layer 1) maps to runtime (layer 2). Max depth: 2
4. **Schema Is Source of Truth** — All code generated from schema, never reverse-engineered
5. **Compiler as Safety Net** — Every generated Rust file must compile and pass clippy

---

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
