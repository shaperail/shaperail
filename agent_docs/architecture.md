# SteelAPI Architecture

## Workspace Layout
```
steel-api/
├── Cargo.toml              # workspace root
├── CLAUDE.md               # AI context (always read first)
├── .claude/                # Claude Code config
│   ├── settings.json       # permissions + hooks
│   ├── commands/           # slash commands
│   ├── agents/             # subagent definitions
│   └── skills/             # auto-loaded context modules
├── agent_docs/             # detailed docs (read on demand)
├── steel-core/             # shared types and traits
├── steel-codegen/          # YAML → Rust generator
├── steel-runtime/          # Actix-web runtime
├── steel-cli/              # `steel` binary
├── migrations/             # sqlx migration files
├── resources/              # example .yaml resource files
└── examples/               # complete example projects
```

## Crate Dependency Graph
```
steel-cli
  └── steel-codegen
        └── steel-core
  └── steel-runtime
        └── steel-core
```
`steel-core` has no internal deps. `steel-codegen` and `steel-runtime` depend only on `steel-core`.

## steel-core — Shared Foundation
**Owns:** ResourceDefinition, FieldType, EndpointConfig, AuthRule, SteelError, all traits
**Does NOT own:** HTTP handlers, DB connections, codegen logic
Key types:
- `ResourceDefinition` — parsed + validated resource file
- `FieldSchema` — a single field with type, validation, metadata
- `EndpointSpec` — one endpoint (method, path, auth, hooks, pagination)
- `SteelError` — unified error enum used across all crates

## steel-codegen — The Generator
**Owns:** YAML parsing, schema validation, Rust code emission
**Does NOT own:** runtime behavior, actual HTTP serving
Key modules:
- `parser` — YAML → ResourceDefinition (uses serde + schemars)
- `validator` — semantic validation of parsed resource
- `emitter` — ResourceDefinition → Rust source code strings
- `migrator` — ResourceDefinition diff → SQL migration

Code generation rule: one resource file → one generated Rust module.
Generated code goes to `steel-runtime/src/generated/`.

## steel-runtime — The Server
**Owns:** Actix-web app factory, all HTTP handlers, middleware, DB pool, Redis client
**Does NOT own:** codegen, YAML parsing
Key modules:
- `app` — Actix-web App builder, middleware chain
- `handlers` — generated handler functions (CRUD, bulk, search)
- `middleware` — auth (JWT/RBAC), rate limiting, request ID
- `db` — sqlx pool, query helpers, transaction support
- `cache` — Redis client, TTL management, invalidation
- `jobs` — Redis job queue, worker, retry logic

## steel-cli — Developer Interface
Commands (v2 target):
- `steel init <name>`        — scaffold new SteelAPI project
- `steel generate <resource>` — run codegen for one resource file
- `steel generate --all`     — codegen for all resource files
- `steel migrate`            — generate + apply SQL migration
- `steel serve`              — start development server with hot reload
- `steel build`              — production build (single static binary)
- `steel validate`           — validate all resource files without generating
- `steel new resource <name>` — scaffold a new resource YAML file
