# Changelog

All notable changes to Shaperail will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-03-13

### Added

- **API Versioning** — the `version` field on each resource YAML now drives route prefixing. `version: 1` registers all endpoints under `/v1/...`. OpenAPI spec, CLI `routes` command, and runtime all reflect versioned paths.
- **Controller System** — new `controller: { before: fn, after: fn }` field on endpoints for synchronous in-request business logic. Controller functions live in `resources/<resource>.controller.rs`, co-located with the resource YAML for a two-file-complete-picture workflow.
- **`ControllerContext` type** — provides mutable input, DB result, authenticated user, database pool, and request headers to controller functions.
- **`ControllerMap` registry** — maps `(resource, function_name)` pairs to controller handlers, following the same pattern as `StoreRegistry`.

### Changed

- **BREAKING:** `hooks:` field removed from `EndpointSpec`. Using it now produces a clear "unknown field" error via `deny_unknown_fields`. Use `controller:` for synchronous in-request logic, or `jobs:` for async background work.
- Scaffolded projects now create a `controllers/` directory instead of `hooks/`.
- All CRUD handlers (`handle_create`, `handle_update`, `handle_delete`) now invoke before/after controllers when declared.
- `enqueue_declared_hooks` function removed from the runtime side-effect pipeline.

## [0.2.2] - 2026-03-13

### Changed

- Rebuilt the public documentation around a standard Jekyll documentation theme with conventional navigation and search
- Added first-class user guides for CLI workflows and the Blog API example so the published docs site is self-contained
- Updated release-facing metadata and install/docs URLs to use `https://shaperail.io`
- Removed the remaining Node 20-based GitHub Actions from CI and release workflows

## [0.2.1] - 2026-03-12

### Changed

- Fixed crate publish manifests so internal Shaperail dependencies include version requirements and package cleanly for crates.io
- Aligned the install script with GitHub release archive names and the real repository coordinates
- Added `shaperail serve --check` plus CLI smoke coverage for scaffolded project validation and compile checks
- Reused the runtime SQL generator in `shaperail migrate`, including foreign keys, array types, enum constraints, soft-delete columns, and `pgcrypto` setup for generated UUIDs
- Updated scaffolded apps to create an initial migration, expose health and metrics routes, and apply migrations automatically on startup
- Wired declared endpoint events, jobs, and hooks into the runtime side-effect pipeline and corrected `jobs:status` to inspect the real Redis queue keys
- Made resource loading fail closed on semantic validation and reject unsupported upload endpoints instead of silently ignoring them

## [0.2.0] - 2026-03-09

### Added

- **Core Types** — `ResourceDefinition`, `FieldType`, `FieldSchema`, `EndpointSpec`, `AuthRule`, `RelationSpec`, `IndexSpec`, `CacheSpec`, `ShaperailError` with standardized error responses
- **YAML Parser** — Parse resource YAML files into typed Rust structs with semantic validation and human-readable error messages
- **Database Layer** — PostgreSQL via sqlx with typed queries, cursor/offset pagination, filtering (`?filter[role]=admin`), sorting (`?sort=-created_at`), and full-text search (`?search=term`)
- **REST Handlers** — Auto-generated Actix-web handlers with consistent JSON envelopes, field selection (`?fields=name,email`), relation loading (`?include=organization`), and bulk operations
- **Authentication** — JWT middleware, RBAC enforcement, owner checks, API key auth (`X-API-Key`), rate limiting (sliding window via Redis), token issuance and refresh
- **Redis Caching** — Response caching with automatic invalidation on writes, cache key scoping by resource/endpoint/query/role, bypass support
- **Background Jobs** — Redis-backed priority queues (critical/high/normal/low), exponential backoff retry, dead letter queue, job status tracking, configurable timeouts
- **WebSockets** — Channel-based real-time communication with room subscriptions, Redis pub/sub for multi-instance broadcast, heartbeat, lifecycle hooks
- **File Storage** — Multi-backend storage (local, S3, GCS, Azure) via `object_store` crate, image processing (resize/thumbnails), signed URLs, orphan cleanup
- **Events & Webhooks** — Async event emission, outbound webhooks with HMAC-SHA256 signing, retry with backoff, event log for audit, inbound webhook verification
- **CLI** — `shaperail init`, `generate`, `serve`, `build`, `validate`, `test`, `migrate`, `seed`, `export openapi`, `export sdk`, `doctor`, `routes`, `jobs:status`
- **Observability** — Structured JSON logging with request IDs, PII redaction, OpenTelemetry tracing, Prometheus metrics at `/metrics`, health checks at `/health` and `/health/ready`
- **OpenAPI Generation** — Deterministic OpenAPI 3.1 spec generation from resource definitions, TypeScript SDK generation

[0.3.0]: https://github.com/shaperail/shaperail/releases/tag/v0.3.0
[0.2.2]: https://github.com/shaperail/shaperail/releases/tag/v0.2.2
[0.2.1]: https://github.com/shaperail/shaperail/releases/tag/v0.2.1
[0.2.0]: https://github.com/shaperail/shaperail/releases/tag/v0.2.0
