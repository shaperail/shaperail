---
title: Shaperail Documentation
nav_order: 1
---

# Shaperail

**Define your API as YAML resources. Shaperail generates the Rust backend — routes, database schema, validation, auth, migrations, and OpenAPI — from that one file.**

*Documentation tracks the [latest release](https://github.com/shaperail/shaperail/releases/latest) on GitHub.*

---

## Quick start

```bash
cargo install shaperail-cli
shaperail init my-app
cd my-app
docker compose up -d
shaperail serve
```

Your app is available at:

| URL | Purpose |
| --- | --- |
| `http://localhost:3000/docs` | Interactive API docs |
| `http://localhost:3000/openapi.json` | OpenAPI 3.1 spec |
| `http://localhost:3000/graphql` | GraphQL endpoint (when `protocols` includes `graphql`) |
| `http://localhost:3000/graphql/playground` | GraphQL Playground (dev) |
| `http://localhost:3000/health` | Liveness check |
| `http://localhost:3000/health/ready` | Readiness (DB + Redis) |
| `http://localhost:3000/metrics` | Prometheus metrics |

---

## Why Shaperail

A typical REST resource in plain Rust spans handler files, database models, migration SQL, validation logic, auth middleware, and OpenAPI annotations — 300–500 lines across 5 or more files. Add another resource, repeat the work. Shaperail replaces all of that with one ~40-line YAML file. The framework reads the file and generates the Rust code, the SQL schema, and the OpenAPI spec deterministically.

| Principle | What it means |
| --- | --- |
| **One source of truth** | Resource YAML drives schema, routes, validation, migrations, and OpenAPI. No hidden conventions. |
| **Explicit over implicit** | No routes or behavior unless you declare it in the resource file. |
| **Flat abstraction** | Resource definition maps directly to runtime; no deep framework layers. |
| **Deterministic output** | Same resource files produce the same OpenAPI spec and code every time. |
| **Docker-first dev** | `docker compose up -d` gives you Postgres and Redis; no manual DB setup. |

> Working with an LLM? Load the [LLM Guide](/llm-guide/) as context — it is the sole file an AI assistant needs to generate valid Shaperail resources.

---

## When to use Shaperail

| Good fit | Less ideal |
| --- | --- |
| REST APIs with clear resources, auth, and optional real-time or background work | Apps that need heavy custom routing or non-REST protocols only |
| Teams that want schema-first development and deterministic codegen | Teams that prefer hand-written controllers and ORM models |
| Docker-based local dev with Postgres and Redis | Environments where you cannot run Docker or Redis |
| Projects where a single YAML resource file should drive routes, DB, and OpenAPI | Prototypes that change shape every day with no schema discipline |

If you need a single source of truth for your API contract and like explicit declarations over magic, Shaperail is a strong fit.

---

## What you author

You edit these files; the framework generates the rest.

| File | Role |
| --- | --- |
| `resources/*.yaml` | Schema, endpoints, auth, relations, filters, pagination, cache, indexes |
| `resources/*.controller.rs` | Business logic before/after DB writes — see [Controllers](/controllers/) |
| `migrations/*.sql` | SQL that evolves the database (initial create files can be generated; later schema changes are manual SQL today) |
| `shaperail.config.yaml` | Port, database, cache, auth, storage, logging, event subscribers |
| `.env` | `DATABASE_URL`, `REDIS_URL`, `JWT_SECRET`, etc. |
| `docker-compose.yml` | Postgres and Redis for local development |

Generated Rust, OpenAPI, and routes live in `generated/` and are not hand-edited.

---

## Features at a glance

### Production-ready today

Everything below works from a resource YAML file with no manual wiring.

- **REST API** — List, get, create, update, delete, bulk create/delete; cursor and offset pagination; filters, sort, full-text search; field selection; relation loading (`?include=`)
- **Authentication** — JWT auth; role-based and owner-based access control declared per endpoint
- **Caching** — Redis-backed cache per GET endpoint with TTL, auto-invalidation on writes, configurable `invalidate_on`
- **File storage** — Local, S3, GCS, Azure; upload validation, signed URLs, image processing
- **Multi-tenancy** — Row-level isolation via `tenant_key`; per-tenant cache keys; `super_admin` bypass
- **Observability** — Structured JSON logs, request_id propagation, Prometheus metrics, health endpoints (`/health`, `/health/ready`), OpenTelemetry trace export
- **Migrations** — Initial create-table SQL generated from schema; sqlx compile-time verified
- **OpenAPI 3.1** — Deterministic spec generation; TypeScript SDK generation
- **API versioning** — Per-resource `version` field prefixes all routes (`/v1/users`, `/v2/orders`); reflected in OpenAPI spec and CLI output
- **Multi-database** — Named database connections via `databases:` config; per-resource `db:` routing; migrations run against `default`

### Available — requires manual wiring

The runtime primitives exist and are documented. GraphQL and gRPC require a Cargo feature flag and a `protocols:` config line, and have known feature gaps listed below. Each linked guide explains what works today.

- **Background jobs** — Queue, worker, and handler registration are fully auto-wired from resource YAML; enqueue jobs from `jobs:` on any write endpoint ([Background jobs](/background-jobs/))
- **Events and webhooks** — Event emission from write handlers and inbound webhook route registration are auto-configured; subscriber execution is still manual ([Events and webhooks](/events-and-webhooks/))
- **WebSockets** — Routes auto-registered from `channels/*.yaml` files at startup ([WebSockets](/websockets/))
- **Rate limiting** — Per-endpoint via `rate_limit: { max_requests: N, window_secs: N }` in resource YAML; requires Redis; startup warning when declared but Redis absent ([Auth and ownership](/auth-and-ownership/))
- **Controllers** — Before/after business logic on write endpoints in Rust or WASM (TypeScript, Python, Rust, Go, or any WASM-targeting language); auto-wired from resource YAML at startup ([Controllers](/controllers/))
- **GraphQL** — Enable with `protocols: [rest, graphql]`; generates list/get queries and create/update/delete mutations; list queries support `limit`/`offset` only ([GraphQL](/graphql/))
- **gRPC** — Enable with `protocols: [rest, grpc]`; supports list, stream, get, create, delete; `Update` RPC is not yet implemented ([gRPC](/grpc/))

### In progress

- gRPC Update RPC
- Events subscriber auto-execution
- Workspace service registry and saga orchestration
