# Shaperail Architecture

## Workspace Layout
```
shaperail/
├── Cargo.toml              # workspace root
├── CLAUDE.md               # AI context (always read first)
├── .claude/                # Claude Code config
│   ├── settings.json       # permissions + hooks
│   ├── commands/           # slash commands
│   ├── agents/             # subagent definitions
│   └── skills/             # auto-loaded context modules
├── agent_docs/             # detailed docs (read on demand)
├── shaperail-core/             # shared types and traits
├── shaperail-codegen/          # YAML → Rust generator
├── shaperail-runtime/          # Actix-web runtime
├── shaperail-cli/              # `shaperail` binary
├── migrations/             # sqlx migration files
├── resources/              # example .yaml resource files
└── examples/               # complete example projects
```

## Crate Dependency Graph
```
shaperail-cli
  └── shaperail-codegen
        └── shaperail-core
  └── shaperail-runtime
        └── shaperail-core
```
`shaperail-core` has no internal deps. `shaperail-codegen` and `shaperail-runtime` depend only on `shaperail-core`.

## shaperail-core — Shared Foundation
**Owns:** ResourceDefinition, FieldType, EndpointConfig, AuthRule, ShaperailError, all traits
**Does NOT own:** HTTP handlers, DB connections, codegen logic
Key types:
- `ResourceDefinition` — parsed + validated resource file
- `FieldSchema` — a single field with type, validation, metadata
- `EndpointSpec` — one endpoint (method, path, auth, hooks, pagination)
- `ShaperailError` — unified error enum used across all crates
- `WorkspaceConfig` — multi-service workspace definition (M17)
- `ServiceDefinition` — a single service within a workspace (M17)
- `ServiceRegistryEntry` — Redis-stored service discovery record (M17)
- `SagaDefinition` — distributed saga with compensating steps (M17)
- `ResourceDefinition.tenant_key` — multi-tenancy isolation key (M18)

## shaperail-codegen — The Generator
**Owns:** YAML parsing, schema validation, Rust code emission
**Does NOT own:** runtime behavior, actual HTTP serving
Key modules:
- `parser` — YAML → ResourceDefinition (uses serde + schemars)
- `validator` — semantic validation of parsed resource
- `emitter` — ResourceDefinition → Rust source code strings
- `migrator` — ResourceDefinition diff → SQL migration
- `workspace_parser` — YAML → WorkspaceConfig + SagaDefinition (M17)
- `service_client` — typed inter-service client code generation (M17)

Code generation rule: one resource file → one generated Rust module.
Generated code goes to `shaperail-runtime/src/generated/`.

## shaperail-runtime — The Server
**Owns:** Actix-web app factory, all HTTP handlers, middleware, DB pool, Redis client
**Does NOT own:** codegen, YAML parsing
Key modules:
- `app` — Actix-web App builder, middleware chain
- `handlers` — generated handler functions (CRUD, bulk, search)
- `middleware` — auth (JWT/RBAC), rate limiting, request ID
- `db` — sqlx pool, query helpers, transaction support
- `cache` — Redis client, TTL management, invalidation
- `jobs` — Redis job queue, worker, retry logic
- `plugins` — WASM plugin runtime with sandboxing (M19)
- `registry` — Redis-backed service registry with heartbeat (M17)

## shaperail-cli — Developer Interface
Commands (v2 — all implemented):
- `shaperail init <name>`        — scaffold new Shaperail project
- `shaperail generate`           — run codegen for all resource files
- `shaperail validate [path]`    — validate resource files without generating
- `shaperail migrate`            — generate + apply SQL migration
- `shaperail migrate --rollback` — rollback last migration batch
- `shaperail seed [path]`        — load YAML fixtures into DB via transaction
- `shaperail serve`              — start development server with hot reload
- `shaperail serve --check`      — validate project without starting server
- `shaperail serve --workspace`  — start all services in a workspace (M17)
- `shaperail build`              — production build (single static binary)
- `shaperail build --docker`     — scratch-based Docker image ≤ 25 MB
- `shaperail test`               — run all tests
- `shaperail export openapi`     — output OpenAPI 3.1 spec
- `shaperail export sdk --lang ts` — generate TypeScript SDK
- `shaperail doctor`             — check system deps
- `shaperail routes`             — print all routes with auth requirements
- `shaperail jobs:status`        — show Redis queue depths and dead letter count
