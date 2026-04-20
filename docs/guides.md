---
title: Guides
nav_order: 3
has_children: true
permalink: /guides/
---

# Guides

Task-focused guides for building and running a Shaperail application.

## All guides

| Guide | Description |
| --- | --- |
| [**Getting started**]({{ '/getting-started/' | relative_url }}) | Install the CLI, scaffold a project, start Postgres and Redis, run the app, make your first schema change, and troubleshoot. |
| [**Auth and ownership**]({{ '/auth-and-ownership/' | relative_url }}) | Public, role-based, and owner-based auth; what JWT wiring the scaffold provides today; API key and rate limiter integration notes; recommended `created_by` patterns. |
| [**Controllers**]({{ '/controllers/' | relative_url }}) | Synchronous business logic before/after DB operations: validation, normalization, response enrichment. Runtime `Context` API, manual registration notes, common patterns. Includes WASM plugin support for TypeScript, Python, and other languages. |
| [**Migrations and schema changes**]({{ '/migrations-and-schema-changes/' | relative_url }}) | Workflow when resources change: validate, migrate, review SQL, rollback. Concrete migration examples, zero-downtime patterns, handling existing data. |
| [**Docker deployment**]({{ '/docker-deployment/' | relative_url }}) | Local development with Docker Compose, release images with `shaperail build --docker`, multi-service Compose, troubleshooting ports, volumes, and networking. |
| [**Caching**]({{ '/caching/' | relative_url }}) | Declaring cache on GET endpoints, cache key format, auto-invalidation, `invalidate_on`, cache bypass, Redis configuration. |
| [**Background jobs**]({{ '/background-jobs/' | relative_url }}) | Declaring jobs on endpoints, what the Redis queue does automatically, and how to wire a worker/registry manually. |
| [**WebSockets**]({{ '/websockets/' | relative_url }}) | Runtime channel/session primitives, current channel YAML format, message shapes, and the manual route wiring still required today. |
| [**File storage**]({{ '/file-storage/' | relative_url }}) | File fields, upload config on endpoints, backends (local, S3, GCS, Azure), signed URLs, image processing, orphan cleanup. |
| [**Events and webhooks**]({{ '/events-and-webhooks/' | relative_url }}) | Auto-emitted events, subscriber config, outbound signing helpers, inbound verification helpers, and the worker/route wiring still required today. |
| [**Observability**]({{ '/observability/' | relative_url }}) | Structured logging, request_id, PII redaction, slow query log; `/health` and `/health/ready`; Prometheus metrics; OpenTelemetry. |
| [**GraphQL**]({{ '/graphql/' | relative_url }}) | Enable with `protocols: [rest, graphql]`. Current schema shape, auth behavior, relation support, and the present `limit`/`offset` list-query limitation. |
| [**gRPC**]({{ '/grpc/' | relative_url }}) | Enable with `protocols: [rest, grpc]`. Current RPC coverage, JWT metadata auth, reflection/health support, and the gaps that still exist. |
| [**Multi-service workspaces**]({{ '/multi-service/' | relative_url }}) | Define a `shaperail.workspace.yaml`, start services in dependency order, and understand what parts of registry/client/saga support are still manual. |
| [**Multi-tenancy**]({{ '/multi-tenancy/' | relative_url }}) | Add `tenant_key` to resources for automatic row-level isolation. JWT `tenant_id` claim, per-tenant cache keys, tenant-scoped rate-limit keys when the limiter is wired, `super_admin` bypass. |
| [**Error handling**]({{ '/error-handling/' | relative_url }}) | ShaperailError types, error response format, validation errors, custom error codes, controller error handling. |
| [**Testing**]({{ '/testing/' | relative_url }}) | Testing resources, controllers, and jobs. Unit tests, integration tests with test databases, snapshot testing for generated code. |
| [**Security**]({{ '/security/' | relative_url }}) | Security by default (SQL injection, validation, redaction), current JWT/API-key/rate-limit realities, CORS, multi-tenancy isolation, webhook signing, production checklist. |
| [**Deployment**]({{ '/deployment/' | relative_url }}) | Production deployment patterns: Docker, Kubernetes, managed platforms. Environment variables, health checks, scaling. |
| [**Performance**]({{ '/performance/' | relative_url }}) | Tuning worker count, connection pools, caching strategies, query optimization, benchmarking with `cargo bench`. |
| [**Debugging**]({{ '/debugging/' | relative_url }}) | Using `shaperail doctor`, reading structured logs, tracing requests, inspecting generated code, common debugging workflows. |
| [**API versioning**]({{ '/api-versioning/' | relative_url }}) | How the `version` field maps to URL prefixes, running multiple versions side by side, deprecation headers, client migration strategies. |

Pick a guide by task: auth, migrations, Docker, caching, jobs, WebSockets, files, events, observability, GraphQL, gRPC, multi-service, multi-tenancy, error handling, testing, security, deployment, performance, debugging, or API versioning.
