---
title: Guides
nav_order: 2
has_children: true
permalink: /guides/
---

# Guides

Task-focused guides for building and running a Shaperail application.

## All guides

| Guide | Description |
| --- | --- |
| [**Getting started**]({{ '/getting-started/' | relative_url }}) | Install the CLI, scaffold a project, start Postgres and Redis, run the app, make your first schema change, and troubleshoot. |
| [**Auth and ownership**]({{ '/auth-and-ownership/' | relative_url }}) | Public, role-based, and owner-based auth; JWT and API keys; rate limiting; recommended patterns for `created_by` and owner checks. |
| [**Controllers**]({{ '/controllers/' | relative_url }}) | Synchronous business logic before/after DB operations: validation, normalization, response enrichment. ControllerContext API, file conventions, common patterns. |
| [**Migrations and schema changes**]({{ '/migrations-and-schema-changes/' | relative_url }}) | Workflow when resources change: validate, migrate, review SQL, rollback. How `shaperail migrate` and `shaperail serve` interact. |
| [**Docker deployment**]({{ '/docker-deployment/' | relative_url }}) | Local development with Docker Compose, standard URLs, release image with `shaperail build --docker`, production checklist. |
| [**Caching**]({{ '/caching/' | relative_url }}) | Declaring cache on GET endpoints, cache key format, auto-invalidation, `invalidate_on`, cache bypass, Redis configuration. |
| [**Background jobs**]({{ '/background-jobs/' | relative_url }}) | Declaring jobs on endpoints, priority levels, lifecycle, retries, dead letter queue, timeout, monitoring with `shaperail jobs:status`. |
| [**WebSockets**]({{ '/websockets/' | relative_url }}) | Channel YAML, connection and auth, subscribe/unsubscribe, broadcasting, Redis pub/sub for multi-instance, heartbeat, lifecycle hooks. |
| [**File storage**]({{ '/file-storage/' | relative_url }}) | File fields, upload config on endpoints, backends (local, S3, GCS, Azure), signed URLs, image processing, orphan cleanup. |
| [**Events and webhooks**]({{ '/events-and-webhooks/' | relative_url }}) | Auto-emitted events, custom events, subscribers (job, webhook, channel, hook), outbound webhooks with HMAC, inbound webhooks. |
| [**Observability**]({{ '/observability/' | relative_url }}) | Structured logging, request_id, PII redaction, slow query log; `/health` and `/health/ready`; Prometheus metrics; OpenTelemetry. |
| [**GraphQL**]({{ '/graphql/' | relative_url }}) | Enable with `protocols: [rest, graphql]`. Queries (list, get, relations), mutations (create, update, delete), same auth as REST, Playground at `/graphql/playground`. |
| [**gRPC**]({{ '/grpc/' | relative_url }}) | Enable with `protocols: [rest, grpc]`. Auto-generated `.proto` files, unary and streaming RPCs, JWT auth via metadata, server reflection, health checks. |

Pick a guide by task: auth, migrations, Docker, caching, jobs, WebSockets, files, events, observability, GraphQL, or gRPC.
