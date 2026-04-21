# Docs Accuracy Fix + Version Bump Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix outdated feature-status descriptions in public docs, add missing `rate_limit` endpoint key to reference tables, fix wrong domain URLs, and bump the version to 0.9.0 with a CHANGELOG entry.

**Architecture:** Pure documentation and config changes — Markdown files, one YAML config, one TOML file, and one JSON file. No Rust code changes. No new files created.

**Tech Stack:** Markdown, Jekyll frontmatter, TOML, YAML, JSON.

---

## Files Modified

| File | Change |
|------|--------|
| `Cargo.toml` | `version = "0.9.0"` |
| `docs/_config.yml` | `release_version: 0.9.0` |
| `CHANGELOG.md` | Insert 0.9.0 entry |
| `docs/index.md` | Update "requires manual wiring" bullets; remove completed in-progress items |
| `docs/llm-reference.md` | Add `rate_limit` row to endpoint keys table |
| `docs/resource-guide.md` | Add `rate_limit` row to endpoint attributes table |
| `docs/getting-started.md` | `shaperail.dev` → `shaperail.io` |
| `docs/llm-guide.md` | `shaperail.dev` → `shaperail.io` |
| `docs/schema/resource.schema.json` | `shaperail.dev` → `shaperail.io` |

---

## Task 1: Version numbers and CHANGELOG

**Files:**
- Modify: `Cargo.toml` (workspace version field)
- Modify: `docs/_config.yml` (release_version field)
- Modify: `CHANGELOG.md` (insert new section at top)

---

- [ ] **Step 1: Bump workspace version in `Cargo.toml`**

Find the line `version = "0.8.0"` in `Cargo.toml` (it is in the `[workspace.package]` section) and change it to:

```toml
version = "0.9.0"
```

Verify:
```bash
grep '^version' Cargo.toml
```
Expected: `version = "0.9.0"`

- [ ] **Step 2: Bump `release_version` in `docs/_config.yml`**

Find the line `release_version: 0.8.0` in `docs/_config.yml` and change it to:

```yaml
release_version: 0.9.0
```

Verify:
```bash
grep 'release_version' docs/_config.yml
```
Expected: `release_version: 0.9.0`

- [ ] **Step 3: Add 0.9.0 entry to `CHANGELOG.md`**

`CHANGELOG.md` currently starts:

```markdown
# Changelog

All notable changes to Shaperail will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.8.0] - 2026-04-20
```

Insert the 0.9.0 section between the preamble and the 0.8.0 entry:

```markdown
## [0.9.0] - 2026-04-21

### Added

- `rate_limit: { max_requests: N, window_secs: N }` — per-endpoint rate limiting via Redis sliding window; declared in resource YAML alongside `cache:`; gracefully skipped when Redis is absent; startup warning logged when declared but Redis not configured
- `signature_header` on inbound webhook config — declare which HTTP header carries the HMAC-SHA256 signature; GitHub and Stripe headers auto-detected as fallback

### Changed

- **Controller registration** — auto-wired from resource YAML at startup; no manual `main.rs` wiring required
- **Background job worker** — auto-started with registered handlers derived from resource YAML; no manual `main.rs` wiring required
- **WebSocket channels** — routes auto-registered from `channels/*.yaml` files at startup
- **Inbound webhook routes** — auto-configured from `inbound_webhooks:` in `shaperail.config.yaml`

### Fixed

- **Tenant isolation bypass** — users without a `tenant_id` JWT claim now receive `403 Forbidden` on all endpoints of a tenant-isolated resource (previously the check silently passed, allowing cross-tenant data access)

```

Verify:
```bash
head -30 CHANGELOG.md
```
Expected: 0.9.0 entry appears before the 0.8.0 entry.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml docs/_config.yml CHANGELOG.md
git commit -m "chore: bump version to 0.9.0 and add CHANGELOG entry"
```

---

## Task 2: Update `docs/index.md` feature status

**Files:**
- Modify: `docs/index.md` (the "Available — requires manual wiring" section)

---

- [ ] **Step 1: Update the section intro text**

Find the paragraph that currently reads:

```
The runtime primitives exist and are documented. Background jobs, events, WebSockets, and API key auth require code in your `main.rs` to connect. GraphQL and gRPC require a Cargo feature flag and a `protocols:` config line, and have known feature gaps listed below. Each linked guide explains what works today.
```

Replace it with:

```
The runtime primitives exist and are documented. GraphQL and gRPC require a Cargo feature flag and a `protocols:` config line, and have known feature gaps listed below. Each linked guide explains what works today.
```

- [ ] **Step 2: Update the five changed bullets**

Find and replace the five outdated bullets. They currently read:

```markdown
- **Background jobs** — Queue and worker primitives; worker registration and handler mapping are manual ([Background jobs](/background-jobs/))
- **Events and webhooks** — Event emission from write handlers works; subscriber execution and inbound route registration are manual ([Events and webhooks](/events-and-webhooks/))
- **WebSockets** — Session and channel primitives work; route registration is manual ([WebSockets](/websockets/))
- **API key auth and rate limiting** — Runtime primitives exist; wiring to endpoints is manual ([Auth and ownership](/auth-and-ownership/))
- **Controllers** — Before/after business logic on write endpoints in Rust or WASM (TypeScript, Python, Rust, Go, or any WASM-targeting language); controller registration requires manual `main.rs` wiring ([Controllers](/controllers/))
```

Replace them with:

```markdown
- **Background jobs** — Queue, worker, and handler registration are fully auto-wired from resource YAML; enqueue jobs from `jobs:` on any write endpoint ([Background jobs](/background-jobs/))
- **Events and webhooks** — Event emission from write handlers and inbound webhook route registration are auto-configured; subscriber execution is still manual ([Events and webhooks](/events-and-webhooks/))
- **WebSockets** — Routes auto-registered from `channels/*.yaml` files at startup ([WebSockets](/websockets/))
- **Rate limiting** — Per-endpoint via `rate_limit: { max_requests: N, window_secs: N }` in resource YAML; requires Redis; startup warning when declared but Redis absent ([Auth and ownership](/auth-and-ownership/))
- **Controllers** — Before/after business logic on write endpoints in Rust or WASM (TypeScript, Python, Rust, Go, or any WASM-targeting language); auto-wired from resource YAML at startup ([Controllers](/controllers/))
```

Leave the GraphQL and gRPC bullets unchanged.

- [ ] **Step 3: Remove completed items from "In progress"**

Find the "In progress" section which currently reads:

```markdown
### In progress

- gRPC Update RPC
- WebSocket auto-routing from channel YAML files
- Events subscriber auto-execution
- Workspace service registry and saga orchestration
- Background job worker auto-registration
```

Replace it with:

```markdown
### In progress

- gRPC Update RPC
- Events subscriber auto-execution
- Workspace service registry and saga orchestration
```

- [ ] **Step 4: Verify**

```bash
grep -n "manual wiring\|manual\|In progress\|Background job worker auto\|WebSocket auto-routing" docs/index.md
```

Expected: No lines containing "Background job worker auto-registration" or "WebSocket auto-routing from channel YAML files". The words "manual" should only appear in the events/webhooks bullet ("subscriber execution is still manual") and in the section heading.

- [ ] **Step 5: Commit**

```bash
git add docs/index.md
git commit -m "docs: update feature status in index.md — controllers, jobs, WebSockets, rate limiting now auto-wired"
```

---

## Task 3: Add `rate_limit` to endpoint reference tables

**Files:**
- Modify: `docs/llm-reference.md` (endpoint keys table, after `upload` row)
- Modify: `docs/resource-guide.md` (endpoint attributes table, after `upload` row)

---

- [ ] **Step 1: Add `rate_limit` row to `docs/llm-reference.md`**

Find the endpoint keys table. The last rows before the blank line currently read:

```
| soft_delete |      |        |     |        | ✓      |        |
| upload      |      | ✓      |     |        |        |        |
| method      |      |        |     |        |        | ✓      |
| path        |      |        |     |        |        | ✓      |
```

Insert a new row after `upload`:

```
| soft_delete |      |        |     |        | ✓      |        |
| upload      |      | ✓      |     |        |        |        |
| rate_limit  | ✓    | ✓      | ✓   | ✓      | ✓      | ✓      |
| method      |      |        |     |        |        | ✓      |
| path        |      |        |     |        |        | ✓      |
```

- [ ] **Step 2: Add `rate_limit` row to `docs/resource-guide.md`**

Find the endpoint attributes table. The relevant rows currently read:

```
| `upload` | Multipart file upload config: `{ field: avatar, storage: s3, max_size: 5mb }` |
| `soft_delete` | When `true`, sets `deleted_at` instead of removing the row |
```

Insert a new row after `upload`:

```
| `upload` | Multipart file upload config: `{ field: avatar, storage: s3, max_size: 5mb }` |
| `rate_limit` | Per-endpoint rate limiting: `{ max_requests: 100, window_secs: 60 }`. Requires Redis. Silently skipped if Redis is not configured. |
| `soft_delete` | When `true`, sets `deleted_at` instead of removing the row |
```

- [ ] **Step 3: Verify**

```bash
grep -n "rate_limit" docs/llm-reference.md docs/resource-guide.md
```

Expected: one match in each file in the endpoint table section.

- [ ] **Step 4: Commit**

```bash
git add docs/llm-reference.md docs/resource-guide.md
git commit -m "docs: add rate_limit to endpoint keys tables in llm-reference and resource-guide"
```

---

## Task 4: Fix domain URLs

**Files:**
- Modify: `docs/getting-started.md` line 38
- Modify: `docs/llm-guide.md` line 10
- Modify: `docs/schema/resource.schema.json` line 3

---

- [ ] **Step 1: Fix `docs/getting-started.md`**

Find:
```
curl -fsSL https://shaperail.dev/install.sh | sh
```

Replace with:
```
curl -fsSL https://shaperail.io/install.sh | sh
```

- [ ] **Step 2: Fix `docs/llm-guide.md`**

Find:
```
**IDE validation:** Add `# yaml-language-server: $schema=https://shaperail.dev/schema/resource.schema.json` as the first line of any resource YAML file for inline validation.
```

Replace with:
```
**IDE validation:** Add `# yaml-language-server: $schema=https://shaperail.io/schema/resource.schema.json` as the first line of any resource YAML file for inline validation.
```

- [ ] **Step 3: Fix `docs/schema/resource.schema.json`**

Find:
```json
  "$id": "https://shaperail.dev/schema/resource.v1.json",
```

Replace with:
```json
  "$id": "https://shaperail.io/schema/resource.v1.json",
```

- [ ] **Step 4: Verify no remaining `shaperail.dev` references**

```bash
grep -rn "shaperail\.dev" docs/
```

Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add docs/getting-started.md docs/llm-guide.md docs/schema/resource.schema.json
git commit -m "docs: fix domain URLs shaperail.dev → shaperail.io"
```

---

## Self-Review

**Spec coverage:**
- ✅ Version bump (Cargo.toml + _config.yml) → Task 1 Steps 1–2
- ✅ CHANGELOG 0.9.0 entry → Task 1 Step 3
- ✅ docs/index.md section intro updated → Task 2 Step 1
- ✅ Five "requires manual wiring" bullets updated → Task 2 Step 2
- ✅ Two completed in-progress items removed → Task 2 Step 3
- ✅ rate_limit added to llm-reference.md → Task 3 Step 1
- ✅ rate_limit added to resource-guide.md → Task 3 Step 2
- ✅ Domain URLs fixed in 3 files → Task 4
- ✅ Navigation fixes: already applied — no task needed (verified by reading frontmatter)

**No placeholders found.**

**Content consistency:**
- `rate_limit` row in llm-reference.md marks ✓ for all 6 endpoint types — matches the implementation (all 9 handlers call `check_rate_limit`)
- Domain `shaperail.io` matches `docs/_config.yml` which already has `url: https://shaperail.io`
- CHANGELOG date `2026-04-21` matches today's date
