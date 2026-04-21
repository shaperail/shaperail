# Changelog

All notable changes to Shaperail will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.0] - 2026-04-21

### Added

- `rate_limit: { max_requests: N, window_secs: N }` — per-endpoint rate limiting via Redis sliding window; declared in resource YAML alongside `cache:`; gracefully skipped when Redis is absent; startup warning logged when declared but Redis not configured
- `signature_header` on inbound webhook config — declare which HTTP header carries the HMAC-SHA256 signature; GitHub and Stripe headers auto-detected as fallback

### Changed

- **Controller registration** — auto-wired from resource YAML at startup; no manual `main.rs` wiring required
- **Background job worker** — auto-started with registered handlers derived from resource YAML; no manual `main.rs` wiring required
- **WebSocket channels** — routes auto-registered from `channels/*.yaml` files at startup
- **Inbound webhook routes** — auto-configured from `events.inbound:` in `shaperail.config.yaml`

### Fixed

- **Tenant isolation bypass** — users without a `tenant_id` JWT claim now receive `403 Forbidden` on all endpoints of a tenant-isolated resource (previously the check silently passed, allowing cross-tenant data access)

## [0.8.0] - 2026-04-20

### Changed

- Add shaperail llm-context command for project-aware LLM context dumps
- Add llm-guide.md and llm-reference.md — machine-readable context files for AI assistants
- Add JSON Schema for resource YAML (docs/schema/resource.schema.json)
- Add runnable incident platform example
- LLM anti-pattern audit: remove alternative syntax from examples, fix canonical format values
- Doc overhaul: pain-first homepage, three-tier feature list, nav cleanup


## [0.7.0] - 2026-03-17

### Added

- **Convention-based endpoint defaults** — `method` and `path` are now optional for the 5 standard CRUD actions (list, get, create, update, delete). Inferred from resource name, reducing tokens and typos.
- **`shaperail check [--json]`** — Structured diagnostics with stable error codes (SR001–SR072), fix suggestions, and inline YAML examples. `--json` for LLM consumption.
- **`shaperail explain <file>`** — Dry-run showing routes, table columns, indexes, and relations from a resource file.
- **`shaperail diff`** — Show what codegen would change without writing files.
- **`shaperail export json-schema`** — JSON Schema for resource YAML files, for IDE autocomplete and LLM validation.
- **Resource archetypes** — `shaperail resource create <name> --archetype <type>` with 5 built-in templates: basic, user, content, tenant, lookup.
- **Controller trait generation** — Codegen produces typed `{Resource}Controller` traits and `{Action}Input` structs. Compiler-enforced function signatures.
- **Feature flag guardrails** — `shaperail generate` warns when resources use upload/WASM/multi-DB without the matching Cargo feature enabled.
- **JSON Schema bundled in init** — `shaperail init` writes `resources/.schema.json` for yaml-language-server autocomplete.

### Changed

- `EndpointSpec.method` and `EndpointSpec.path` are now `Option<>` to support convention-based defaults.
- Scaffolded projects now declare `[features]` for graphql, grpc, and wasm-plugins with proper `#[cfg]` guards.
- All example resource YAML files simplified to use convention-based defaults.

## [0.6.0] - 2026-03-16

### Changed

- M17 Multi-Service: workspace YAML, service registry, typed inter-service clients, distributed sagas
- M18 Multi-Tenancy: tenant_key for automatic row-level isolation, scoped caching and rate limits
- M19 WASM Plugins: wasmtime runtime, sandboxed plugin execution, TypeScript/Python examples


## [0.5.0] - 2026-03-15

### Added

- **GraphQL (M15)** — Full GraphQL API from the same resource schema. Enable with `protocols: [rest, graphql]` in `shaperail.config.yaml`. Dynamic schema built at startup via async-graphql v7 — no hand-written GraphQL files.
  - **Query resolvers** — `list_<resource>` with filters, cursor pagination, and sorting; `<resource>(id)` for single records; nested relation resolvers for `belongs_to`, `has_many`, and `has_one`.
  - **Mutation resolvers** — `create_<resource>`, `update_<resource>`, `delete_<resource>` with the same auth rules, input validation, controller execution, and side effects (events, jobs, webhooks) as REST.
  - **Subscription resolvers** — generated from declared `events:` on endpoints, backed by broadcast channels.
  - **DataLoader** — automatic N+1 prevention for all relation queries, with per-request caching.
  - **Endpoints** — `POST /graphql` and `GET /graphql/playground` (self-contained, no external dependencies).
  - **Limits** — configurable `depth_limit` (default 16) and `complexity_limit` (default 256) to prevent DoS.
- **gRPC (M16)** — Full gRPC API from the same resource schema. Enable with `protocols: [rest, grpc]` in `shaperail.config.yaml`. Runs on a separate port (default `50051`).
  - **Proto generation** — `.proto` files auto-generated from resource schema with correct type mappings (uuid→string, timestamp→google.protobuf.Timestamp, json→Struct, etc.).
  - **Tonic server** — dynamic service dispatch routing `/<package>.<Service>/<Method>` to the correct resource handler.
  - **Streaming RPCs** — every `list` endpoint generates both a unary `List<Resource>` RPC and a server-streaming `Stream<Resource>` RPC.
  - **JWT auth** — extracted from `authorization` gRPC metadata, validated with the same `JwtConfig` as REST and GraphQL.
  - **Server reflection** — enabled by default (`grpc: { reflection: true }`), compatible with grpcurl and other tools.
  - **Health check** — `grpc.health.v1.Health` service with per-resource service status.
- **`GraphQLConfig`** — new config type: `graphql: { depth_limit: 10, complexity_limit: 200 }`.
- **`GrpcConfig`** — new config type: `grpc: { port: 50051, reflection: true }`.
- **`protocols` field** — new top-level config field: `protocols: [rest, graphql, grpc]`. Defaults to `["rest"]` when omitted.
- **Proto codegen** — `shaperail-codegen` now generates `.proto` files via `generate_proto()`.

### Changed

- `ProjectConfig` now has `protocols`, `graphql`, and `grpc` fields. All are optional with backward-compatible defaults — existing configs work unchanged.
- Documentation updated with GraphQL guide, gRPC guide, and configuration reference for both protocols.

## [0.4.0] - 2026-03-13

### Added

- **Multi-database (M14)** — Optional `databases:` in `shaperail.config.yaml` with named connections (e.g. `default`, `analytics`). Resources can set `db: <name>` to use a specific connection; omit for the default. When `databases` is set, the server uses an ORM-backed store (SeaORM) and runs migrations against the `default` connection.
- **`DatabaseEngine`** — Core enum: Postgres, MySQL, SQLite, MongoDB. Config supports `engine` and `url` per named database.
- **`DatabaseManager`** — Runtime connection manager for named SQL backends (Postgres wired; MySQL/SQLite config accepted, runtime support in progress).
- **Engine-specific migration SQL** — `build_create_table_sql_for_engine` for Postgres, MySQL, and SQLite dialect output.
- **ORM-backed CRUD path** — `OrmResourceQuery` and `OrmBackedStore`; `build_orm_store_registry(manager, resources)` builds a store registry when using `databases:`.
- **Scaffolded main** — When `config.databases` is present, app creates `DatabaseManager`, runs migrations on default DB URL, and uses ORM stores; otherwise keeps single-DB `generated::build_store_registry(pool)`.
- **Documentation** — Configuration reference documents `databases:` and `db:`; resource guide and Blog API example updated for multi-DB; index and reference pages mention multi-database.
- **GraphQL (M15)** — Optional GraphQL API from the same resource schema. Enable with `protocols: [rest, graphql]` in `shaperail.config.yaml`. Queries: list (filters, cursor pagination), get by id, nested relations (belongs_to, has_many, has_one). Mutations: create, update, delete with the same auth as REST (JWT, API key, RBAC, owner checks). `POST /graphql` and `GET /graphql/playground` for development. New [GraphQL guide](https://shaperail.io/graphql/) in the docs.

### Changed

- **BREAKING:** `ResourceDefinition` now has an optional `db: Option<String>` field. All struct literals in tests/benches were updated with `db: None`.
- **BREAKING:** `ProjectConfig` now has optional `databases: Option<IndexMap<String, NamedDatabaseConfig>>`. All config literals updated with `databases: None`.
- Blog API example and docs now show optional `db:` and commented `databases:` config.

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

[0.5.0]: https://github.com/shaperail/shaperail/releases/tag/v0.5.0
[0.4.0]: https://github.com/shaperail/shaperail/releases/tag/v0.4.0
[0.3.0]: https://github.com/shaperail/shaperail/releases/tag/v0.3.0
[0.2.2]: https://github.com/shaperail/shaperail/releases/tag/v0.2.2
[0.2.1]: https://github.com/shaperail/shaperail/releases/tag/v0.2.1
[0.2.0]: https://github.com/shaperail/shaperail/releases/tag/v0.2.0
[0.7.0]: https://github.com/shaperail/shaperail/releases/tag/v0.7.0
[0.6.0]: https://github.com/shaperail/shaperail/releases/tag/v0.6.0
[0.8.0]: https://github.com/shaperail/shaperail/releases/tag/v0.8.0
