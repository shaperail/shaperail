---
title: Upgrading to v0.13
parent: Reference
nav_order: 90
---

# Upgrading from v0.12.x to v0.13.0

v0.13.0 introduces two breaking changes that every consumer must address before redeploying:

1. **`integer` field type is now 64-bit.** Maps to Postgres `BIGINT` and Rust `i64` (was `INTEGER` / `i32`). The separate `bigint` field type was removed.
2. **`AuthenticatedUser.id` renamed to `AuthenticatedUser.sub`** (and `Subject.id` → `Subject.sub`) to match RFC 7519 vocabulary.

Bumping `shaperail-*` versions in `Cargo.toml` is necessary but not sufficient. Generated Rust code and existing database columns from v0.12.x do **not** auto-update — without the steps below your app will hit runtime sqlx type-mismatch errors (`column "x" is of type integer but expression is of type bigint`) on the first request that touches a `type: integer` column.

## Quick checklist

```bash
# 1. Bump deps in Cargo.toml: shaperail-* = "0.13"
# 2. Regenerate Rust output
shaperail generate

# 3. Add an ALTER migration for every type: integer column (see template below)

# 4. Apply migrations
shaperail migrate

# 5. Rename `auth.id` / `subject.id` to `.sub` in custom handlers
grep -rn "user\.id\|subject\.id" src resources --include="*.rs"

# 6. Verify
cargo build --workspace
cargo test --workspace
shaperail check
```

## ALTER migration template

For every `type: integer` field declared in your resources, write one ALTER statement. Pick the next migration number (`shaperail migrate` uses `max(numeric_prefix) + 1`) and place the file as `migrations/00NN_alter_integer_to_bigint.sql`:

```sql
ALTER TABLE policies      ALTER COLUMN cap_minor TYPE BIGINT;
ALTER TABLE spend_intents ALTER COLUMN max_minor TYPE BIGINT;
-- ... one ALTER per (table, column) pair where type: integer is declared
```

Postgres rewrites the column on disk under an `ACCESS EXCLUSIVE` lock. On tables larger than ~10M rows, schedule the migration during an off-peak window or use a multi-phase `ADD COLUMN` / backfill / `DROP COLUMN` pattern.

## Detecting which columns need the ALTER

`shaperail check` in v0.13.1+ scans every `migrations/*.sql` and emits a warning (`SR100`) for any column whose latest committed migration declares `INTEGER` but whose schema field is now `type: integer`:

```text
W [SR100] policies.cap_minor: existing migration migrations/0011_create_policies.sql
       declares INTEGER, but type: integer now emits BIGINT in v0.13.0+.
       Add an ALTER TABLE policies ALTER COLUMN cap_minor TYPE BIGINT migration
       before deploying v0.13.0 generated code.
```

Warnings do not fail the check (exit code stays `0` if the YAML itself is valid). Use `--json` to consume them as machine-readable diagnostics in CI. Once every flagged column has a follow-up ALTER migration, the warnings disappear because the latest migration for that column declares `BIGINT`.

## What changed at the codegen level

For a field declared `cap_minor: { type: integer, ... }`:

| Surface | v0.12.x | v0.13.0 |
| --- | --- | --- |
| Rust struct field | `pub cap_minor: i32` | `pub cap_minor: i64` |
| sqlx column annotation | `"cap_minor!: i32"` | `"cap_minor!: i64"` |
| JSON parser | `parse_optional_json::<i32>` | `parse_optional_json::<i64>` |
| Postgres column | `INTEGER` | `BIGINT` |
| OpenAPI | `type: integer` | `type: integer, format: int64` |
| Protobuf | `int32` | `int64` |

The Rust changes ride on `cargo build` once you re-run codegen. The SQL change requires the ALTER migration above.

## Auth: `id` → `sub`

The JWT `sub` claim was being exposed as `AuthenticatedUser.id` and `Subject.id`. The name implied a `users.id` and invited custom handlers to bind it to FK columns, which silently fails for `super_admin` (whose `sub` is a routable platform identity, not a `users` row). Mechanical rename:

```diff
- let reviewer = Uuid::parse_str(&auth.id).ok();
+ let reviewer = Uuid::parse_str(&auth.sub).ok();
+ // 'sub' is opaque per RFC 7519. For super_admin it does NOT map to users.id.
+ // Narrow FK assignments to roles whose sub is guaranteed to be a users row.
```

See `docs/security.md` "JWT Claims" for the complete contract.

## Why the upgrade isn't automatic

shaperail-codegen is the source of truth for *fresh* schemas, but committed migrations are immutable history — rewriting them would break any environment that already ran them. v0.13.0 chose to break the type rather than ship the framework with a permanent footgun (`i32::MAX` cents is ~$21M USD, far below practical money limits, and the framework can't tell which integer columns are money-shaped). v0.13.1 ships this guide and the drift warning so the migration cost is visible at check-time and one-pass, not a runtime surprise.