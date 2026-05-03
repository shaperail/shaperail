# Upgrading from v0.12.x to v0.13.0

v0.13.0 contains two breaking changes that affect every consumer:

1. **`integer` field type is now 64-bit.** Maps to Postgres `BIGINT` and Rust `i64` (was `INTEGER` / `i32`). The separate `bigint` field type was removed.
2. **`AuthenticatedUser.id` renamed to `AuthenticatedUser.sub`** (and `Subject.id` → `Subject.sub`) to match RFC 7519 vocabulary.

Bumping `shaperail-*` deps in `Cargo.toml` is necessary but not sufficient. Generated Rust code and existing database columns from v0.12.x do not auto-update — without the steps below you will see runtime sqlx type-mismatch errors (`column "x" is of type integer but expression is of type bigint`) on the first request that touches a `type: integer` column.

## Upgrade checklist

```bash
# 1. Bump deps
sed -i '' 's/shaperail-\([a-z]*\) = "0.12.*"/shaperail-\1 = "0.13"/g' Cargo.toml

# 2. Regenerate Rust output (re-runs shaperail-codegen against your resources/)
shaperail generate

# 3. For every type: integer column already on disk, write an ALTER migration.
#    The next available numeric prefix comes from `shaperail migrate` (which uses
#    max(numeric_prefix) + 1) — pick that number and create:
#
#    migrations/00NN_alter_integer_to_bigint.sql

# 4. Apply migrations
shaperail migrate

# 5. Update custom handlers: replace `auth.id` / `subject.id` with `.sub`.
#    Document this rename: `sub` is opaque per RFC 7519 — do NOT bind it to
#    foreign-key columns without verifying. See agent_docs/auth-claims.md.
grep -rn "user\.id\|subject\.id" src resources --include="*.rs"

# 6. Verify
cargo build --workspace
cargo test --workspace
shaperail check
```

## ALTER migration template

For each `type: integer` field in your resources, emit one statement:

```sql
-- migrations/00NN_alter_integer_to_bigint.sql
ALTER TABLE policies     ALTER COLUMN cap_minor TYPE BIGINT;
ALTER TABLE spend_intents ALTER COLUMN max_minor TYPE BIGINT;
-- ... one ALTER per (table, column) pair where type: integer is declared
```

Postgres `ALTER COLUMN ... TYPE BIGINT` rewrites the column on disk (`AccessExclusiveLock` on the table for the duration of the rewrite). For tables larger than ~10M rows, schedule this against an off-peak window or use a multi-phase add-column / backfill / drop-column pattern.

`shaperail check` (v0.13.1+) emits `SR100` warnings naming every column that needs this ALTER — see "Detecting drift before deploy" below.

## What changed at the codegen level

For a field declared `cap_minor: { type: integer, ... }`, v0.12.x produced:

```rust
pub cap_minor: i32,                                    // struct field
"cap_minor" as "cap_minor!: i32",                      // sqlx column annotation
parse_optional_json::<i32>(data, "cap_minor")?,        // JSON parser
```

```sql
"cap_minor" INTEGER NOT NULL,                          // migration
```

v0.13.0 produces:

```rust
pub cap_minor: i64,
"cap_minor" as "cap_minor!: i64",
parse_optional_json::<i64>(data, "cap_minor")?,
```

```sql
"cap_minor" BIGINT NOT NULL,
```

The Rust changes ride on `cargo build` once you re-run codegen. The SQL change requires the ALTER migration above — old migration files are historical artifacts and never auto-rewrite.

## Detecting drift before deploy

`shaperail check` in v0.13.1+ scans every `migrations/*.sql` for `"<col>" INTEGER` patterns whose corresponding resource field is now `type: integer`, and warns with code `SR100`:

```
W [SR100] policies.cap_minor: existing migration migrations/0011_create_policies.sql
       declares INTEGER, but type: integer now emits BIGINT in v0.13.0+.
       Add an ALTER TABLE policies ALTER COLUMN cap_minor TYPE BIGINT migration
       before deploying v0.13.0 generated code.
```

Warnings do not fail `shaperail check` (exit code stays `0` if the YAML is valid). Use `--json` to get them as machine-readable diagnostics. Once every flagged column has a follow-up `ALTER` migration, the warnings stop because the latest migration declares `BIGINT`.

## Why this rough edge exists

shaperail-codegen is the source of truth for fresh schemas, but committed migrations are immutable history — rewriting them would break any environment that already ran them. v0.13.0 chose to break the type rather than ship the framework with a permanent footgun (`i32::MAX` cents = ~$21M USD, far below practical money limits). v0.13.1 ships this guide and the drift warning so the migration cost is visible and one-pass, not a runtime surprise.