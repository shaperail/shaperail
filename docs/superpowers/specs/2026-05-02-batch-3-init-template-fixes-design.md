# Batch 3 — `init` Template Fixes

**Date:** 2026-05-02
**Issues:** #7 (Postgres healthcheck spam), #8 (dead `database:` config block)
**Risk:** Low. One small breaking config change, gated by `deny_unknown_fields`.

## Goal

Make `shaperail init <name> && cd <name> && docker compose up -d` produce a project that:

1. Boots cleanly with no `FATAL` lines in the Postgres log.
2. Has no dead configuration that silently does nothing.

## Non-Goals

- Refactoring multi-database (`databases:`) parsing.
- Changing the `.env` template or `DATABASE_URL` resolution.
- Migration tooling for users upgrading from older Shaperail versions (a CHANGELOG note is enough).

## Change 1 — Healthcheck (#7)

**File:** `shaperail-cli/src/commands/init.rs`, the `docker-compose.yml` template (~line 1901).

**Before:**

```yaml
healthcheck:
  test: ["CMD-SHELL", "pg_isready -U shaperail"]
  interval: 5s
  timeout: 3s
  retries: 10
```

**After:**

```yaml
healthcheck:
  test: ["CMD-SHELL", "pg_isready -U $${POSTGRES_USER} -d $${POSTGRES_DB}"]
  interval: 5s
  timeout: 3s
  retries: 10
```

**Why:** Without `-d`, `pg_isready` defaults to `dbname = $PGUSER` (= `shaperail`), which does not exist (the actual DB is `POSTGRES_DB` = the project name). Each probe logs `FATAL: database "shaperail" does not exist`. The `$$` is the docker-compose literal-`$` escape so the value resolves at container runtime from the `environment:` block already declared on the service.

**Why this approach over hard-coding `-d postgres`:** the variable form keeps a single source of truth (the `POSTGRES_DB` env var) and survives any future template change to the project-name → db-name mapping.

## Change 2 — Drop dead `database:` field (#8)

The singular `database:` block has been parsed by `ProjectConfig` since before multi-DB support landed, but the runtime DB manager (`shaperail-runtime/src/db/manager.rs:55-90`) only reads `databases` (plural) or falls back to the `DATABASE_URL` env var. The field is functionally dead. Per the framework's "ONE WAY" and "loud failure on invalid input" rules, we remove it.

### 2a — Remove the field from `ProjectConfig`

**File:** `shaperail-core/src/config.rs`

- Delete the field on line 48: `pub database: Option<DatabaseConfig>`.
- Update the doc-comment example on lines 7–17 to show `databases.default:` instead of `database:`.
- If `DatabaseConfig` is no longer referenced anywhere after this removal, delete the type as well. (Verify with `cargo check --workspace` before deleting.)

Because `ProjectConfig` carries `#[serde(deny_unknown_fields)]`, any existing project config that still has a `database:` block now fails to parse with `unknown field 'database'`. That is the intended behavior — the previous silent ignore is the bug.

### 2b — Remove the block from the generated config

**File:** `shaperail-cli/src/commands/init.rs`, the `shaperail.config.yaml` template (lines 500–505 inside the format-string at line 495).

- Delete the entire `database:` stanza (`type: postgresql`, `host`, `port`, `name`, `pool_size`).
- Leave the rest of the file (project, port, workers, cache, auth, logging) unchanged.
- The `.env` file already provides `DATABASE_URL`, which is what the runtime reads.

### 2c — Scrub remaining call sites

Run `grep -rn "config\.database\b\|ProjectConfig.*database\b\|DatabaseConfig" shaperail-*/src` and update any consumer that still references the removed field.

Known site to verify: `shaperail-cli/src/commands/llm_context.rs:347` constructs a `DatabaseConfig`. Either drop the construction or migrate it to `NamedDatabaseConfig` inside a `databases` map, depending on what that command emits.

## Tests

- `shaperail-codegen/src/config_parser.rs:150` and `:168` — replace `database:` in test fixtures with `databases.default:`.
- Add `parse_config_legacy_database_field_rejected` next to the existing `parse_config_unknown_key_fails` test: feed in a YAML with a `database:` block, assert the parse error mentions the field name.
- If there is a snapshot/golden test for `shaperail init` output, regenerate it.
- Manual smoke test: `cargo run -p shaperail-cli -- init demo && cd demo && docker compose up -d && docker compose logs postgres | grep -i fatal` → expect no output.

## Documentation

**`CHANGELOG.md`** under `[Unreleased]`:

- **Breaking:** `database:` (singular) block removed from `shaperail.config.yaml`. Use `databases.default:` or set `DATABASE_URL`. The block was previously parsed but never read at runtime.
- **Fixed:** `docker-compose.yml` Postgres healthcheck no longer logs `FATAL: database "shaperail" does not exist` every 5 s (#7).

**`agent_docs/resource-format.md`** and any other doc that shows the singular form: update to the plural form or to `DATABASE_URL` in `.env`.

## Acceptance Criteria

1. `shaperail init demo && cd demo && docker compose up -d` followed by `docker compose logs postgres` shows no `FATAL` lines under steady state.
2. A `shaperail.config.yaml` containing a `database:` block fails `shaperail check` with an `unknown field 'database'` error pointing at the line.
3. Generated `shaperail.config.yaml` no longer contains a `database:` stanza.
4. `cargo build --workspace`, `cargo test --workspace`, and `cargo clippy --workspace -- -D warnings` all pass.

## Rollout

Single PR. Tag a v0.11.0 release after merge (breaking config change). Mention the migration in release notes: replace `database:` with `databases.default:` in `shaperail.config.yaml`, or rely on `DATABASE_URL`.
