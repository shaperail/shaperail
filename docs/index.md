---
title: Shaperail Documentation
nav_order: 1
---

# Shaperail

**Define your API as YAML resources. Shaperail generates the Rust backend — routes, database schema, validation, auth, migrations, and OpenAPI — from that one file.**

*Documentation for v{{ site.release_version }}.*

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
- **Multi-tenancy** — Row-level isolation via `tenant_key`; per-tenant cache and rate-limit keys; `super_admin` bypass
- **Observability** — Structured JSON logs, request_id propagation, Prometheus metrics, health endpoints (`/health`, `/health/ready`), OpenTelemetry trace export
- **Migrations** — Initial create-table SQL generated from schema; sqlx compile-time verified
- **OpenAPI 3.1** — Deterministic spec generation; TypeScript SDK generation
- **WASM plugins** — Controller hooks in TypeScript, Python, Rust, or any WASM-targeting language; sandboxed, fuel-limited, crash-isolated

### Available — requires manual wiring

The runtime primitives exist and are documented. Connecting them requires code in your `main.rs` or config. Each linked guide explains exactly what to wire.

- **Background jobs** — Queue and worker primitives; worker registration and handler mapping are manual ([Background jobs](/background-jobs/))
- **Events and webhooks** — Event emission from write handlers works; subscriber execution and inbound route registration are manual ([Events and webhooks](/events-and-webhooks/))
- **WebSockets** — Session and channel primitives work; route registration is manual ([WebSockets](/websockets/))
- **API key auth and rate limiting** — Runtime primitives exist; wiring to endpoints is manual ([Auth and ownership](/auth-and-ownership/))
- **GraphQL** — Enable with `protocols: [rest, graphql]`; generates list/get queries and create/update/delete mutations; list queries support `limit`/`offset` only ([GraphQL](/graphql/))
- **gRPC** — Enable with `protocols: [rest, grpc]`; supports list, stream, get, create, delete; `Update` RPC is not yet implemented ([gRPC](/grpc/))

### In progress

- gRPC Update RPC
- WebSocket auto-routing from channel YAML files
- Events subscriber auto-execution
- Workspace service registry and saga orchestration
- Background job worker auto-registration
