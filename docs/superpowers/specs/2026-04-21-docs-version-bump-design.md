# Docs Accuracy Fix + Version Bump — Design Spec
**Date:** 2026-04-21
**Status:** Approved
**Scope:** Fix outdated/invalid content in public docs, eliminate table inconsistencies, execute existing navigation plan, and bump version to 0.9.0.

---

## Problem

The `feat/wiring-gaps` branch shipped five features that were previously documented as "requires manual wiring." The public docs still describe them as manual. The endpoint keys tables in `llm-reference.md` and `resource-guide.md` are missing the `rate_limit` field added in this branch. The navigation plan from `docs/superpowers/plans/2026-04-20-docs-overhaul.md` was never executed. Version numbers are stale at 0.8.0.

---

## Out of Scope

- Rewriting any doc pages from scratch
- Changing the docs theme or site structure
- Adding new documentation pages
- Fixing the GraphQL/gRPC feature gaps (documented separately as in-progress)
- Subscriber execution auto-wiring (not yet implemented)

---

## Changes

### 1. `docs/index.md` — Feature status corrections

**"Available — requires manual wiring" section** — update five entries:

| Old text | New text |
|---|---|
| Background jobs — worker registration and handler mapping are manual | Background jobs — queue, worker, and handler registration are fully auto-wired from resource YAML |
| Events and webhooks — subscriber execution and inbound route registration are manual | Events and webhooks — event emission and inbound webhook route registration are auto-configured; subscriber execution is still manual |
| WebSockets — route registration is manual | WebSockets — routes auto-registered from `channels/*.yaml` files |
| API key auth and rate limiting — wiring to endpoints is manual | Rate limiting — per-endpoint via `rate_limit: { max_requests: N, window_secs: N }` in resource YAML; requires Redis |
| Controllers — controller registration requires manual `main.rs` wiring | Controllers — auto-wired from resource YAML; no `main.rs` changes required |

**"In progress" section** — remove two completed items:
- Remove: "WebSocket auto-routing from channel YAML files"
- Remove: "Background job worker auto-registration"

Remaining in-progress items stay as-is.

### 2. `docs/llm-reference.md` — Add `rate_limit` to endpoint keys table

Add row after `upload`:

```
| rate_limit  | ✓    | ✓      | ✓   | ✓      | ✓      | ✓      |
```

### 3. `docs/resource-guide.md` — Add `rate_limit` to endpoint attributes table

Add row after `upload`:

```
| `rate_limit` | Per-endpoint rate limiting: `{ max_requests: 100, window_secs: 60 }`. Requires Redis. Silently skipped if Redis is not configured. |
```

### 4. Navigation fixes (from `docs/superpowers/plans/2026-04-20-docs-overhaul.md`)

| File | Change |
|---|---|
| `docs/getting-started.md` | Remove `parent: Guides`; set `nav_order: 2` |
| `docs/guides.md` | `nav_order: 2` → `3` |
| `docs/reference.md` | `nav_order: 3` → `4` |
| `docs/examples.md` | `nav_order: 4` → `5` |
| `docs/llm-guide.md` | Add `nav_exclude: true` to frontmatter |

`docs/llm-reference.md` already has `nav_exclude: true` — no change needed.

### 5. Domain URL fix

Replace all occurrences of `shaperail.dev` with `shaperail.io` across all docs files. The `docs/_config.yml` already has `url: https://shaperail.io` — correct.

### 6. `docs/_config.yml` — Version bump

```yaml
release_version: 0.8.0  →  release_version: 0.9.0
```

### 7. `Cargo.toml` (workspace) — Version bump

```toml
version = "0.8.0"  →  version = "0.9.0"
```

### 8. `CHANGELOG.md` — Add 0.9.0 entry

Insert at the top (after the header, before the 0.8.0 entry):

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

---

## Files Changed

| File | Change |
|---|---|
| `Cargo.toml` | `version = "0.9.0"` |
| `docs/_config.yml` | `release_version: 0.9.0` |
| `docs/index.md` | Update feature tiers; remove completed in-progress items |
| `docs/llm-reference.md` | Add `rate_limit` row to endpoint keys table |
| `docs/resource-guide.md` | Add `rate_limit` row to endpoint attributes table |
| `docs/getting-started.md` | Navigation fix (remove parent, nav_order: 2); fix any `shaperail.dev` URLs |
| `docs/guides.md` | `nav_order: 3` |
| `docs/reference.md` | `nav_order: 4` |
| `docs/examples.md` | `nav_order: 5` |
| `docs/llm-guide.md` | Add `nav_exclude: true` |
| `CHANGELOG.md` | Add 0.9.0 entry |

No new files. No code changes.
