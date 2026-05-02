# Changelog

All notable changes to Shaperail will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking

- **List endpoints reject bracket-notation filter params for undeclared fields.** `validate_filter_param_form` in `shaperail-runtime/src/handlers/params.rs` was extended: in addition to the v0.11.3 `INVALID_FILTER_FORM` rejection of bare-field params that match a declared filter, the runtime now also returns **422** with code `UNDECLARED_FILTER` when a request sends `?filter[<field>]=<value>` and `<field>` is not in the endpoint's `filters:` list (or the endpoint declares no filters at all). The error message names the available filters when there are any, or notes that the endpoint declares none. Multiple offending keys accumulate into a single 422 response. Closes Issue H.

## [0.11.3] - 2026-05-02

> **Note on version label.** Both bullet points under **Breaking** below would normally warrant a minor bump under this project's pre-1.0 semver convention (breaking changes go to `0.x+1.0`). They shipped under `0.11.3` due to a release-PR race in the new release-plz pipeline: an empty release PR was created during the pipeline cutover and was merged in parallel with the `feat!:` PR, so the version computation never saw the breaking-change commits. The published `0.11.3` artifacts on crates.io contain the changes described here regardless of the version label. The next user-visible change will trigger a clean `0.12.0`.

### Breaking

- **`handler:` on convention action keys is now a hard validation error.** Declaring `handler: <fn>` on `list` / `get` / `update` / `create` / `delete` was previously silently dropped at codegen time — `collect_custom_handlers` filtered the entry out, the function was never registered, and the endpoint served the standard CRUD response. The new validator rule (`shaperail-codegen/src/validator.rs::validate_handler_only_on_custom`) rejects this with a clear error and a workaround that renames the endpoint key to a non-convention action (e.g. `post_<resource>`) with explicit `method:` / `path:`. To customize standard CRUD without replacing the runtime path, use `controller: { before: ... }` / `controller: { after: ... }` on the convention key. Closes Issue F.
- **List endpoints reject bare-field query params that match a declared filter.** The runtime convention has always been `?filter[<field>]=<value>`; bare `?<field>=<value>` was silently ignored, producing a structurally-correct-but-unfiltered response (a footgun that surfaced as phantom data leaks across tenants in tests). The new check (`shaperail-runtime/src/handlers/params.rs::validate_filter_param_form`) returns **422** with `INVALID_FILTER_FORM` and a "did you mean `?filter[<field>]=...`?" hint when a bare key exactly matches a declared `filters:` entry. Bare params that don't match any declared filter remain ignored without error. Closes Issue G.

### Changed

- **Release pipeline replaced with release-plz.** Every push to `main` runs `.github/workflows/release-plz.yml`, which opens a single auto-updated release PR and, on merge, publishes crates + tags + creates the GitHub Release. Cross-platform binaries are uploaded by `.github/workflows/release-binaries.yml` on `release: published`. The seven-place version-bump checklist, the local pre-release verification gate, and the manual `workflow_dispatch` release path are gone — release-plz manages workspace versions, internal `shaperail-*` dep versions, and the CHANGELOG. Authors only need conventional-commit PR titles (`feat:`, `fix:`, `feat!:`, etc.); release-plz does the rest. See `agent_docs/release.md` and the Release Process section of `CLAUDE.md`.
- CI: `ci.yml` `check` job no longer runs `cargo bench --no-run` and `cargo build --workspace` after `cargo clippy --all-targets` — the clippy invocation already type-checks tests, benches, and examples, so the follow-up steps were redundant.
- `docs/_config.yml` no longer hard-codes `release_version`; the Jekyll footer and `docs/index.md` link to the GitHub `releases/latest` page instead.

### Removed

- Deleted the custom release infrastructure: `.github/workflows/release.yml`, `.github/workflows/release-command.yml`, `.github/workflows/prepare-release.yml`, `.github/workflows/auto-release.yml`, `.github/ISSUE_TEMPLATE/release.md`, and the helper scripts under `.github/scripts/` (`assert-release-version.sh`, `check-pending-release.sh`, `extract-changelog-section.sh`, `publish-crates.sh`, `set-release-version.sh`). All of their responsibilities are now handled by release-plz.

## [0.11.2] - 2026-05-02

### Fixed

- **Custom handlers can now read the request body.** The custom-endpoint dispatch closure in `shaperail-runtime/src/handlers/routes.rs` had only `(req, state)` in its argument list — actix-web only extracts the request payload when an extractor is declared there, so `ServiceRequest.payload` was dropped before the handler ran and `req.take_payload()` returned `Payload::None` unconditionally. Any custom POST/PUT/PATCH handler trying to read a body got zero bytes regardless of `Content-Length`. The closure now also accepts `body: web::Bytes`; the runtime stashes the buffered bytes in `req.extensions_mut().insert(body)`, and custom handlers read them via `req.extensions().get::<web::Bytes>().cloned()`. Bodies larger than actix's default `PayloadConfig` limit (256 KB) still fail with 413 before the handler runs — configure that at the app level if you need bigger payloads. See `agent_docs/custom-handlers.md` for the full pattern.

## [0.11.1] - 2026-05-02

Patch release fixing five issues caught in the v0.11.0 follow-up review. The headline runtime additions from v0.11.0 (`test_support`, `Context.session`/`response_extras`, `Subject`) are unchanged in semantics; this release fixes bugs and design regressions in those features.

### Added

- **`controller: { before: ... }` is now supported on custom (non-CRUD) endpoints.** The runtime builds a `Context` with auto-populated `tenant_id` (from the auth subject + the resource's `tenant_key`), dispatches the before-hook, and stashes the resulting Context into `req.extensions_mut()` via `extensions_mut().insert(ctx)`. The custom handler can then read it: `req.extensions().get::<shaperail_runtime::handlers::controller::Context>().cloned()`. This eliminates the most common cross-tenant data-access bug class in custom handlers (forgetting to scope by `tenant_id`). `controller: { after: ... }` on custom endpoints remains rejected — custom handlers own their response shape, so the runtime has no place to merge `response_extras`. (Issues A and B in the v0.11.0 follow-up review, refining #1.)

### Changed (breaking, pre-1.0 cargo audit)

- **`shaperail_runtime::test_support::ensure_migrations_run` signature** now takes `migrations_dir: &Path`. The previous signature used the compile-time `sqlx::migrate!("../migrations")` macro, which baked in `shaperail-runtime`'s manifest dir at *its* compile time — so the function was effectively unusable from any external consumer (Issue C). The new signature uses the runtime `Migrator::new` API: pass `Path::new("./migrations")` from your crate root, or `concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")` for absolute resolution.
- **`shaperail_runtime::test_support::spawn_with_listener` factory contract** now accepts an async closure: `FnOnce(TcpListener) -> impl Future<Output = io::Result<Server>>`. The previous synchronous contract didn't compose with realistic `build_server` functions that connect a sqlx pool, generate OpenAPI, build registries, etc. (Issue D). Sync factories still work via `|l| std::future::ready(sync_build(l))`.

### Fixed

- **`Claims` test-token recipe is now discoverable on docs.rs.** The "Minting a test token" example was previously inside the struct rustdoc; it now also appears as a module-level doc on `shaperail_runtime::auth::jwt`, which renders at the top of the rendered module page. The `token_type` field doc additionally spells out the "must equal `\"access\"` for protected requests → 401" rule (Issue E).

### Migration

- Anyone calling `ensure_migrations_run(&pool)` must add the `migrations_dir` argument: `ensure_migrations_run(&pool, Path::new("./migrations"))`.
- Anyone calling `spawn_with_listener(listener, |l| sync_build(l))` must wrap the factory: `spawn_with_listener(listener, |l| async move { sync_build(l) })` or `|l| std::future::ready(sync_build(l))`.

In practice, the v0.11.0 versions of both functions were broken-by-design for the use cases they advertised, so existing code is unlikely to depend on them.

## [0.11.0] - 2026-05-02

### Breaking

- **`database:` (singular) config block removed** from `shaperail.config.yaml`. The block was parsed by `ProjectConfig` but never read at runtime — the runtime only ever consumed `databases:` (plural) or `DATABASE_URL`. Configs that retain the legacy block now fail to parse with a clear `unknown field 'database'` error. Migrate by replacing the block with `databases.default:` (preferred — see the new `shaperail init` template) or by relying on `DATABASE_URL` from `.env`. The `DatabaseConfig` type is also removed from `shaperail-core`.
- **`controller:` declared on a non-CRUD (custom) endpoint is now rejected** at validation time (`shaperail check`). The old behavior was a silent no-op — the runtime dispatched custom endpoints via `handler:` only and never invoked declared controllers. Move shared logic into the custom handler itself; use `shaperail_runtime::auth::Subject` for auth and tenant scoping (#1). (v0.11.1 partially relaxes this: `controller: { before: ... }` is now supported on custom endpoints.)

### Added

- **`shaperail_runtime::test_support`** — new module behind the `test-support` cargo feature, providing `TestServer`, `spawn_with_listener`, and `ensure_migrations_run`. Lets library consumers spin up the actix server in-process on an ephemeral port for integration tests, modeled on the zero2prod `TestApp` pattern (#4). See `agent_docs/testing-strategy.md` for the canonical lib/bin split that consumers should adopt to use it.
- **`shaperail_runtime::auth::Claims`** is now re-exported from the auth module so consumers minting tokens for tests can use the canonical struct directly. `Claims` rustdoc spells out the required claim shape and includes a test-token recipe (#10).
- **OpenAPI export** now emits `x-shaperail-auth: [<roles>]` as a vendor extension on each operation that declares non-public `auth:`. Matches the existing `x-shaperail-controller` / `x-shaperail-events` extension pattern. Standard `security:` is unchanged — roles are deliberately not stuffed into the OAuth-scopes array, which would mislead SDK generators that apply OAuth-flow code paths to non-empty scopes (#9).
- **`Context.response_extras`** — `serde_json::Map<String, Value>` field on `ControllerContext`. Merged into the response body's `data:` envelope after the after-hook returns; never persisted. Perfect for one-time fields like minted plaintext secrets that must reach the client exactly once (#2).
- **`Context.session`** — cross-phase scratch space on `ControllerContext`. Anything written in `before:` is visible in `after:` for the same request. Never persisted, never serialized to the client (#11).
- **`shaperail_runtime::auth::Subject`** — typed wrapper around the authenticated user with role/tenant accessors and `sqlx::QueryBuilder<Postgres>` integration. Use in custom handlers for explicit tenant scoping; CRUD endpoints continue to apply scoping automatically (#3).

### Changed

- **`Context` is preserved across `before:` and `after:` hooks** for the same request. Previously the runtime constructed a new `Context` for each phase, so state set in `before:` was not visible in `after:`. Now both phases share the same struct instance (#11). `input` is no longer reset between phases. The Context lifecycle is documented on the struct's rustdoc and in `agent_docs/hooks-system.md`.

### Fixed

- **`docker-compose.yml` Postgres healthcheck** no longer logs `FATAL: database "shaperail" does not exist` every 5 seconds (#7). The scaffolded healthcheck now reads `POSTGRES_USER` / `POSTGRES_DB` from the compose service environment, so it always probes the database that was actually created.
- **`shaperail init` scaffolded `shaperail.config.yaml`** now emits a working `databases.default:` block with `${DATABASE_URL:postgresql://localhost/<project>}` interpolation (and an inline comment explaining the override) instead of the old, dead singular `database:` block (#8). Fresh projects connect cleanly without manual `.env` editing.
- **JWT auth middleware logs structured warnings on rejection.** Previously, requests with a malformed/expired JWT or with `token_type != "access"` returned a silent 401 with no log line. The middleware now emits `tracing::warn!` lines with the rejection reason (`decode failed` or `token_type must be "access"`) and the rejected `sub`/`token_type` fields. External response is unchanged (#10).
- **`shaperail generate` output now passes `cargo fmt --check`.** Each generated `.rs` file is run through `rustfmt --edition 2021` post-write. Missing rustfmt on `PATH` is degraded to a warning rather than failing codegen (#5).
- **Generated list handlers no longer trip `cargo clippy -- -D warnings`** with `unused_variables: filters` (or, secondarily, `sort`). When a resource declares no `filters:` / `pagination.sort:` in YAML, the codegen now emits `let _ = filters;` / `let _ = sort;` at the top of the find_all body (#6).

## [0.10.1] - 2026-05-01

### Fixed

- **Cross-compile to `aarch64-unknown-linux-gnu` no longer requires system OpenSSL.** Switched `reqwest` from `native-tls` (default) to `rustls-tls`, dropping the `openssl-sys` dependency entirely. The 0.10.0 release shipped to crates.io but the GitHub Release binary build matrix failed because the cross-compile environment lacked ARM OpenSSL libraries. 0.10.1 is functionally identical to 0.10.0; the only change is the TLS backend.

## [0.10.0] - 2026-05-01

### Added

- **`transient: true` field flag** — input-only fields validated, exposed to the before-controller via `ctx.input`, never persisted (no migration column, no SQL reference), never returned in responses. Stripped from `ctx.input` automatically before INSERT/UPDATE.
- **Two-phase validation around the before-controller** for `create`, `update`, and `bulk_create`. `validate_input_shape()` runs before the controller (rule check on present fields); `validate_required_present()` runs after (required-presence check + rule check on injected keys). Lets `required: true` columns be populated by a `before:` controller without failing input validation.
- **`AppState::new(pool, resources)`** in `shaperail-runtime::handlers::crud` — defaults every optional subsystem to `None` and creates the broadcast bus. Scaffolds and tests no longer drift when `AppState` gains a field.
- **`strip_transient_fields()`** in `shaperail-runtime::handlers::validate` — removes transient keys from input data before persistence.
- Validator: rejects `transient: true` combined with `primary` / `generated` / `ref` / `unique` / `default`, and rejects transient fields not declared in any endpoint's `input:` (dead-field check).

### Fixed

- **`sensitive: true` is now honored at every codegen surface.** Previously parsed but ignored — sensitive fields leaked into JSON responses, OpenAPI response schemas, and TypeScript response types. The Rust response struct now emits `#[serde(skip_serializing)]`; OpenAPI and TypeScript response shapes omit them entirely. Request schemas keep them (a sensitive field can legitimately be an `input:`).
- **Validator typo: `soft_delete` checks for `deleted_at`** instead of the wrong column `updated_at`.
- **`handle_update` now runs validation.** Previously skipped validation entirely — partial updates with malformed input could reach the database. The full two-phase pipeline now runs.
- **`shaperail init` scaffold drift fixed** via `AppState::new(...)`. Adding a future field to `AppState` only requires updating the constructor; scaffolded projects keep compiling.
- **`wasmtime` upgraded to 44** to address [RUSTSEC-2026-0114](https://rustsec.org/advisories/RUSTSEC-2026-0114) (medium severity — panic when allocating a table exceeding host address space).

### Changed

- **`sensitive` documentation reconciled** across `docs/llm-guide.md`, `docs/resource-guide.md`, and `agent_docs/resource-format.md`. Three different definitions before; now consistent on "Omitted from all responses; redacted in logs and error messages".
- `agent_docs/resource-format.md` documents the new validation lifecycle and a `password` / `password_hash` worked example.

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
[0.9.0]: https://github.com/shaperail/shaperail/releases/tag/v0.9.0
[0.10.0]: https://github.com/shaperail/shaperail/releases/tag/v0.10.0
[0.10.1]: https://github.com/shaperail/shaperail/releases/tag/v0.10.1
[0.11.0]: https://github.com/shaperail/shaperail/releases/tag/v0.11.0
[0.11.1]: https://github.com/shaperail/shaperail/releases/tag/v0.11.1
[0.11.2]: https://github.com/shaperail/shaperail/releases/tag/v0.11.2
[0.11.3]: https://github.com/shaperail/shaperail/releases/tag/shaperail-cli-v0.11.3
