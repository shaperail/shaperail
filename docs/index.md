---
title: Shaperail Documentation
nav_order: 1
---

# Shaperail

**An AI-native Rust backend framework.** Define resources in YAML; get a production-ready REST API with auth, caching, jobs, WebSockets, and OpenAPI — with one canonical schema as the source of truth.

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
| `resources/*.controller.rs` | Business logic functions for controllers declared in the YAML |
| `migrations/*.sql` | SQL that evolves the database (generated from resource diff) |
| `shaperail.config.yaml` | Port, database, cache, auth, storage, logging, event subscribers |
| `.env` | `DATABASE_URL`, `REDIS_URL`, `JWT_SECRET`, etc. |
| `docker-compose.yml` | Postgres and Redis for local development |

Generated Rust, OpenAPI, and routes live in `generated/` and are not hand-edited.

---

## Features at a glance

- **REST API** — List, get, create, update, delete, bulk create/delete; cursor or offset pagination; filters, sort, full-text search; field selection and relation loading (`?include=…`).
- **Multi-database** — Optional `databases:` in config with named connections (e.g. `default`, `analytics`). Per-resource `db:` routes that resource to a connection; migrations run against `default`.
- **API versioning** — Per-resource `version` field prefixes all routes (`/v1/users`, `/v2/orders`). OpenAPI spec and CLI output reflect versioned paths.
- **Controllers** — Synchronous before/after business logic on write endpoints. Validate input, normalize data, enrich responses — all within the request lifecycle.
- **Auth** — JWT (Bearer) and API keys (`X-API-Key`); role-based and owner-based rules; rate limiting via Redis.
- **Caching** — Redis-backed cache per GET endpoint with TTL and configurable invalidation.
- **Background jobs** — Priority queues, retries, dead letter queue, job status; enqueue from endpoint declarations.
- **WebSockets** — Channel YAML, JWT on upgrade, room subscriptions, Redis pub/sub for multi-instance broadcast.
- **File storage** — Local, S3, GCS, Azure; upload validation, signed URLs, image processing.
- **Events & webhooks** — Auto-emitted resource events; subscribers (job, webhook, channel); outbound HMAC-signed webhooks; inbound webhook endpoints.
- **Observability** — Structured JSON logs, request_id, PII redaction; Prometheus metrics; OpenTelemetry; `/health` and `/health/ready`.
- **OpenAPI & SDK** — Deterministic OpenAPI 3.1; TypeScript SDK generation.

---

## Documentation map

### Get going

- [**Getting started**]({{ '/getting-started/' | relative_url }}) — Install CLI, scaffold a project, run the app, first schema change.

### Guides

- [**Guides**]({{ '/guides/' | relative_url }}) — Auth, controllers, migrations, Docker, caching, jobs, WebSockets, file storage, events, observability.

### Reference

- [**Reference**]({{ '/reference/' | relative_url }}) — Resource format, configuration, CLI, API responses and query parameters.

### Examples

- [**Examples**]({{ '/examples/' | relative_url }}) — [Blog API example]({{ '/blog-api-example/' | relative_url }}) — full sample with posts, comments, relations, and migrations.
