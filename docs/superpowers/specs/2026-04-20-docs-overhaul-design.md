# Documentation Overhaul Design — Approach B
**Date:** 2026-04-20
**Status:** Approved
**Scope:** Homepage rewrite, feature tier split, nav cleanup

---

## Goal

Make the public docs work equally well for two audiences:

1. **Evaluators** — developers deciding whether to adopt Shaperail. They need to immediately understand the problem the framework solves, what is production-ready today, and what requires extra wiring.
2. **Users** — developers already building with Shaperail. They need accurate, scannable reference material without noise from machine-targeted files or buried entry points.

---

## Problems Being Fixed

### 1. The tagline doesn't explain the product
"AI-native Rust backend framework" sounds like a framework that uses AI, not one designed for AI-assisted development. The actual pitch — write YAML, get a production-ready Rust REST API — is buried.

### 2. "Why Shaperail" explains design principles, not pain
The current table starts with "One source of truth / Explicit over implicit / Flat abstraction". These are architectural philosophy items. Evaluators need to understand the cost of NOT using Shaperail before they care about its design philosophy.

### 3. Feature list conflates production-ready with in-progress
Bullet items like "WebSockets — Runtime session/channel primitives exist, but the scaffold does not auto-load..." sit next to fully-working features with no visual distinction. Evaluators cannot tell what to count on.

### 4. LLM guide files appear in the user-facing nav
`llm-guide.md` and `llm-reference.md` are machine-targeted context files. They have no Jekyll frontmatter, so they appear in the sidebar as plain items alongside user-facing guides. Regular developers clicking them get a terse reference not intended for them.

### 5. Getting started is buried inside Guides
`getting-started.md` has `parent: Guides` and `nav_order: 1`, making it a child page under the Guides hub. For a framework's documentation, getting started should be a direct top-level item.

---

## Out of Scope

- Changes to individual guide/reference pages (separate audit pass)
- `blog-api-example.md` — it is a child of Examples and correctly linked from `examples.md`; no change needed
- Any content changes to LLM guide files — only their nav visibility changes

---

## Changes

### Change 1: Homepage opening and "Why Shaperail"

**File:** `docs/index.md`

**New opening line (replaces current H1 subtext):**
> Define your API as YAML resources. Shaperail generates the Rust backend — routes, database schema, validation, auth, migrations, and OpenAPI — from that one file.

**New "Why Shaperail" section — pain-first structure:**

Open with the problem statement: a single REST resource in plain Rust spans handler files, database models, migration SQL, validation logic, auth middleware, and OpenAPI annotations — typically 300–500 lines across 5+ files. Add another resource, repeat the work. Shaperail replaces that with one ~40-line YAML file. The framework reads that file and generates the Rust, the SQL, and the spec deterministically.

Follow with the existing principles table (One source of truth, Explicit over implicit, etc.) as supporting detail — it stays, but it supports the story rather than leading it.

Move "AI-native" language to a brief note at the end of the section: the docs, codegen, and runtime are kept in sync specifically so that LLMs can generate valid Shaperail resources and commands with minimal mistakes. This is what "AI-native" means in practice.

**"When to use Shaperail" table** — keep as-is, it is already well-written.

**"What you author" table** — remove the awkward caveat language in the Role column. Replace "One workable convention for controller modules; current apps still require manual controller registration" with "Business logic before/after DB writes — see [Controllers](/controllers/)" and trust the Controllers guide to explain registration.

---

### Change 2: Feature tier split

**File:** `docs/index.md` — replace the current "Features at a glance" section

Replace the single bullet list with three named tiers.

**Tier 1 — Production-ready today**
Everything in this tier works out of the box from a resource YAML file with no manual wiring:
- REST API: list, get, create, update, delete, bulk operations; cursor and offset pagination; filters, sort, full-text search; field selection; relation loading (`?include=`)
- JWT authentication; role-based and owner-based access control per endpoint
- Redis caching with TTL, auto-invalidation on writes, configurable `invalidate_on`
- File storage: local, S3, GCS, Azure; upload validation, signed URLs, image processing
- Multi-tenancy: row-level isolation via `tenant_key`; per-tenant cache and rate-limit keys; `super_admin` bypass
- Observability: structured JSON logs, request_id propagation, Prometheus metrics, health endpoints, OpenTelemetry trace export
- Database migrations: initial create-table SQL generated from schema; sqlx compile-time verified
- OpenAPI 3.1 spec generation; TypeScript SDK generation
- WASM plugins: controller hooks in TypeScript, Python, Rust, or any WASM-targeting language; sandboxed, fuel-limited, crash-isolated

**Tier 2 — Available, requires manual wiring**
The runtime primitives exist and are documented. Connecting them requires code in your app's `main.rs` or config. The relevant guide explains exactly what to wire and how:
- Background jobs — queue and worker primitives; worker registration and handler mapping are manual (see [Background jobs](/background-jobs/))
- Events and webhooks — event emission from write handlers works; subscriber execution and inbound route registration are manual (see [Events and webhooks](/events-and-webhooks/))
- WebSockets — session and channel primitives work; route registration is manual (see [WebSockets](/websockets/))
- API key auth — runtime primitive exists; wiring to endpoints is manual (see [Auth and ownership](/auth-and-ownership/))
- Rate limiting — primitive exists; wiring is manual (see [Auth and ownership](/auth-and-ownership/))
- GraphQL — enable with `protocols: [rest, graphql]`; generates list/get queries and create/update/delete mutations; list queries currently support `limit`/`offset` only (see [GraphQL](/graphql/))
- gRPC — enable with `protocols: [rest, grpc]`; supports list, stream, get, create, delete; `Update` RPC is not implemented (see [gRPC](/grpc/))

**Tier 3 — In progress**
These items have partial scaffolding or runtime stubs but are not complete:
- gRPC Update RPC
- WebSocket auto-routing from channel YAML files
- Events subscriber auto-execution
- Workspace service registry and saga orchestration
- Background job worker auto-registration

---

### Change 3: Nav cleanup

**Files:** `docs/llm-guide.md`, `docs/llm-reference.md`, `docs/getting-started.md`

**`llm-guide.md` and `llm-reference.md`:**
Add Jekyll frontmatter with `nav_exclude: true`. The files remain accessible by direct URL. Add a brief note at the bottom of the "Why Shaperail" section on `docs/index.md` that links to them: "Working with an LLM? Load [llm-guide.md](/llm-guide/) as context — it is the sole file an AI assistant needs to generate valid Shaperail resources." They should not be in the sidebar because they are not written for human navigation.

```yaml
---
title: Shaperail LLM Guide
nav_exclude: true
---
```

```yaml
---
title: Shaperail Quick Reference
nav_exclude: true
---
```

**`getting-started.md`:**
Remove `parent: Guides`. Set `nav_order: 2` so it appears as a top-level nav item directly after the homepage. Renumber the Guides hub to `nav_order: 3`, Reference to `nav_order: 4`, Examples to `nav_order: 5`.

```yaml
---
title: Getting started
nav_order: 2
---
```

---

## Files Changed

| File | Type of change |
|------|----------------|
| `docs/index.md` | Rewrite opening, Why section, feature tiers, What you author table |
| `docs/getting-started.md` | Remove `parent: Guides`, set `nav_order: 2` |
| `docs/guides.md` | `nav_order: 2` → `3` |
| `docs/reference.md` | `nav_order: 3` → `4` |
| `docs/examples.md` | `nav_order: 4` → `5` |
| `docs/llm-guide.md` | Add `nav_exclude: true` frontmatter |
| `docs/llm-reference.md` | Add `nav_exclude: true` frontmatter |

---

## Success Criteria

- A developer landing on the homepage can state in one sentence what Shaperail does and what problem it solves within 10 seconds of reading
- The features section makes it unambiguous what requires manual work vs. what works from YAML alone
- `llm-guide.md` and `llm-reference.md` no longer appear in the sidebar
- Getting started is a direct top-level nav item
