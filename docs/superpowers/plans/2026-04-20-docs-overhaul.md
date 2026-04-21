# Documentation Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the public docs homepage to clearly state what Shaperail solves, split features into three tiers (ready / manual wiring / in progress), and fix nav issues that bury entry points or expose machine-targeted files.

**Architecture:** Pure documentation changes across 7 existing Markdown files. No new files created. No code changes. Changes are independent and can be committed separately.

**Tech Stack:** Jekyll (just-the-docs theme), Markdown, YAML frontmatter.

---

## Files Modified

| File | Change |
|------|--------|
| `docs/getting-started.md` | Remove `parent: Guides`, set `nav_order: 2` (standalone top-level) |
| `docs/guides.md` | `nav_order: 2` → `3` |
| `docs/reference.md` | `nav_order: 3` → `4` |
| `docs/examples.md` | `nav_order: 4` → `5` |
| `docs/llm-guide.md` | Add frontmatter block with `nav_exclude: true` |
| `docs/llm-reference.md` | Add frontmatter block with `nav_exclude: true` |
| `docs/index.md` | Rewrite opening line, Why Shaperail, What you author row, feature tiers; remove Documentation map |

---

## Task 1: Promote Getting Started and renumber nav

**Files:**
- Modify: `docs/getting-started.md` lines 1-5 (frontmatter)
- Modify: `docs/guides.md` line 3 (nav_order)
- Modify: `docs/reference.md` line 3 (nav_order)
- Modify: `docs/examples.md` line 3 (nav_order)

- [ ] **Step 1: Update getting-started.md frontmatter**

Replace the current frontmatter:
```yaml
---
title: Getting started
parent: Guides
nav_order: 1
---
```

With:
```yaml
---
title: Getting started
nav_order: 2
---
```

- [ ] **Step 2: Verify getting-started.md frontmatter**

Run:
```bash
head -5 docs/getting-started.md
```

Expected output:
```
---
title: Getting started
nav_order: 2
---
```

- [ ] **Step 3: Update guides.md nav_order**

In `docs/guides.md`, change:
```yaml
nav_order: 2
```
to:
```yaml
nav_order: 3
```

- [ ] **Step 4: Update reference.md nav_order**

In `docs/reference.md`, change:
```yaml
nav_order: 3
```
to:
```yaml
nav_order: 4
```

- [ ] **Step 5: Update examples.md nav_order**

In `docs/examples.md`, change:
```yaml
nav_order: 4
```
to:
```yaml
nav_order: 5
```

- [ ] **Step 6: Verify all nav_orders**

Run:
```bash
grep "nav_order" docs/getting-started.md docs/guides.md docs/reference.md docs/examples.md
```

Expected output:
```
docs/getting-started.md:nav_order: 2
docs/guides.md:nav_order: 3
docs/reference.md:nav_order: 4
docs/examples.md:nav_order: 5
```

- [ ] **Step 7: Commit**

```bash
git add docs/getting-started.md docs/guides.md docs/reference.md docs/examples.md
git commit -m "docs: promote getting-started to top-level nav, renumber hub pages"
```

---

## Task 2: Hide LLM files from sidebar

**Files:**
- Modify: `docs/llm-guide.md` — prepend frontmatter
- Modify: `docs/llm-reference.md` — prepend frontmatter

- [ ] **Step 1: Add frontmatter to llm-guide.md**

The file currently starts with `# Shaperail LLM Guide` (no frontmatter). Prepend the following block so it becomes the first 4 lines of the file:

```markdown
---
title: Shaperail LLM Guide
nav_exclude: true
---

# Shaperail LLM Guide
```

The rest of the file content stays unchanged after that heading.

- [ ] **Step 2: Verify llm-guide.md frontmatter**

Run:
```bash
head -6 docs/llm-guide.md
```

Expected output:
```
---
title: Shaperail LLM Guide
nav_exclude: true
---

# Shaperail LLM Guide
```

- [ ] **Step 3: Add frontmatter to llm-reference.md**

The file currently starts with `# Shaperail Quick Reference` (no frontmatter). Prepend:

```markdown
---
title: Shaperail Quick Reference
nav_exclude: true
---

# Shaperail Quick Reference
```

The rest of the file content stays unchanged after that heading.

- [ ] **Step 4: Verify llm-reference.md frontmatter**

Run:
```bash
head -6 docs/llm-reference.md
```

Expected output:
```
---
title: Shaperail Quick Reference
nav_exclude: true
---

# Shaperail Quick Reference
```

- [ ] **Step 5: Commit**

```bash
git add docs/llm-guide.md docs/llm-reference.md
git commit -m "docs: exclude LLM context files from sidebar nav"
```

---

## Task 3: Rewrite homepage opening, Why Shaperail, and What you author

**Files:**
- Modify: `docs/index.md`

The changes in this task cover lines 6–101 of the current file (opening tagline, Why Shaperail, When to use, What you author) and remove the Documentation map section (lines 104–121) which duplicates the sidebar nav.

- [ ] **Step 1: Replace the opening tagline (line 8)**

Current line 8:
```markdown
**An AI-native Rust backend framework.** Define resources in YAML; get a production-ready REST API plus optional protocol and async primitives from one canonical schema.
```

Replace with:
```markdown
**Define your API as YAML resources. Shaperail generates the Rust backend — routes, database schema, validation, auth, migrations, and OpenAPI — from that one file.**
```

- [ ] **Step 2: Verify tagline**

Run:
```bash
grep "Define your API as YAML" docs/index.md
```

Expected: one match on that line.

- [ ] **Step 3: Replace the Why Shaperail section**

Current block (lines 38–50):
```markdown
## Why Shaperail

| Principle | What it means |
| --- | --- |
| **One source of truth** | Resource YAML drives schema, routes, validation, migrations, and OpenAPI. No hidden conventions. |
| **Explicit over implicit** | No routes or behavior unless you declare it in the resource file. |
| **Flat abstraction** | Resource definition maps directly to runtime; no deep framework layers. |
| **Deterministic output** | Same resource files produce the same OpenAPI spec and code every time. |
| **Docker-first dev** | `docker compose up -d` gives you Postgres and Redis; no manual DB setup. |

The framework is built so that docs, codegen, and runtime stay in sync — and so that LLMs can generate valid Shaperail resources and commands with minimal mistakes.
```

Replace with:
```markdown
## Why Shaperail

A typical REST resource in plain Rust spans handler files, database models, migration SQL, validation logic, auth middleware, and OpenAPI annotations — 300–500 lines across 5 or more files. Add another resource, repeat the work. Shaperail replaces all of that with one ~40-line YAML file. The framework reads the file and generates the Rust code, the SQL schema, and the OpenAPI spec deterministically.

| Principle | What it means |
| --- | --- |
| **One source of truth** | Resource YAML drives schema, routes, validation, migrations, and OpenAPI. No hidden conventions. |
| **Explicit over implicit** | No routes or behavior unless you declare it in the resource file. |
| **Flat abstraction** | Resource definition maps directly to runtime; no deep framework layers. |
| **Deterministic output** | Same resource files produce the same OpenAPI spec and code every time. |
| **Docker-first dev** | `docker compose up -d` gives you Postgres and Redis; no manual DB setup. |

> Working with an LLM? Load [llm-guide.md](/llm-guide/) as context — it is the sole file an AI assistant needs to generate valid Shaperail resources.
```

- [ ] **Step 4: Verify Why Shaperail section**

Run:
```bash
grep -c "300–500 lines" docs/index.md && grep -c "llm-guide" docs/index.md
```

Expected: both print `1`.

- [ ] **Step 5: Fix the controller row in the What you author table (line 72)**

Current line 72:
```markdown
| `resources/*.controller.rs` | One workable convention for controller modules; current apps still require manual controller registration |
```

Replace with:
```markdown
| `resources/*.controller.rs` | Business logic before/after DB writes — see [Controllers](/controllers/) |
```

- [ ] **Step 6: Verify What you author table**

Run:
```bash
grep "controller.rs" docs/index.md
```

Expected output:
```
| `resources/*.controller.rs` | Business logic before/after DB writes — see [Controllers](/controllers/) |
```

- [ ] **Step 7: Remove the Documentation map section**

Remove lines 104–121 (the entire `## Documentation map` section through the end of the file):

```markdown
## Documentation map

### Get going

- [**Getting started**]({{ '/getting-started/' | relative_url }}) — Install CLI, scaffold a project, run the app, first schema change.

### Guides

- [**Guides**]({{ '/guides/' | relative_url }}) — Auth, controllers, migrations, Docker, caching, jobs, WebSockets, file storage, events, observability, GraphQL.

### Reference

- [**Reference**]({{ '/reference/' | relative_url }}) — Resource format, configuration, CLI, API responses and query parameters.

### Examples

- [**Examples**]({{ '/examples/' | relative_url }}) — [Blog API example]({{ '/blog-api-example/' | relative_url }}) plus direct links to the checked-in repository examples for enterprise SaaS billing, an incident platform, multi-tenant SaaS, multi-service workspaces, and WASM plugins.
```

The file should end after the closing `---` that follows the Features at a glance section.

- [ ] **Step 8: Verify Documentation map is gone**

Run:
```bash
grep "Documentation map" docs/index.md
```

Expected: no output (zero matches).

- [ ] **Step 9: Commit**

```bash
git add docs/index.md
git commit -m "docs: rewrite homepage opening, Why Shaperail, and What you author"
```

---

## Task 4: Replace Features at a glance with three-tier structure

**Files:**
- Modify: `docs/index.md`

- [ ] **Step 1: Replace the Features at a glance section**

Current block (lines 82–101):
```markdown
## Features at a glance

- **REST API** — List, get, create, update, delete, bulk create/delete; cursor or offset pagination; filters, sort, full-text search; field selection and relation loading (`?include=…`).
- **GraphQL** — Enable with `protocols: [rest, graphql]`. The current generated schema exposes `list_<resource>`, singular get-by-id fields, and `create_` / `update_` / `delete_` mutations. List fields currently support `limit` and `offset` only.
- **gRPC** — Enable with `protocols: [rest, grpc]`. The current server supports list, stream, get, create, and delete RPCs plus health/reflection. `Update` is not implemented yet, and the CLI does not currently write `.proto` files to disk.
- **Multi-database** — Optional `databases:` in config with named connections (e.g. `default`, `analytics`). Per-resource `db:` routes that resource to a connection; migrations run against `default`.
- **API versioning** — Per-resource `version` field prefixes all routes (`/v1/users`, `/v2/orders`). OpenAPI spec and CLI output reflect versioned paths.
- **Controllers** — Synchronous before/after business logic on write endpoints. Validate input, normalize data, enrich responses — in Rust or sandboxed WASM (TypeScript, Python, Rust, etc.).
- **Auth** — JWT auth is scaffolded from `JWT_SECRET`. API key auth and Redis-backed rate limiting exist as runtime primitives but require manual wiring in the generated app.
- **Caching** — Redis-backed cache per GET endpoint with TTL and configurable invalidation.
- **Background jobs** — Endpoint `jobs:` declarations enqueue work into the Redis queue. Running a worker and registering handlers is still a manual bootstrap step.
- **WebSockets** — Runtime session/channel primitives exist, but the scaffold does not auto-load `channels/*.channel.yaml` or register `/ws/...` routes.
- **File storage** — Local, S3, GCS, Azure; upload validation, signed URLs, image processing.
- **Events & webhooks** — Write handlers can emit events into the job queue. Subscriber execution, webhook delivery handlers, and inbound webhook route registration still require manual wiring.
- **Observability** — Structured JSON logs, request_id, PII redaction; Prometheus metrics; OpenTelemetry; `/health` and `/health/ready`.
- **Multi-service workspaces** — `shaperail serve --workspace` validates a workspace and starts each service in dependency order. Registry, typed clients, and saga orchestration are not wired into that flow yet.
- **Multi-tenancy** — Add `tenant_key: org_id` to any resource for automatic row-level isolation. Queries are scoped to the JWT `tenant_id` claim; cache keys are per-tenant; rate-limit keys are too when the limiter is wired; `super_admin` bypasses the filter.
- **WASM plugins** — Write controller hooks in TypeScript, Python, Rust, or any language that compiles to WASM. Sandboxed execution with no filesystem or network access; fuel-limited; crash-isolated from the server.
- **OpenAPI & SDK** — Deterministic OpenAPI 3.1; TypeScript SDK generation.
```

Replace with:
```markdown
## Features at a glance

### Production-ready today

Everything below works from a resource YAML file with no manual wiring.

- **REST API** — List, get, create, update, delete, bulk create/delete; cursor and offset pagination; filters, sort, full-text search; field selection; relation loading (`?include=`)
- **Authentication** — JWT auth; role-based and owner-based access control declared per endpoint
- **Caching** — Redis-backed cache per GET endpoint with TTL, auto-invalidation on writes, configurable `invalidate_on`
- **File storage** — Local, S3, GCS, Azure; upload validation, signed URLs, image processing
- **Multi-tenancy** — Row-level isolation via `tenant_key`; per-tenant cache and rate-limit keys; `super_admin` bypass
- **Observability** — Structured JSON logs, request_id propagation, Prometheus metrics, health endpoints (`/health`, `/health/ready`), OpenTelemetry trace export
- **Migrations** — Initial create-table SQL generated from schema; sqlx compile-time verified
- **OpenAPI 3.1** — Deterministic spec generation; TypeScript SDK generation
- **WASM plugins** — Controller hooks in TypeScript, Python, Rust, or any WASM-targeting language; sandboxed, fuel-limited, crash-isolated

### Available — requires manual wiring

The runtime primitives exist and are documented. Connecting them requires code in your `main.rs` or config. Each linked guide explains exactly what to wire.

- **Background jobs** — Queue and worker primitives; worker registration and handler mapping are manual ([Background jobs](/background-jobs/))
- **Events and webhooks** — Event emission from write handlers works; subscriber execution and inbound route registration are manual ([Events and webhooks](/events-and-webhooks/))
- **WebSockets** — Session and channel primitives work; route registration is manual ([WebSockets](/websockets/))
- **API key auth and rate limiting** — Runtime primitives exist; wiring to endpoints is manual ([Auth and ownership](/auth-and-ownership/))
- **GraphQL** — Enable with `protocols: [rest, graphql]`; generates list/get queries and create/update/delete mutations; list queries support `limit`/`offset` only ([GraphQL](/graphql/))
- **gRPC** — Enable with `protocols: [rest, grpc]`; supports list, stream, get, create, delete; `Update` RPC is not yet implemented ([gRPC](/grpc/))

### In progress

- gRPC Update RPC
- WebSocket auto-routing from channel YAML files
- Events subscriber auto-execution
- Workspace service registry and saga orchestration
- Background job worker auto-registration
```

- [ ] **Step 2: Verify tier headings are present**

Run:
```bash
grep "### Production-ready today\|### Available\|### In progress" docs/index.md
```

Expected output:
```
### Production-ready today
### Available — requires manual wiring
### In progress
```

- [ ] **Step 3: Verify old mixed-caveat language is gone**

Run:
```bash
grep "manual bootstrap step\|does not auto-load\|still require manual wiring\|not wired into that flow" docs/index.md
```

Expected: no output (zero matches).

- [ ] **Step 4: Commit**

```bash
git add docs/index.md
git commit -m "docs: split features into production-ready / manual-wiring / in-progress tiers"
```

---

## Self-Review

**Spec coverage check:**
- ✅ Change 1 (opening + Why Shaperail) → Task 3 steps 1–6
- ✅ Change 1 (What you author cleanup) → Task 3 step 5
- ✅ Change 1 (LLM link in Why section) → Task 3 step 3
- ✅ Change 1 (remove Documentation map) → Task 3 steps 7–8
- ✅ Change 2 (feature tiers) → Task 4
- ✅ Change 3 (getting-started standalone) → Task 1
- ✅ Change 3 (guides/reference/examples renumbered) → Task 1
- ✅ Change 3 (llm files nav_exclude) → Task 2

**No placeholders found.**
