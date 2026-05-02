# Fix array `items:` constraint maps and migration numbering

**Date:** 2026-05-03
**Status:** Approved (brainstorming)
**Issues addressed:** Issue I (array items as constraint map rejected) and Issue J (migrate numbers collide with hand-written migrations)

## Problem

Two reported issues, both blocking clean declaration of vendor-style resources, ship together because they share the same review surface (CLI/codegen) and have the same audience.

### Issue I — `items:` as constraint map rejected

```yaml
schema:
  currencies: { type: array, items: { type: string, min: 3, max: 3 } }
```

`shaperail check --json` reports `SR000` with the unhelpful fix `"fix the YAML syntax error shown above"`. Root cause: `FieldSchema.items` is `Option<String>` in `shaperail-core/src/schema.rs:71`, and `FieldSchema` carries `#[serde(deny_unknown_fields)]`, so the parser refuses any map shape on `items:`.

The workaround (`type: json` + a controller validator) loses declarative validation, requires per-resource code, and forces JSONB storage instead of native Postgres array columns — losing `= ANY(...)` operators downstream.

### Issue J — migration numbering collides with hand-written migrations

`shaperail-cli/src/commands/migrate.rs:33` numbers new migrations with `existing.len() + 1` (count-based). When a hand-written invariants migration sits past the highest auto-generated `_create_*` file (e.g. `0010_m02_ledger_invariants.sql` after `0009_create_journal_lines.sql`), the next emitted file collides at `0010_create_<resource>.sql`. sqlx's checksum guard then refuses to apply the new migration.

Manual workaround is to `mv` the colliding file. The fix is to use `max(numeric_prefix) + 1` instead of `count + 1`.

## Goals

- `items: { type: <T>, <constraint>: <value>, ... }` parses cleanly for `T ∈ {string, integer, bigint, number, boolean, timestamp, date, enum, uuid}`.
- Element-level constraints (`min`, `max`, `format`, `values`, `ref`) are validated at runtime per element in `check_field_rules`.
- `items.ref` performs a runtime existence check on Postgres (`SELECT … WHERE id = ANY($1::uuid[])`). Backend support is decided at runtime — Postgres uses `= ANY`; mysql and sqlite return a clear runtime error directing the user to switch backend or remove the ref.
- The legacy `items: <typename>` shorthand keeps working unchanged.
- `shaperail migrate` always emits `max(numeric_prefix) + 1` regardless of gaps or hand-written migrations.

## Non-goals

- **Nested arrays.** `items: { type: array, items: ... }` is rejected by the validator. Users wanting nested structure should use `type: json`.
- **Postgres CHECK constraints for element enums.** Runtime validation is sufficient and matches how scalar enums work today.
- **`items.ref` on non-Postgres backends.** Runtime returns `Err(ShaperailError::Internal("items.ref requires Postgres"))` if the bound pool is mysql or sqlite. No JSON-storage emulation. (Validator can't catch this — `db:` names a connection, not a backend, and config is loaded at runtime — so the runtime is the authoritative gate.)
- **Deprecating the bare-string `items:` shorthand.** Shipped form stays. A future breaking release can revisit it.
- **Typifying `proto.rs` element types per element.** `Array → google.protobuf.ListValue` is preserved.

## Design

### Schema model

Add a new `ItemsSpec` in `shaperail-core/src/schema.rs`. Distinct from `FieldSchema` because element-level fields like `primary`, `generated`, `unique`, `transient`, `nullable`, `required`, `search`, `sensitive`, and nested `items` are nonsensical at the element level — defining a focused type means invalid combinations fail at parse time:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemsSpec {
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "ref")]
    pub reference: Option<String>,
}
```

`FieldSchema.items` becomes `Option<ItemsSpec>`. `ItemsSpec` is `pub` so codegen can pattern-match.

### Dual-form parsing

Both forms accepted:

```yaml
# Legacy (kept)
items: string
# New
items: { type: string, min: 3, max: 3 }
```

A custom `Deserialize` impl on `ItemsSpec` (visitor that handles both `Value::String` and `Value::Map`) maps a bare scalar to `ItemsSpec { field_type: <parsed>, ..Default::default() }` and a map directly to the struct.

### Validator rules

In `shaperail-codegen/src/validator.rs`, when `field.field_type == FieldType::Array && field.items.is_some()`:

| Rule | Fix message |
|---|---|
| `items.field_type != FieldType::Array` (no nesting) | "use type: json for nested arrays" |
| `items.field_type == FieldType::Enum` requires `items.values` | "add `values: [...]` to items" |
| `items.format.is_some()` requires `items.field_type == String` | "format only valid on string items" |
| `items.reference.is_some()` requires `items.field_type == Uuid` | "items.ref requires type: uuid" |
| `items.reference` must be `resource.field` shape | (mirrors top-level rule) |

The non-Postgres backend check moves to runtime (see "items.ref existence check" below) — the validator can't see backend bindings.

Existing `field.field_type == FieldType::Array && field.items.is_none()` check is preserved.

Diagnostic codes follow the existing scheme in `shaperail-codegen/src/diagnostics.rs`. Specific codes will be assigned by the implementation plan from the next free range to avoid collision with concurrent work; each rule above maps to one new code.

### Runtime validation

In `shaperail-runtime/src/handlers/validate.rs::check_field_rules`, when the field is an array and `items` is set and the value is `Value::Array`, iterate and apply a new `check_item_rules(name, idx, items, element, &mut errors)` that mirrors scalar rule handling:

- string min/max → `too_short` / `too_long`
- integer/bigint/number min/max → `too_small` / `too_large`
- enum values → `invalid_enum`
- uuid parse → `invalid_uuid`
- format email/url → `invalid_format`

Each error sets `field` to `<field_name>[<idx>]` so client UX surfaces the offending index.

If the value is non-null but not an array, emit `invalid_type`. (Tightening this only when `items.is_some()` keeps the change forward-only.)

### `items.ref` existence check

New pass `validate_item_references` in `shaperail-runtime/src/handlers/validate.rs`, called from `crud.rs` between `validate_required_present` and the `INSERT` / `UPDATE`. For each array field with `items.reference`:

1. Skip if absent or empty.
2. Confirm the bound pool is Postgres. If not, return `Err(ShaperailError::Internal("items.ref requires a Postgres-backed resource"))`.
3. Parse the FK target (`organizations.id` → table `organizations`, column `id`).
4. Issue `SELECT COUNT(DISTINCT "<column>") FROM "<table>" WHERE "<column>" = ANY($1::uuid[])`.
5. If count != distinct element count, return `ShaperailError::Validation` with code `invalid_reference` and a message listing up to the first 5 missing IDs.

One query per FK-array column per write. Runs only when the field is present and non-empty. Uses the same DB pool as the request.

### Codegen impacts

| File | Change |
|---|---|
| `shaperail-codegen/src/rust.rs` (`sql_cast_type`, `query_type`) | Switch `field.items.as_deref()` to `field.items.as_ref().map(|i| &i.field_type)`. Extend element-type table to include `bigint`, `number`, `boolean`, `timestamp`, `date`. |
| `shaperail-runtime/src/db/query.rs` (`field_type_to_sql_postgres`) | Same one-line shape change. Storage type for enum-array stays `TEXT[]`. |
| `shaperail-codegen/src/openapi.rs` | Emit real `items` schema reflecting element type, min/max, enum values, format. Closes the `items: {}` gap. |
| `shaperail-codegen/src/typescript.rs` | No shape change beyond reading the new field path. |
| `shaperail-codegen/src/json_schema.rs` | Emit element-level constraints so IDE/LLM validation matches runtime. |
| `shaperail-codegen/src/proto.rs` | Unchanged. `Array → google.protobuf.ListValue` preserved. |

### Migration numbering fix

In `shaperail-cli/src/commands/migrate.rs`, replace the count-based snippet inside the resource loop:

```rust
// Compute once before the loop:
let mut next_version = list_migration_files(migrations_dir)
    .iter()
    .filter_map(|name| {
        name.split_once('_')
            .and_then(|(prefix, _)| prefix.parse::<u32>().ok())
    })
    .max()
    .map(|m| m + 1)
    .unwrap_or(1);

// Inside the resource loop, after the migration_exists() early-skip:
let filename = format!("{next_version:04}_{migration_name}.sql");
// ... write file ...
next_version += 1;
```

The `migration_exists` early-skip is preserved so re-running `shaperail migrate` on already-migrated resources stays a no-op. Multiple new resources in one invocation get distinct numbers because we increment locally.

## Testing

| Scope | Cases |
|---|---|
| `shaperail-core` schema unit | bare-string form parses; full-map form parses; nested-array rejected; unknown field on `ItemsSpec` rejected. |
| `shaperail-codegen` validator unit | `items.ref` non-uuid rejected; nested-array rejected; enum-items missing `values` rejected; `items.format` on non-string rejected; ref shape mismatch rejected. Each asserts the diagnostic code. |
| `shaperail-runtime` items.ref backend gate | sqlite-bound resource with `items.ref` returns `Internal("…requires a Postgres-backed resource")` on write. |
| `shaperail-codegen` rust unit | Generated model contains `Vec<i64>`, `Vec<f64>`, `Vec<chrono::NaiveDate>` for the new element types. |
| `shaperail-runtime` validate unit | string-element too-short; enum-element not allowed; uuid-element invalid; mixed-success error aggregation (multiple element errors in one field); empty array passes; `null` passes; non-array value emits `invalid_type`. |
| `shaperail-cli` migrate unit | Gap pattern (`0008,0009,0010_handwritten`) yields `0011_create_*`; empty dir yields `0001`; multi-resource invocation gets `next, next+1, next+2`. |
| Postgres integration (`db_integration.rs`) | One resource declaring `tags: { type: array, items: { type: uuid, ref: organizations.id } }`: happy path inserts; missing-id rejects with `invalid_reference` and the missing UUID surfaced; empty array accepted. |

## Docs & changelog

Per the public-mirror rule:

- `agent_docs/resource-format.md` + `docs/resource-guide.md` — `items:` accepts either a type name or a constraint map; supported element constraints; `items.ref` semantics and Postgres-only caveat; migration numbering described as "next free integer above max prefix".
- `agent_docs/codegen-patterns.md` + `docs/openapi.md` — element-level OpenAPI emission.
- `CHANGELOG.md` `[Unreleased]` — two entries: "Element-level constraints on `items:`" (`feat(shaperail-codegen)`) and "Migration numbering uses max prefix" (`fix(shaperail-cli)`).
- `CLAUDE.md` — no change; conventions unchanged.

## Conventional-commit plan

Two PRs (or one bundled PR with both commits):

- `feat(shaperail-codegen): element-level constraints on array items` — minor bump.
- `fix(shaperail-cli): use max prefix for migration numbering` — patch bump.

Both crate-scoped, both user-facing.
