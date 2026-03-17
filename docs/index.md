---
title: Shaperail Documentation
nav_order: 1
---

# Shaperail

**An AI-native Rust backend framework.** Define resources in YAML; get a production-ready REST API plus optional protocol and async primitives from one canonical schema.

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

| Principle | What it means |
| --- | --- |
| **One source of truth** | Resource YAML drives schema, routes, validation, migrations, and OpenAPI. No hidden conventions. |
| **Explicit over implicit** | No routes or behavior unless you declare it in the resource file. |
| **Flat abstraction** | Resource definition maps directly to runtime; no deep framework layers. |
| **Deterministic output** | Same resource files produce the same OpenAPI spec and code every time. |
| **Docker-first dev** | `docker compose up -d` gives you Postgres and Redis; no manual DB setup. |

The framework is built so that docs, codegen, and runtime stay in sync — and so that LLMs can generate valid Shaperail resources and commands with minimal mistakes.

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
| `resources/*.controller.rs` | One workable convention for controller modules; current apps still require manual controller registration |
| `migrations/*.sql` | SQL that evolves the database (initial create files can be generated; later schema changes are manual SQL today) |
| `shaperail.config.yaml` | Port, database, cache, auth, storage, logging, event subscribers |
| `.env` | `DATABASE_URL`, `REDIS_URL`, `JWT_SECRET`, etc. |
| `docker-compose.yml` | Postgres and Redis for local development |

Generated Rust, OpenAPI, and routes live in `generated/` and are not hand-edited.

---

## Features at a glance

- **REST API** — List, get, create, update, delete, bulk create/delete; cursor or offset pagination; filters, sort, full-text search; field selection and relation loading (`?include=…`).
- **GraphQL** — Enable with `protocols: [rest, graphql]`. The current generated schema exposes `list_<resource>`, singular get-by-id fields, and `create_` / `update_` / `delete_` mutations. List fields currently support `limit` and `offset` only.
- **gRPC** — Enable with `protocols: [rest, grpc]`. The current server supports list, stream, get, create, and delete RPCs plus health/reflection. `Update` is not implemented yet, and the CLI does not currently write `.proto` files to disk.
- **Multi-database** — Optional `databases:` in config with named connections (e.g. `default`, `analytics`). Per-resource `db:` routes that resource to a connection; migrations run against `default`.
- **API versioning** — Per-resource `version` field prefixes all routes (`/v1/users`, `/v2/orders`). OpenAPI spec and CLI output reflect versioned paths.
- **Controllers** — Synchronous before/after business logic on write endpoints. Validate input, normalize data, enrich responses — in Rust or sandboxed WASM (TypeScript, Python, Rust, etc.).
- **Auth** — JWT auth is scaffolded from `JWT_SECRET`. API key auth and Redis-backed rate limiting exist as runtime primitives but require manual wiring in the generated app.
- **Caching** — Redis-backed cache per GET endpoint with TTL and configurable invalidation.
- **Background jobs** — Endpoint `jobs:` declarations enqueue work into the Redis queue. Running a worker and registering handlers is still a manual bootstrap step.
- **WebSockets** — Runtime session/channel primitives exist, but the scaffold does not auto-load `channels/*.channel.yaml` or register `/ws/...` routes.
- **File storage** — Local, S3, GCS, Azure; upload validation, signed URLs, image processing.
- **Events & webhooks** — Write handlers can emit events into the job queue. Subscriber execution, webhook delivery handlers, and inbound webhook route registration still require manual wiring.
- **Observability** — Structured JSON logs, request_id, PII redaction; Prometheus metrics; OpenTelemetry; `/health` and `/health/ready`.
- **Multi-service workspaces** — `shaperail serve --workspace` validates a workspace and starts each service in dependency order. Registry, typed clients, and saga orchestration are not wired into that flow yet.
- **Multi-tenancy** — Add `tenant_key: org_id` to any resource for automatic row-level isolation. Queries are scoped to the JWT `tenant_id` claim; cache keys are per-tenant; rate-limit keys are too when the limiter is wired; `super_admin` bypasses the filter.
- **WASM plugins** — Write controller hooks in TypeScript, Python, Rust, or any language that compiles to WASM. Sandboxed execution with no filesystem or network access; fuel-limited; crash-isolated from the server.
- **OpenAPI & SDK** — Deterministic OpenAPI 3.1; TypeScript SDK generation.

---

## Documentation map

### Get going

- [**Getting started**]({{ '/getting-started/' | relative_url }}) — Install CLI, scaffold a project, run the app, first schema change.

### Guides

- [**Guides**]({{ '/guides/' | relative_url }}) — Auth, controllers, migrations, Docker, caching, jobs, WebSockets, file storage, events, observability, GraphQL.

### Reference

- [**Reference**]({{ '/reference/' | relative_url }}) — Resource format, configuration, CLI, API responses and query parameters.

### Examples

- [**Examples**]({{ '/examples/' | relative_url }}) — [Blog API example]({{ '/blog-api-example/' | relative_url }}) plus direct links to the checked-in repository examples for enterprise SaaS billing, an incident platform, multi-tenant SaaS, multi-service workspaces, and WASM plugins.
