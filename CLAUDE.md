# Shaperail — AI-Native Rust Backend Framework

## What We Are Building
This repo IS the framework itself. We are building the tool that gets published
to crates.io so others can install it with `cargo install shaperail-cli`.

The end-user experience we are building toward:
```bash
cargo install shaperail-cli
shaperail init my-app
cd my-app && shaperail serve   # full working API, zero boilerplate
```

## Main Goal
Make Shaperail the backend framework that an LLM can use with very low mistake
rates. A model trained on Shaperail docs, or given Shaperail docs in-context, should be
able to generate valid resources, config, and commands on the first pass.

## Core Value
Shaperail gives users a small, explicit schema language that expands into a
production-ready Rust backend. The value is not flexibility through many
equivalent options. The value is correctness through one canonical way,
deterministic generation, and loud failure on invalid input.

## What AI-First Means Here
- One canonical syntax per concept
- Docs, scaffolds, parser, codegen, and runtime must match exactly
- Unknown or unsupported fields must fail clearly
- Common CRUD and API tasks should take very few tokens
- Explicit declarations beat implicit framework behavior
- If docs and code disagree, fix the disagreement immediately

## Tech Stack (PRD-mandated — do not deviate)
| Component       | Library                        |
|-----------------|--------------------------------|
| HTTP            | Actix-web 4                    |
| Async           | Tokio                          |
| Serialization   | serde + serde_json             |
| Database        | sqlx (compile-time verified)   |
| Cache           | deadpool-redis                 |
| WebSockets      | Actix-web WS actors            |
| Jobs            | Custom Redis-backed queue      |
| Scheduler       | tokio-cron-scheduler           |
| File storage    | object_store crate             |
| Email           | lettre                         |
| Observability   | tracing + tracing-opentelemetry|
| OpenAPI         | Custom deterministic generator |

## Crate Responsibilities
- `shaperail-core`    → ResourceDefinition, FieldType, ShaperailError, all shared traits
- `shaperail-codegen` → YAML parser + Rust/SQL/OpenAPI generator
- `shaperail-runtime` → Actix-web server, handlers, middleware, DB, Redis, jobs
- `shaperail-cli`     → `shaperail` binary — the developer-facing tool

## Five Design Rules (NEVER violate — enforced by /run-checks)
1. ONE WAY — no aliases, no alternative syntax, no shortcuts
2. EXPLICIT OVER IMPLICIT — nothing executes unless declared in the resource file
3. FLAT ABSTRACTION — resource (layer 1) maps to runtime (layer 2). Max depth: 2
4. SCHEMA IS SOURCE OF TRUTH — generate all code FROM schema, never reverse
5. COMPILER AS SAFETY NET — every generated Rust file must compile + pass clippy

## Exact Resource File Format (from PRD — this is what users write)
```yaml
resource: users       # "resource:" key, not "name:"
version: 1

schema:
  id:        { type: uuid, primary: true, generated: true }
  email:     { type: string, format: email, unique: true, required: true }
  name:      { type: string, min: 1, max: 200, required: true }
  role:      { type: enum, values: [admin, member, viewer], default: member }
  org_id:    { type: uuid, ref: organizations.id, required: true }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }

endpoints:
  list:
    method: GET
    path: /users
    auth: [member, admin]
    filters: [role, org_id]
    search: [name, email]
    pagination: cursor
    cache: { ttl: 60 }

  create:
    method: POST
    path: /users
    auth: [admin]
    input: [email, name, role, org_id]
    hooks: [validate_org]
    events: [user.created]
    jobs: [send_welcome_email]

  update:
    method: PATCH
    path: /users/:id
    auth: [admin, owner]
    input: [name, role]

  delete:
    method: DELETE
    path: /users/:id
    auth: [admin]
    soft_delete: true

relations:
  organization: { resource: organizations, type: belongs_to, key: org_id }
  orders:       { resource: orders, type: has_many, foreign_key: user_id }

indexes:
  - fields: [org_id, role]
  - fields: [created_at], order: desc
```

## Performance Targets (PRD-mandated — must pass benchmarks before release)
- Simple JSON response: 150,000+ req/s
- DB read cached: 80,000+ req/s, P99 < 2ms
- DB write: 20,000+ req/s, P99 < 10ms
- Idle memory: ≤ 60 MB
- Release binary size: < 20 MB
- Cold start: < 100ms

## Success Metrics (PRD-mandated)
- ≥ 90% of generated endpoints compile + pass tests on first try
- ≤ 75% fewer tokens than Express/FastAPI for identical CRUD
- 100% auto-generated valid OpenAPI 3.1 specs
- `shaperail init myapp && cd myapp && shaperail serve` works end-to-end

## Docker Requirements (PRD-mandated)
- `docker compose up` → starts Postgres + Redis for local dev
- Framework CI runs inside Docker (no local Rust install needed)
- `shaperail build --docker` → produces scratch-based image ≤ 25 MB for user apps

## Release Targets
- `shaperail-core`, `shaperail-codegen`, `shaperail-runtime`, `shaperail-cli` published to crates.io
- GitHub Releases with pre-built binaries for macOS, Linux, Windows
- Install script: `curl -fsSL https://shaperail.dev/install.sh | sh`

## Commands
```bash
cargo build --workspace          # build all crates
cargo test --workspace           # run all tests (385 as of v0.2.2)
cargo clippy -- -D warnings      # lint — must pass before every commit
cargo fmt                        # format — run after every edit
cargo bench -p shaperail-runtime # run performance benchmarks (no DB needed)
docker compose up -d             # start dev postgres + redis
docker compose down              # stop dev services
```

## All Milestones
agent_docs/milestones.md — 26 milestones across v2 / v3 / v4
Active: agent_docs/current-milestone.md

## Key Docs
- agent_docs/milestones.md        → all 26 milestones with full deliverables
- agent_docs/current-milestone.md → which milestone is active
- agent_docs/architecture.md      → crate structure + boundaries
- agent_docs/resource-format.md   → exact YAML spec (match PRD exactly)
- agent_docs/codegen-patterns.md  → Rust code generation patterns
- agent_docs/hooks-system.md      → HookContext + hook patterns
- agent_docs/testing-strategy.md  → what to test at each layer
- agent_docs/docker.md            → Docker dev + CI + release image setup
- agent_docs/release.md           → crates.io publish + GitHub Releases

## Documentation Rule
After every code change that alters behavior, CLI commands, APIs, or test
infrastructure, update the relevant docs BEFORE committing:
- `docs/` — user-facing documentation (CLI reference, guides, examples)
- `agent_docs/` — internal developer docs (architecture, testing strategy, milestones)
- `CLAUDE.md` — if the change affects project-level conventions or workflows

If docs and code disagree, fix the disagreement immediately (AI-First rule).

## Git Workflow
- Branch per milestone: `git checkout -b feat/m01-core-types`
- Only commit when clippy + tests pass
- Commit format: `feat(shaperail-core): M01 — Core Types`
