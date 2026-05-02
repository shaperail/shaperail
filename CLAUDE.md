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
version: 1            # drives route prefix: /v1/...

schema:
  id:        { type: uuid, primary: true, generated: true }
  email:     { type: string, format: email, unique: true, required: true }
  name:      { type: string, min: 1, max: 200, required: true }
  role:      { type: enum, values: [admin, member, viewer], default: member }
  org_id:    { type: uuid, ref: organizations.id, required: true }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }

# All paths below are auto-prefixed with /v{version} (e.g. /v1/users)
# Convention-based defaults: for list/get/create/update/delete,
# method and path are inferred automatically. Override only if needed.
endpoints:
  list:
    auth: [member, admin]
    filters: [role, org_id]
    search: [name, email]
    pagination: cursor
    cache: { ttl: 60 }

  create:
    auth: [admin]
    input: [email, name, role, org_id]
    controller: { before: validate_org }   # replaces hooks — see resources/users.controller.rs
    events: [user.created]
    jobs: [send_welcome_email]

  update:
    auth: [admin, owner]
    input: [name, role]

  delete:
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
- Install script: `curl -fsSL https://shaperail.io/install.sh | sh`

## Commands
```bash
cargo build --workspace          # build all crates
cargo test --workspace           # run all tests (452 as of v0.6.0)
cargo clippy -- -D warnings      # lint — must pass before every commit
cargo fmt                        # format — run after every edit
cargo bench -p shaperail-runtime # run performance benchmarks (no DB needed)
docker compose up -d             # start dev postgres + redis
docker compose down              # stop dev services
```

## AI-Native CLI Commands
```bash
shaperail check [path] --json    # structured diagnostics with error codes + fix suggestions
shaperail explain <file>         # dry-run: shows routes, table, relations from a resource
shaperail diff                   # shows what codegen would change (dry-run diff)
shaperail export json-schema     # JSON Schema for resource YAML (IDE/LLM validation)
shaperail resource create <name> --archetype <type>  # archetypes: basic, user, content, tenant, lookup
```

## Key Docs
- agent_docs/architecture.md      → crate structure + boundaries
- agent_docs/resource-format.md   → exact YAML spec (match PRD exactly)
- agent_docs/codegen-patterns.md  → Rust code generation patterns
- agent_docs/hooks-system.md      → Controller system (before/after) + ControllerContext
- agent_docs/testing-strategy.md  → what to test at each layer
- agent_docs/docker.md            → Docker dev + CI + release image setup
- agent_docs/release.md           → crates.io publish + GitHub Releases

## Documentation Rule

After every code change that alters behavior, CLI commands, APIs, configuration, error semantics, or test infrastructure, update the relevant docs BEFORE committing:

- `docs/` — **user-facing public documentation** (rendered to https://shaperail.io). CLI reference, guides, examples, recipes. Has Jekyll front matter (`title`, `parent`, `nav_order`).
- `agent_docs/` — **internal developer docs** (architecture, testing strategy, codegen patterns, hooks system). No front matter.
- `CLAUDE.md` — **project-level conventions and workflows** (this file). Update when the change affects how future Claude sessions should approach the codebase.

If docs and code disagree, fix the disagreement immediately (AI-First rule).

### Public-mirror requirement

`docs/` and `agent_docs/` are not optional alternatives — they're parallel audiences. **Every behavior change documented in `agent_docs/X.md` MUST have a corresponding update in `docs/`**, either in an existing user-facing page or as a new mirror page (`docs/X.md`). The internal note isn't enough; users reading the public site at shaperail.io need the same information in public-voice form.

Concretely, the checklist when you finish a code change:

1. ✅ Code change committed with tests.
2. ✅ `agent_docs/<relevant>.md` updated.
3. ✅ **Corresponding `docs/<page>.md` updated** — either an existing page got a new section, or a new mirror page was created with Jekyll front matter. If no public page is appropriate, write a one-line justification in the commit message saying so (very rare — most user-visible changes belong in public docs).
4. ✅ `CHANGELOG.md` `[Unreleased]` (or current version) section names the change.
5. ✅ If the change affects a release-process, validation rule, or codebase convention, update `CLAUDE.md`.

Examples of the mirror requirement in action:

| Change | `agent_docs/` page | `docs/` page |
|---|---|---|
| New runtime API (`Subject`, `Context.session`, `test_support`) | `agent_docs/auth-claims.md`, `agent_docs/custom-handlers.md`, `agent_docs/testing-strategy.md` | `docs/security.md`, `docs/custom-handlers.md`, `docs/testing.md` |
| Custom-handler body extraction (v0.11.2) | `agent_docs/custom-handlers.md` "Reading the request body" | `docs/custom-handlers.md` "Reading the request body" + callout in `docs/controllers.md` |
| New validator rule | `agent_docs/codegen-patterns.md` | inline note in `docs/resource-guide.md` |
| Breaking config change | `agent_docs/architecture.md` if structural | `docs/configuration.md` migration section |

When unsure where the public version belongs, default to creating a new `docs/<topic>.md` page with the same structure as `agent_docs/<topic>.md`, adapted for end-user voice (less internal jargon, more "how do I do this in my project" framing).

## Git Workflow
- Never start new feature work on `main`. Create and switch to a fresh branch first.
- Branch per milestone or feature: `git checkout -b feat/m01-core-types`
- Only commit when clippy + tests pass
- Commit format: `feat(shaperail-core): M01 — Core Types`
- **Never push more commits to a branch with an open PR you intend to merge as-is.** PRs auto-merge as soon as CI is green; follow-up commits frequently land too late and end up stranded on the merged branch. Open a new PR for follow-ups.

## Release Process — FOLLOW EVERY TIME

Releases ship via the `auto-release.yml` workflow on every push to `main`. The workflow checks whether the workspace version is ahead of the latest git tag; if yes, it tests + builds binaries (5 targets including the slow Windows MSVC) + publishes 4 crates to crates.io + creates a GitHub Release. Cross-platform binaries take ~15 minutes; the GitHub Release tag/page only appears at the end.

**Bumping the version requires updating SEVEN places.** `bash .github/scripts/assert-release-version.sh <version>` checks all of them. Run it locally before pushing — if it fails in CI the auto-release aborts and the next merge has to repeat the dance.

The seven places (use this checklist verbatim):

1. `Cargo.toml` — `[workspace.package] version = "X.Y.Z"`
2. `shaperail-cli/Cargo.toml` — `shaperail-codegen = { version = "X.Y.Z", ... }` (and `shaperail-core`, `shaperail-runtime`)
3. `shaperail-runtime/Cargo.toml` — `shaperail-core = { version = "X.Y.Z", ... }`
4. `shaperail-codegen/Cargo.toml` — `shaperail-core = { version = "X.Y.Z", ... }`
5. `docs/_config.yml` — `release_version: X.Y.Z` (Jekyll site footer; non-obvious; has bitten us)
6. `CHANGELOG.md` — section heading `## [X.Y.Z] - YYYY-MM-DD`
7. `CHANGELOG.md` — link reference at bottom: `[X.Y.Z]: https://github.com/shaperail/shaperail/releases/tag/vX.Y.Z`

**Pre-1.0 semver convention this project uses:**
- Patch (`0.11.0` → `0.11.1`): bug fixes, additive non-breaking features, documentation.
- Minor (`0.11.x` → `0.12.0`): breaking API or behavior changes (e.g., removing a public field, changing function signatures, validation rules that reject previously-accepted YAML).
- A change behind an opt-in feature flag (e.g. `test-support`) that nobody could have realistically used yet → patch is acceptable.

**Pre-release verification gate (all must pass before push):**

```
bash .github/scripts/assert-release-version.sh X.Y.Z
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo audit
cargo test --workspace --no-fail-fast --features test-support
```

The 44 `PoolTimedOut` failures in `api_integration` / `db_integration` / `grpc_service_tests` are pre-existing and acceptable when no Postgres is running locally.

**RUSTSEC advisories that block the release** must be either upgraded out (`cargo update -p <crate> --precise <version>`) or ignored in `.cargo/audit.toml` with a comment explaining the upstream block. Do NOT release without addressing them — `cargo audit` is part of the auto-release pipeline.

**CHANGELOG section structure:**

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Breaking
- ...

### Added
- ...

### Changed
- ...

### Fixed
- ...
```

Skip subsections that have no entries. Once a release is published, **never edit its CHANGELOG section** — it's frozen as historical record. New changes go in a new `[X.Y.Z+1]` section above.

**The PR title for a release commit MUST be descriptive of the user-facing change, not "release X.Y.Z".** The auto-merge squash uses the PR title as the commit message; "release X.Y.Z" loses the actual content. Example: `v0.11.2 patch — fix custom-handler body extraction (#issue)`.

**Verifying the release shipped:**

```
gh run list --workflow auto-release.yml --limit 1   # status: completed/success
gh release view vX.Y.Z                              # tag exists, all 5 binaries
cargo install shaperail-cli@X.Y.Z                   # crates.io has it
```

The first two only succeed once the slowest binary (Windows MSVC) finishes building, which can take up to 15 minutes after merge. crates.io publish happens earlier in the pipeline, so `cargo install` works before the GitHub Release page appears.
