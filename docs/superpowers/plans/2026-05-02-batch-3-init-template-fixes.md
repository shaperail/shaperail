# Batch 3 — `init` Template Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the `docker-compose up` Postgres healthcheck from spamming `FATAL` lines, and remove the dead `database:` (singular) configuration block so misconfigurations fail loudly instead of silently doing nothing.

**Architecture:** Three coordinated edits — (1) delete `database` field on `ProjectConfig` plus the now-orphaned `DatabaseConfig` type and its helpers, with `serde(deny_unknown_fields)` providing the loud-failure guarantee for upgrading projects; (2) drop the `database:` stanza from the `shaperail init` config template; (3) fix the docker-compose healthcheck to point at the actual database created by `POSTGRES_DB`. Single PR; no runtime semantics change beyond the noisy parse error for legacy configs.

**Tech Stack:** Rust 2021, serde / serde_yaml, sqlx, Actix-web (only the docker-compose target), `cargo` workflow per `CLAUDE.md`.

**Spec:** `docs/superpowers/specs/2026-05-02-batch-3-init-template-fixes-design.md`

**Branch:** `fix/init-template-cleanup` (already created, with the spec committed)

---

## File Structure

| File | Responsibility | Action |
|------|----------------|--------|
| `shaperail-core/src/config.rs` | `ProjectConfig` schema and `DatabaseConfig` legacy type | Modify — drop field + type + 2 helpers + 3 tests |
| `shaperail-codegen/src/config_parser.rs` | YAML → `ProjectConfig` parser plus tests | Modify — replace 3 test fixtures, add 1 rejection test |
| `shaperail-cli/src/commands/llm_context.rs` | Generates LLM-context summaries | Modify — drop `DatabaseConfig` import + fallback branch + test fixture field |
| `shaperail-cli/src/commands/init.rs` | `shaperail init` template emitter | Modify — drop `database:` stanza, fix healthcheck command |
| `CHANGELOG.md` | Project changelog | Modify — add `[Unreleased]` entries |

---

## Task 1: Lock the rejection behavior with a failing test

**Files:**
- Modify (test only): `shaperail-codegen/src/config_parser.rs`

- [ ] **Step 1.1: Add the failing test**

In `shaperail-codegen/src/config_parser.rs`, inside `mod tests` (after `parse_config_unknown_key_fails`, around current line 201), add:

```rust
    #[test]
    fn parse_config_legacy_database_field_rejected() {
        // The singular `database:` block was removed in v0.11. Existing configs
        // must fail loudly so users migrate to `databases.default:` or DATABASE_URL.
        let yaml = r#"
project: legacy-app
database:
  type: postgresql
  name: legacy_db
"#;
        let err = parse_config(yaml).expect_err(
            "legacy `database:` block should be rejected after v0.11"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field") && msg.contains("database"),
            "expected error to name the rejected `database` field, got: {msg}"
        );
    }
```

- [ ] **Step 1.2: Run the test to confirm it fails**

```
cargo test -p shaperail-codegen -- config_parser::tests::parse_config_legacy_database_field_rejected --nocapture
```

Expected: **FAIL** with `legacy `database:` block should be rejected after v0.11` — the field is currently still recognized, so `parse_config` returns `Ok` and `expect_err` panics.

- [ ] **Step 1.3: Commit the failing test**

```
git add shaperail-codegen/src/config_parser.rs
git commit -m "test(codegen): assert legacy database: field is rejected (failing)

Lock the v0.11 behavior: shaperail.config.yaml with a singular
\`database:\` block must fail to parse with an unknown-field error.
Currently fails — rejection is wired in the next commit.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Remove the `database` field, `DatabaseConfig` type, and orphan helpers

**Files:**
- Modify: `shaperail-core/src/config.rs`

- [ ] **Step 2.1: Update the `ProjectConfig` doc-comment example**

In `shaperail-core/src/config.rs`, replace the doc-comment block on lines 5–17 (everything from `/// Project-level configuration` through the closing `/// ```` of the singular example, but **keep** the multi-database example that follows):

Old:

```rust
/// Project-level configuration, parsed from `shaperail.config.yaml`.
///
/// ```yaml
/// project: my-api
/// port: 3000
/// workers: auto
/// database:
///   type: postgresql
///   host: localhost
///   port: 5432
///   name: my_api_db
///   pool_size: 20
/// ```
///
/// Multi-database (M14):
```

New:

```rust
/// Project-level configuration, parsed from `shaperail.config.yaml`.
///
/// ```yaml
/// project: my-api
/// port: 3000
/// workers: auto
/// databases:
///   default:
///     engine: postgres
///     url: ${DATABASE_URL}
/// ```
///
/// Multi-database (M14):
```

- [ ] **Step 2.2: Delete the `database` field from `ProjectConfig`**

In `shaperail-core/src/config.rs`, delete lines 46–48 (the doc comment plus the field):

```rust
    /// Single database configuration (legacy). Ignored if `databases` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<DatabaseConfig>,
```

Leave the `databases:` (plural) field untouched.

- [ ] **Step 2.3: Delete the `DatabaseConfig` struct**

In `shaperail-core/src/config.rs`, delete lines 187–209 (the `/// Database connection configuration (legacy single-DB).` doc-comment and the entire `pub struct DatabaseConfig { ... }` block).

- [ ] **Step 2.4: Delete the orphaned default helpers**

In `shaperail-core/src/config.rs`, delete `fn default_host()` (lines 226–228) and `fn default_db_port()` (lines 230–232). **Keep** `fn default_pool_size()` — it is still referenced by `NamedDatabaseConfig`.

- [ ] **Step 2.5: Update the in-file unit tests**

In `shaperail-core/src/config.rs`'s `mod tests`:

- In `project_config_minimal` (around line 434), delete the line `assert!(cfg.database.is_none());` (the field no longer exists).
- In `project_config_full` (around line 446), edit the JSON literal: remove the `"database": { ... }`, line. Then delete the assertions that touch `cfg.database` (around lines 482–486: `let db = cfg.database.unwrap(); ...` four lines of assertions). Leave all other assertions intact.
- Delete the entire `database_config_defaults` test (around lines 514–521).
- In `project_config_serde_roundtrip` (around line 532), in the `ProjectConfig { ... }` literal, delete the `database: Some(DatabaseConfig { ... }),` lines (one literal field plus the multi-line struct value, ~7 lines). The literal should now go straight from `workers: WorkerCount::Auto,` to `databases: None,`.

- [ ] **Step 2.6: Verify the core crate builds**

```
cargo build -p shaperail-core
```

Expected: **success**, no errors.

- [ ] **Step 2.7: Run shaperail-core tests**

```
cargo test -p shaperail-core
```

Expected: **all tests pass**. (The struct and its tests are gone; the remaining tests should be unaffected.)

---

## Task 3: Fix downstream call sites so the workspace compiles

After Task 2 the `shaperail-codegen` and `shaperail-cli` crates will fail to build because they reference the removed `database` field / `DatabaseConfig` type. Fix them.

**Files:**
- Modify: `shaperail-codegen/src/config_parser.rs`
- Modify: `shaperail-cli/src/commands/llm_context.rs`

- [ ] **Step 3.1: Update `parse_minimal_config` in `config_parser.rs`**

In `shaperail-codegen/src/config_parser.rs`, in `parse_minimal_config` (around line 86), delete the line:

```rust
        assert!(cfg.database.is_none());
```

- [ ] **Step 3.2: Rewrite `parse_full_config` in `config_parser.rs`**

In `shaperail-codegen/src/config_parser.rs`, replace the entire `parse_full_config` test (around lines 97–140) with:

```rust
    #[test]
    fn parse_full_config() {
        let yaml = r#"
project: my-api
port: 8080
workers: 4

databases:
  default:
    engine: postgres
    url: postgresql://localhost/my_api_db
    pool_size: 20

cache:
  type: redis
  url: redis://localhost:6379

auth:
  provider: jwt
  secret_env: JWT_SECRET
  expiry: 24h
  refresh_expiry: 30d

storage:
  provider: s3
  bucket: my-bucket
  region: us-east-1

logging:
  level: info
  format: json
  otlp_endpoint: http://localhost:4317
"#;
        let cfg = parse_config(yaml).unwrap();
        assert_eq!(cfg.project, "my-api");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.workers, WorkerCount::Fixed(4));
        let dbs = cfg.databases.as_ref().unwrap();
        assert_eq!(
            dbs.get("default").unwrap().url,
            "postgresql://localhost/my_api_db"
        );
        let auth = cfg.auth.unwrap();
        assert_eq!(auth.provider, "jwt");
    }
```

- [ ] **Step 3.3: Rewrite `parse_config_interpolates_env_vars`**

In `shaperail-codegen/src/config_parser.rs`, replace the entire `parse_config_interpolates_env_vars` test (around lines 149–165) with:

```rust
    #[test]
    fn parse_config_interpolates_env_vars() {
        let yaml = r#"
project: ${SHAPERAIL_TEST_PROJECT}
databases:
  default:
    engine: postgres
    url: postgresql://localhost/${SHAPERAIL_TEST_DB:test_db}
"#;
        std::env::set_var("SHAPERAIL_TEST_PROJECT", "shaperail-ai");
        std::env::remove_var("SHAPERAIL_TEST_DB");

        let cfg = parse_config(yaml).unwrap();
        assert_eq!(cfg.project, "shaperail-ai");
        let dbs = cfg.databases.as_ref().unwrap();
        assert_eq!(
            dbs.get("default").unwrap().url,
            "postgresql://localhost/test_db"
        );

        std::env::remove_var("SHAPERAIL_TEST_PROJECT");
    }
```

- [ ] **Step 3.4: Update the import in `llm_context.rs` test module**

In `shaperail-cli/src/commands/llm_context.rs` at line 340, replace:

```rust
    use shaperail_core::{DatabaseConfig, ProjectConfig, WorkerCount};
```

with:

```rust
    use shaperail_core::{ProjectConfig, WorkerCount};
```

- [ ] **Step 3.5: Update `make_config` test fixture**

In `shaperail-cli/src/commands/llm_context.rs`, in the `make_config` function (around lines 342–364), delete the seven lines:

```rust
            database: Some(DatabaseConfig {
                db_type: "postgresql".to_string(),
                host: "localhost".to_string(),
                port: 5432,
                name: "test_db".to_string(),
                pool_size: 5,
            }),
```

The `ProjectConfig { ... }` literal should go directly from `workers: WorkerCount::Auto,` to `databases: None,`.

- [ ] **Step 3.6: Drop the legacy `else if` branch in the database-summary helper**

In `shaperail-cli/src/commands/llm_context.rs`, locate the function containing line 64 (the `else if let Some(ref db) = config.database { db.db_type.clone() }` branch). Delete that branch entirely so the chain becomes `if let Some(ref dbs) = config.databases { ... } else { "unknown".into() }`.

Concretely, the existing code reads (excerpt around lines 50–69):

```rust
    if let Some(ref dbs) = config.databases {
        let mut engines: Vec<String> = dbs
            .values()
            .map(|d| {
                serde_json::to_value(d.engine)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| format!("{:?}", d.engine).to_lowercase())
            })
            .collect();
        engines.sort();
        engines.dedup();
        engines.join(", ")
    } else if let Some(ref db) = config.database {
        db.db_type.clone()
    } else {
        "unknown".into()
    }
```

Replace with:

```rust
    if let Some(ref dbs) = config.databases {
        let mut engines: Vec<String> = dbs
            .values()
            .map(|d| {
                serde_json::to_value(d.engine)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| format!("{:?}", d.engine).to_lowercase())
            })
            .collect();
        engines.sort();
        engines.dedup();
        engines.join(", ")
    } else {
        "unknown".into()
    }
```

- [ ] **Step 3.7: Verify the workspace builds**

```
cargo build --workspace
```

Expected: **success**, no errors. If errors point to additional callsites (e.g., other tests we missed), grep `config\.database\b` across the workspace and fix the same way.

```
grep -rn "config\.database\b\|cfg\.database\b\|DatabaseConfig" shaperail-cli/src shaperail-runtime/src shaperail-codegen/src shaperail-core/src 2>/dev/null
```

Expected: no hits other than `NamedDatabaseConfig` (which is a different type).

- [ ] **Step 3.8: Run the rejection test introduced in Task 1**

```
cargo test -p shaperail-codegen -- config_parser::tests::parse_config_legacy_database_field_rejected --nocapture
```

Expected: **PASS** — `parse_config` now rejects the `database:` block with an `unknown field` error.

- [ ] **Step 3.9: Run the full workspace test suite**

```
cargo test --workspace
```

Expected: **all tests pass**. If a test fails because it expected the legacy field, that is a test we missed — update it the same way.

- [ ] **Step 3.10: Commit**

```
git add shaperail-core/src/config.rs shaperail-codegen/src/config_parser.rs shaperail-cli/src/commands/llm_context.rs
git commit -m "feat(core)!: drop legacy database: config field

Removes ProjectConfig::database, the DatabaseConfig type, and two
default helpers that are no longer referenced. The runtime never
read the singular database: block — it only ever consumed databases
(plural) or DATABASE_URL. With deny_unknown_fields on ProjectConfig,
existing configs containing database: now fail to parse with a
clear unknown-field error.

Migration: replace the database: block with databases.default: or
set DATABASE_URL in .env.

Closes #8.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Drop `database:` from the generated config and fix the healthcheck

**Files:**
- Modify: `shaperail-cli/src/commands/init.rs`

- [ ] **Step 4.1: Drop the `database:` stanza from the generated `shaperail.config.yaml`**

In `shaperail-cli/src/commands/init.rs`, locate the `let config = format!(...)` block at line ~495. Delete the seven lines making up the `database:` block (lines 500–506 in the source, including the trailing blank line):

```rust
database:
  type: postgresql
  host: localhost
  port: 5432
  name: {db_name}
  pool_size: 20

```

The format-string should now go from:

```rust
project: {project_name}
port: 3000
workers: auto

cache:
```

…directly into the `cache:` section. Also, since `db_name` is no longer interpolated into the string, drop the `db_name = project_name.replace('-', "_")` argument from the `format!(...)` call (around line 521). (The `db_name` value is still used by the docker-compose template later — leave that occurrence alone.)

- [ ] **Step 4.2: Fix the Postgres healthcheck**

In `shaperail-cli/src/commands/init.rs`, locate the docker-compose template at line ~1888. Replace line ~1901:

Old:

```yaml
      test: ["CMD-SHELL", "pg_isready -U shaperail"]
```

New:

```yaml
      test: ["CMD-SHELL", "pg_isready -U $${POSTGRES_USER} -d $${POSTGRES_DB}"]
```

The doubled `$$` is the docker-compose escape for a literal `$`; the variables resolve at container runtime from the `environment:` block already declared above on the same service.

- [ ] **Step 4.3: Verify the CLI builds**

```
cargo build -p shaperail-cli
```

Expected: **success**. If the compiler complains about an unused `db_name` binding inside the `format!` call, remove the now-unused argument from that specific call. Do not touch the docker-compose `format!` (which still uses `db_name`).

- [ ] **Step 4.4: Smoke-test the generated config**

Generate a fresh project in a tempdir and assert the new shape:

```
cd /tmp && rm -rf shaperail-batch3-smoke && cargo run -q -p shaperail-cli -- init shaperail-batch3-smoke
grep -c "^database:" shaperail-batch3-smoke/shaperail.config.yaml
grep "pg_isready" shaperail-batch3-smoke/docker-compose.yml
```

Expected:
- First `grep` prints `0` (no singular `database:` line).
- Second `grep` shows: `      test: ["CMD-SHELL", "pg_isready -U $${POSTGRES_USER} -d $${POSTGRES_DB}"]`

- [ ] **Step 4.5: Run all CLI tests**

```
cargo test -p shaperail-cli
```

Expected: **all tests pass**. If a snapshot test for `init` output exists and fails, regenerate it with `cargo insta accept` (or manual snapshot update — depends on the snapshot library in use; check `shaperail-cli/Cargo.toml` for `insta`).

- [ ] **Step 4.6: Commit**

```
git add shaperail-cli/src/commands/init.rs
git commit -m "fix(cli): clean up shaperail init template

- Drop the dead database: block from the generated
  shaperail.config.yaml. Projects rely on DATABASE_URL from .env.
- Fix the Postgres healthcheck to read POSTGRES_USER and POSTGRES_DB
  from the compose service environment, eliminating the
  'FATAL: database \"shaperail\" does not exist' log spam.

Closes #7.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Update `CHANGELOG.md`

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 5.1: Add `[Unreleased]` entries**

Open `CHANGELOG.md`. If an `## [Unreleased]` section already exists, append the entries below to its `### Breaking` and `### Fixed` subsections (creating those subsections if missing). If no `[Unreleased]` section exists, add one above the most recent versioned heading:

```markdown
## [Unreleased]

### Breaking
- `database:` (singular) block removed from `shaperail.config.yaml`. The block was previously parsed but never read at runtime. Replace it with `databases.default:` or set `DATABASE_URL` in `.env`. Configs that retain the legacy block now fail to parse with `unknown field 'database'`.

### Fixed
- `docker-compose.yml` Postgres healthcheck no longer logs `FATAL: database "shaperail" does not exist` every 5 s. The healthcheck now uses `POSTGRES_USER`/`POSTGRES_DB` from the compose environment (#7).
```

- [ ] **Step 5.2: Commit**

```
git add CHANGELOG.md
git commit -m "docs(changelog): note v0.11 init-template fixes

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Final quality gate

**Files:** none — verification only.

- [ ] **Step 6.1: Format**

```
cargo fmt --all
```

Expected: no output (already formatted) or a few small reformats. Stage and amend into the most recent commit if the diff is purely whitespace, otherwise commit separately:

```
git add -u
git commit -m "style: cargo fmt"
```

(Skip this commit if `cargo fmt` produces no diff.)

- [ ] **Step 6.2: Clippy**

```
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: **no warnings, no errors**. If warnings appear:
- `unused import` for `DatabaseConfig` — go back to the file and remove.
- `dead_code` for a helper — delete it.
- Anything else — fix at the root cause; do not silence with `#[allow]`.

- [ ] **Step 6.3: Full test suite**

```
cargo test --workspace
```

Expected: **all tests pass**.

- [ ] **Step 6.4: Final docker-compose smoke test (optional, if Docker is available)**

If Docker is running locally:

```
cd /tmp && rm -rf shaperail-batch3-final && cargo run -q -p shaperail-cli -- init shaperail-batch3-final
cd shaperail-batch3-final && docker compose up -d
sleep 15
docker compose logs postgres 2>&1 | grep -i fatal | head
docker compose down -v
```

Expected: the `grep` produces **no output**. (Previously it would show `FATAL: database "shaperail" does not exist` lines.)

- [ ] **Step 6.5: Push the branch and open PR (await user approval before pushing)**

Confirm with the user before pushing. Once approved:

```
git push -u origin fix/init-template-cleanup
gh pr create --title "fix(cli): clean up init template — drop dead database: block, fix postgres healthcheck" --body "$(cat <<'EOF'
## Summary

- Removes the dead singular `database:` config block (#8). The runtime never read it; existing configs now fail loudly via `deny_unknown_fields`.
- Fixes the docker-compose Postgres healthcheck to use `POSTGRES_USER`/`POSTGRES_DB` from compose env (#7). Stops the `FATAL` log spam.
- Drops `DatabaseConfig`, `default_host`, and `default_db_port` (orphaned after the field removal).

## Breaking change

Existing `shaperail.config.yaml` files containing a `database:` block will fail to parse with `unknown field 'database'`. Migrate to `databases.default:` or rely on `DATABASE_URL` from `.env`. CHANGELOG updated accordingly.

## Test plan

- [x] `cargo build --workspace`
- [x] `cargo test --workspace`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] New test: `parse_config_legacy_database_field_rejected` proves loud failure on legacy configs
- [x] Manual: `shaperail init demo && docker compose up -d && docker compose logs postgres | grep -i fatal` → no output

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Acceptance Checklist

These map to the spec's acceptance criteria. Tick each before considering the plan complete.

- [ ] `shaperail init demo && cd demo && docker compose up -d` followed by `docker compose logs postgres` shows no `FATAL` lines under steady state. (Step 6.4)
- [ ] A `shaperail.config.yaml` containing a `database:` block fails `shaperail check` (or `cargo test -p shaperail-codegen`) with an `unknown field 'database'` error. (Step 3.8 / Task 1)
- [ ] Generated `shaperail.config.yaml` no longer contains a `database:` stanza. (Step 4.4)
- [ ] `cargo build --workspace`, `cargo test --workspace`, and `cargo clippy --workspace -- -D warnings` all pass. (Step 6.2 / 6.3)
- [ ] `CHANGELOG.md` has Breaking and Fixed entries under `[Unreleased]`. (Task 5)
