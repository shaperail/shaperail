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

## shaperail-codegen — The Generator
**Owns:** YAML parsing, schema validation, Rust code emission
**Does NOT own:** runtime behavior, actual HTTP serving
Key modules:
- `parser` — YAML → ResourceDefinition (uses serde + schemars)
- `validator` — semantic validation of parsed resource
- `emitter` — ResourceDefinition → Rust source code strings
- `migrator` — ResourceDefinition diff → SQL migration

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

## shaperail-cli — Developer Interface
Commands (v2 target):
- `shaperail init <name>`        — scaffold new Shaperail project
- `shaperail generate <resource>` — run codegen for one resource file
- `shaperail generate --all`     — codegen for all resource files
- `shaperail migrate`            — generate + apply SQL migration
- `shaperail serve`              — start development server with hot reload
- `shaperail build`              — production build (single static binary)
- `shaperail validate`           — validate all resource files without generating
- `shaperail new resource <name>` — scaffold a new resource YAML file
