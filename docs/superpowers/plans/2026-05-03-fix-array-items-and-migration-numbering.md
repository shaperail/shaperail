# Fix array `items:` constraint maps and migration numbering — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land both reported issues — element-level constraints on array `items:` (Issue I) and `max(prefix)+1` migration numbering (Issue J) — on a single branch with two conventional commits.

**Architecture:** Issue J is a 5-line change in `shaperail-cli/src/commands/migrate.rs`, ships first with its own commit. Issue I introduces a new `ItemsSpec` type in `shaperail-core` with a custom `Deserialize` visitor accepting both the bare-string shorthand and a constraint map; everything downstream (validator, runtime check_field_rules, runtime ref existence check, OpenAPI / JSON Schema / Rust codegen) reads through this single type.

**Tech Stack:** Rust 2021, serde / serde_yaml, sqlx (PgPool), Actix-web 4. No new crate dependencies.

**Spec:** `docs/superpowers/specs/2026-05-03-fix-array-items-and-migration-numbering-design.md`

**Branch:** create a fresh branch off main — `git checkout -b fix/items-spec-and-migration-numbering`. Two commits land on this branch:
- Commit A: `fix(shaperail-cli): use max prefix for migration numbering` — patch bump.
- Commit B: `feat(shaperail-codegen): element-level constraints on array items` — minor bump.

---

## File Structure

| File | Role | Change |
|---|---|---|
| `shaperail-cli/src/commands/migrate.rs` | CLI migrate command | Replace count-based numbering with max-prefix scan; extract pure helper for testability |
| `shaperail-core/src/schema.rs` | `FieldSchema`, new `ItemsSpec` | Add `ItemsSpec` with custom Deserialize; change `items` field type |
| `shaperail-core/src/lib.rs` | Public re-exports | Re-export `ItemsSpec` |
| `shaperail-codegen/src/validator.rs` | Resource validation | Add 5 element-level validator rules |
| `shaperail-codegen/src/diagnostics.rs` | Diagnostic codes | Add 5 new diagnostic codes (next free range) |
| `shaperail-codegen/src/rust.rs` | Rust codegen | Read element type via `ItemsSpec.field_type`; extend element-type table |
| `shaperail-codegen/src/openapi.rs` | OpenAPI codegen | Emit element type/min/max/enum/format on array items |
| `shaperail-codegen/src/json_schema.rs` | JSON Schema codegen | Mirror OpenAPI element constraints |
| `shaperail-runtime/src/db/query.rs` | SQL type mapping | `field_type_to_sql_postgres` reads through `ItemsSpec` |
| `shaperail-runtime/src/handlers/validate.rs` | Input validation | Add `check_item_rules` + `validate_item_references` |
| `shaperail-runtime/src/handlers/crud.rs` | CRUD handlers | Wire `validate_item_references` into create / update / bulk_create |
| `agent_docs/resource-format.md`, `docs/resource-guide.md` | Public + internal docs | Document constraint-map form, element constraints, ref semantics |
| `agent_docs/codegen-patterns.md`, `docs/openapi.md` | Public + internal docs | Document new OpenAPI element emission |
| `CHANGELOG.md` | Changelog | One `Fixed` and one `Added` entry under `[Unreleased]` |

---

## Task 1: Branch + extract migrate's next-version computation into a pure helper

**Files:**
- Modify: `shaperail-cli/src/commands/migrate.rs`

- [ ] **Step 1: Create the branch**

```bash
git checkout main
git pull --ff-only origin main
git checkout -b fix/items-spec-and-migration-numbering
```

- [ ] **Step 2: Add a failing unit test for the new helper**

Append to `shaperail-cli/src/commands/migrate.rs` inside a new `#[cfg(test)] mod tests`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_version_empty_dir() {
        let files: Vec<String> = vec![];
        assert_eq!(compute_next_version(&files), 1);
    }

    #[test]
    fn next_version_picks_max_plus_one_with_gap() {
        // Mirrors the bug-report repro: 0001-0006, gap at 0007, 0008-0010 with
        // 0010 hand-written (non-_create_*). New file must be 0011, not 0010.
        let files = vec![
            "0001_create_organizations.sql".to_string(),
            "0002_create_users.sql".to_string(),
            "0006_create_accounts.sql".to_string(),
            "0008_create_journal_entries.sql".to_string(),
            "0009_create_journal_lines.sql".to_string(),
            "0010_m02_ledger_invariants.sql".to_string(),
        ];
        assert_eq!(compute_next_version(&files), 11);
    }

    #[test]
    fn next_version_ignores_non_numeric_prefix() {
        let files = vec!["readme.sql".to_string(), "0003_create_x.sql".to_string()];
        assert_eq!(compute_next_version(&files), 4);
    }
}
```

- [ ] **Step 3: Run the test to confirm it fails**

```bash
cargo test -p shaperail-cli --lib commands::migrate::tests
```

Expected: compile error — `compute_next_version` not yet defined.

- [ ] **Step 4: Add the helper, replacing the count-based logic**

In `shaperail-cli/src/commands/migrate.rs`, immediately above the existing `fn list_migration_files`, add:

```rust
/// Returns the next free integer prefix above the highest existing one.
/// Robust to gaps and non-`_create_*` migrations (e.g. hand-written invariants files).
fn compute_next_version(filenames: &[String]) -> u32 {
    filenames
        .iter()
        .filter_map(|name| {
            name.split_once('_')
                .and_then(|(prefix, _)| prefix.parse::<u32>().ok())
        })
        .max()
        .map(|m| m + 1)
        .unwrap_or(1)
}
```

Then in `pub fn run`, replace this block (around lines 26–49):

```rust
    // Generate migration SQL from resource definitions
    for resource in &resources {
        let migration_name = format!("create_{}", resource.resource);
        let sql = render_migration_sql(resource);

        // Find next migration number
        let existing = list_migration_files(migrations_dir);
        let next_num = existing.len() + 1;
        let filename = format!("{next_num:04}_{migration_name}.sql");
        let path = migrations_dir.join(&filename);

        if migration_exists(migrations_dir, &migration_name) {
            println!(
                "Migration for '{}' already exists, skipping",
                resource.resource
            );
            continue;
        }
```

with:

```rust
    // Compute the next free version once; increment locally per emitted file
    // so multi-resource invocations get distinct numbers.
    let mut next_version = compute_next_version(&list_migration_files(migrations_dir));

    // Generate migration SQL from resource definitions
    for resource in &resources {
        let migration_name = format!("create_{}", resource.resource);
        let sql = render_migration_sql(resource);

        if migration_exists(migrations_dir, &migration_name) {
            println!(
                "Migration for '{}' already exists, skipping",
                resource.resource
            );
            continue;
        }

        let filename = format!("{next_version:04}_{migration_name}.sql");
        let path = migrations_dir.join(&filename);
        next_version += 1;
```

- [ ] **Step 5: Run the test to confirm it passes**

```bash
cargo test -p shaperail-cli --lib commands::migrate::tests
```

Expected: 3 passed.

- [ ] **Step 6: Run clippy + workspace tests**

```bash
cargo clippy -p shaperail-cli -- -D warnings
cargo test -p shaperail-cli
```

Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add shaperail-cli/src/commands/migrate.rs
git commit -m "$(cat <<'EOF'
fix(shaperail-cli): use max prefix for migration numbering

Previously `shaperail migrate` numbered new migrations using
existing.len() + 1, which collided with hand-written invariants
migrations that sit past the highest auto-generated _create_*
file. Now the next file always uses max(numeric_prefix) + 1 and
increments locally across multi-resource invocations.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add `ItemsSpec` type with custom dual-form `Deserialize`

**Files:**
- Modify: `shaperail-core/src/schema.rs`
- Modify: `shaperail-core/src/lib.rs`

- [ ] **Step 1: Write failing tests for `ItemsSpec` deserialize**

In `shaperail-core/src/schema.rs`, append to the existing `#[cfg(test)] mod tests` block (above the closing `}`):

```rust
    #[test]
    fn items_spec_bare_string_form() {
        let yaml = r#"type: array
items: string"#;
        let fs: FieldSchema = serde_yaml::from_str(yaml).unwrap();
        let items = fs.items.expect("items present");
        assert_eq!(items.field_type, FieldType::String);
        assert!(items.min.is_none());
        assert!(items.max.is_none());
        assert!(items.values.is_none());
        assert!(items.reference.is_none());
        assert!(items.format.is_none());
    }

    #[test]
    fn items_spec_full_map_form() {
        let yaml = r#"type: array
items: { type: string, min: 3, max: 3 }"#;
        let fs: FieldSchema = serde_yaml::from_str(yaml).unwrap();
        let items = fs.items.expect("items present");
        assert_eq!(items.field_type, FieldType::String);
        assert_eq!(items.min, Some(serde_json::json!(3)));
        assert_eq!(items.max, Some(serde_json::json!(3)));
    }

    #[test]
    fn items_spec_enum_form() {
        let yaml = r#"type: array
items: { type: enum, values: [a, b, c] }"#;
        let fs: FieldSchema = serde_yaml::from_str(yaml).unwrap();
        let items = fs.items.expect("items present");
        assert_eq!(items.field_type, FieldType::Enum);
        assert_eq!(
            items.values.as_deref(),
            Some(["a".to_string(), "b".to_string(), "c".to_string()].as_slice())
        );
    }

    #[test]
    fn items_spec_uuid_ref_form() {
        let yaml = r#"type: array
items: { type: uuid, ref: organizations.id }"#;
        let fs: FieldSchema = serde_yaml::from_str(yaml).unwrap();
        let items = fs.items.expect("items present");
        assert_eq!(items.field_type, FieldType::Uuid);
        assert_eq!(items.reference.as_deref(), Some("organizations.id"));
    }

    #[test]
    fn items_spec_unknown_field_rejected() {
        let yaml = r#"type: array
items: { type: string, unknown_key: 1 }"#;
        let result: Result<FieldSchema, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "unknown field on ItemsSpec must be rejected");
    }
```

`serde_yaml` is already a workspace dev-dep on `shaperail-core` (`serde_yaml = { workspace = true }`), so no Cargo.toml change is needed.

- [ ] **Step 2: Run the new tests to confirm they fail**

```bash
cargo test -p shaperail-core schema::tests::items_spec
```

Expected: compile error — `ItemsSpec` not defined and the field's type is still `Option<String>`.

- [ ] **Step 3: Define `ItemsSpec` and switch `FieldSchema.items` to `Option<ItemsSpec>`**

Replace `shaperail-core/src/schema.rs` lines around the existing `pub items` field (line 71) with the new declaration AND add the `ItemsSpec` type definition above `FieldSchema`. Full text of the changes:

In `shaperail-core/src/schema.rs`, near the top under the existing `use` line:

```rust
use crate::FieldType;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;
```

Above `pub struct FieldSchema`, add:

```rust
/// Element specification for `type: array` fields.
///
/// Accepts two YAML shapes — both are equivalent for fields that need no element
/// constraints:
///
/// ```yaml
/// items: string                             # bare-name shorthand
/// items: { type: string }                   # equivalent map form
/// items: { type: string, min: 3, max: 3 }   # element-level constraints
/// items: { type: enum, values: [a, b] }     # element allowlist
/// items: { type: uuid, ref: organizations.id }  # FK array (Postgres only)
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
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

impl ItemsSpec {
    /// Constructs a bare `ItemsSpec` with only `field_type` set.
    pub fn of(field_type: FieldType) -> Self {
        Self {
            field_type,
            min: None,
            max: None,
            format: None,
            values: None,
            reference: None,
        }
    }
}

impl<'de> Deserialize<'de> for ItemsSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ItemsSpecVisitor;

        impl<'de> Visitor<'de> for ItemsSpecVisitor {
            type Value = ItemsSpec;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a type name (e.g. \"string\") or a constraint map with `type:`")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                let field_type = FieldType::deserialize(de::value::StrDeserializer::new(v))?;
                Ok(ItemsSpec::of(field_type))
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Inner {
                    #[serde(rename = "type")]
                    field_type: FieldType,
                    #[serde(default)]
                    min: Option<serde_json::Value>,
                    #[serde(default)]
                    max: Option<serde_json::Value>,
                    #[serde(default)]
                    format: Option<String>,
                    #[serde(default)]
                    values: Option<Vec<String>>,
                    #[serde(default, rename = "ref")]
                    reference: Option<String>,
                }
                let inner = Inner::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(ItemsSpec {
                    field_type: inner.field_type,
                    min: inner.min,
                    max: inner.max,
                    format: inner.format,
                    values: inner.values,
                    reference: inner.reference,
                })
            }
        }

        deserializer.deserialize_any(ItemsSpecVisitor)
    }
}
```

Then change the `items` field on `FieldSchema` from:

```rust
    pub items: Option<String>,
```

to:

```rust
    pub items: Option<ItemsSpec>,
```

- [ ] **Step 4: Update the existing `field_schema_serde_roundtrip` test in the same file**

In `shaperail-core/src/schema.rs`, find the `field_schema_serde_roundtrip` test (around line 141) and replace the `items: None,` line with `items: None,` (no change — verify it still compiles). The test's `FieldSchema` literal initializer continues to use `items: None`.

The `field_schema_full` test does not reference `items`, so no change.

- [ ] **Step 5: Re-export `ItemsSpec` from the crate root**

In `shaperail-core/src/lib.rs`, find the `pub use schema::FieldSchema;` line and replace with:

```rust
pub use schema::{FieldSchema, ItemsSpec};
```

- [ ] **Step 6: Run the new tests**

```bash
cargo test -p shaperail-core schema::tests
```

Expected: all 5 new `items_spec_*` tests pass; existing tests still pass.

- [ ] **Step 7: Workspace builds (will surface every consumer that needs updating)**

```bash
cargo build --workspace 2>&1 | head -100
```

Expected: a handful of compile errors in `shaperail-runtime/src/db/query.rs`, `shaperail-codegen/src/rust.rs`, etc., where `field.items.as_deref()` and `field.items.as_str()` are used. We fix these in Tasks 3–4.

- [ ] **Step 8: Stage but do not commit yet** — this work is part of the items-spec feature commit; we'll bundle it after the consumer fixes compile.

---

## Task 3: Update `shaperail-runtime/src/db/query.rs` to read through `ItemsSpec`

**Files:**
- Modify: `shaperail-runtime/src/db/query.rs:1023-1035` (production)
- Modify: `shaperail-runtime/src/db/query.rs:1451` (one stale test fixture)

- [ ] **Step 1: Update the production code**

Replace lines 1023–1035 (the `FieldType::Array` arm of `field_type_to_sql_postgres`):

```rust
        FieldType::Array => {
            if let Some(items) = &field.items {
                let item_sql = match items.as_str() {
                    "string" => "TEXT",
                    "integer" => "INTEGER",
                    "uuid" => "UUID",
                    _ => "TEXT",
                };
                format!("{item_sql}[]")
            } else {
                "TEXT[]".to_string()
            }
        }
```

with:

```rust
        FieldType::Array => {
            if let Some(items) = &field.items {
                let item_sql = match items.field_type {
                    FieldType::String | FieldType::Enum => "TEXT",
                    FieldType::Integer => "INTEGER",
                    FieldType::Bigint => "BIGINT",
                    FieldType::Number => "DOUBLE PRECISION",
                    FieldType::Boolean => "BOOLEAN",
                    FieldType::Timestamp => "TIMESTAMPTZ",
                    FieldType::Date => "DATE",
                    FieldType::Uuid => "UUID",
                    _ => "TEXT",
                };
                format!("{item_sql}[]")
            } else {
                "TEXT[]".to_string()
            }
        }
```

- [ ] **Step 2: Update the test fixture at line 1451**

The existing test currently has:

```rust
            items: Some("string".to_string()),
```

Replace with:

```rust
            items: Some(shaperail_core::ItemsSpec::of(shaperail_core::FieldType::String)),
```

- [ ] **Step 3: Run the existing test in this module**

```bash
cargo test -p shaperail-runtime --lib db::query::tests::field_type_to_sql
```

Expected: pre-existing tests still pass. (The `field_type_to_sql` Array test asserts `TEXT[]` for `items: Some(ItemsSpec::of(String))`, which the new code path produces.)

---

## Task 4: Update `shaperail-codegen/src/rust.rs::sql_cast_type` and `query_type` to read through `ItemsSpec`

**Files:**
- Modify: `shaperail-codegen/src/rust.rs:1279-1311`

- [ ] **Step 1: Update both helpers**

Replace the existing `FieldType::Array` arms in `sql_cast_type` (line 1279) and `query_type` (line 1301):

For `sql_cast_type`:

```rust
        FieldType::Array => match field.items.as_deref() {
            Some("uuid") => "uuid[]".to_string(),
            Some("integer") => "integer[]".to_string(),
            Some("bigint") => "bigint[]".to_string(),
            Some("number") => "double precision[]".to_string(),
            Some("boolean") => "boolean[]".to_string(),
            _ => "text[]".to_string(),
        },
```

becomes:

```rust
        FieldType::Array => match field.items.as_ref().map(|i| &i.field_type) {
            Some(FieldType::Uuid) => "uuid[]".to_string(),
            Some(FieldType::Integer) => "integer[]".to_string(),
            Some(FieldType::Bigint) => "bigint[]".to_string(),
            Some(FieldType::Number) => "double precision[]".to_string(),
            Some(FieldType::Boolean) => "boolean[]".to_string(),
            Some(FieldType::Timestamp) => "timestamptz[]".to_string(),
            Some(FieldType::Date) => "date[]".to_string(),
            _ => "text[]".to_string(),
        },
```

For `query_type`:

```rust
        FieldType::Array => match field.items.as_deref() {
            Some("uuid") => "Vec<uuid::Uuid>".to_string(),
            Some("integer") => "Vec<i32>".to_string(),
            Some("bigint") => "Vec<i64>".to_string(),
            Some("number") => "Vec<f64>".to_string(),
            Some("boolean") => "Vec<bool>".to_string(),
            Some("timestamp") => "Vec<chrono::DateTime<chrono::Utc>>".to_string(),
            Some("date") => "Vec<chrono::NaiveDate>".to_string(),
            _ => "Vec<String>".to_string(),
        },
```

becomes:

```rust
        FieldType::Array => match field.items.as_ref().map(|i| &i.field_type) {
            Some(FieldType::Uuid) => "Vec<uuid::Uuid>".to_string(),
            Some(FieldType::Integer) => "Vec<i32>".to_string(),
            Some(FieldType::Bigint) => "Vec<i64>".to_string(),
            Some(FieldType::Number) => "Vec<f64>".to_string(),
            Some(FieldType::Boolean) => "Vec<bool>".to_string(),
            Some(FieldType::Timestamp) => "Vec<chrono::DateTime<chrono::Utc>>".to_string(),
            Some(FieldType::Date) => "Vec<chrono::NaiveDate>".to_string(),
            _ => "Vec<String>".to_string(),
        },
```

- [ ] **Step 2: Build the workspace and confirm only test fixtures fail next**

```bash
cargo build --workspace 2>&1 | tail -40
```

Expected: production code compiles. Test fixtures across `shaperail-codegen` and `shaperail-runtime` still pass because they all use `items: None`.

---

## Task 5: Validator rules for `ItemsSpec`

**Files:**
- Modify: `shaperail-codegen/src/validator.rs`
- Modify: `shaperail-codegen/src/diagnostics.rs`

- [ ] **Step 1: Read the current diagnostic-code conventions**

```bash
grep -n "code: " /Users/Mahin/Desktop/shaperail/shaperail-codegen/src/diagnostics.rs | head -30
```

Pick the next 5 free codes (e.g. SR043–SR047 if highest is SR042; adjust based on output). The exact numbers don't matter — they just need to be free and consecutive.

- [ ] **Step 2: Write failing tests**

In `shaperail-codegen/src/validator.rs`, append to the existing `#[cfg(test)] mod tests` block:

```rust
    fn array_field(items: shaperail_core::ItemsSpec) -> shaperail_core::FieldSchema {
        shaperail_core::FieldSchema {
            field_type: shaperail_core::FieldType::Array,
            primary: false,
            generated: false,
            required: false,
            unique: false,
            nullable: false,
            reference: None,
            min: None,
            max: None,
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: false,
            items: Some(items),
            transient: false,
        }
    }

    fn resource_with_array(name: &str, field: shaperail_core::FieldSchema) -> shaperail_core::ResourceDefinition {
        let mut schema = indexmap::IndexMap::new();
        schema.insert(name.to_string(), field);
        shaperail_core::ResourceDefinition {
            resource: "test".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: None,
            relations: None,
            indexes: None,
        }
    }

    #[test]
    fn items_nested_array_rejected() {
        let field = array_field(shaperail_core::ItemsSpec::of(shaperail_core::FieldType::Array));
        let rd = resource_with_array("nested", field);
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e.message.contains("nested array")
            || e.message.contains("type: json")));
    }

    #[test]
    fn items_enum_requires_values() {
        let field = array_field(shaperail_core::ItemsSpec::of(shaperail_core::FieldType::Enum));
        let rd = resource_with_array("flags", field);
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e.message.contains("values")));
    }

    #[test]
    fn items_format_only_on_string() {
        let mut items = shaperail_core::ItemsSpec::of(shaperail_core::FieldType::Integer);
        items.format = Some("email".to_string());
        let field = array_field(items);
        let rd = resource_with_array("nums", field);
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e.message.contains("format")));
    }

    #[test]
    fn items_ref_requires_uuid() {
        let mut items = shaperail_core::ItemsSpec::of(shaperail_core::FieldType::String);
        items.reference = Some("organizations.id".to_string());
        let field = array_field(items);
        let rd = resource_with_array("badrefs", field);
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e.message.contains("ref")
            && e.message.contains("uuid")));
    }

    #[test]
    fn items_ref_format_must_be_resource_dot_field() {
        let mut items = shaperail_core::ItemsSpec::of(shaperail_core::FieldType::Uuid);
        items.reference = Some("organizations".to_string());  // missing .id
        let field = array_field(items);
        let rd = resource_with_array("orgs", field);
        let errors = validate_resource(&rd);
        assert!(errors.iter().any(|e| e.message.contains("resource.field")));
    }

    #[test]
    fn items_uuid_ref_valid() {
        let mut items = shaperail_core::ItemsSpec::of(shaperail_core::FieldType::Uuid);
        items.reference = Some("organizations.id".to_string());
        let field = array_field(items);
        let rd = resource_with_array("tags", field);
        let errors = validate_resource(&rd);
        // No errors related to this field.
        assert!(errors.iter().all(|e| !e.message.contains("tags")));
    }
```

If `validate_resource` has a different name in this file, adjust accordingly. Look at the existing test cases to copy the exact entry point.

- [ ] **Step 3: Run tests to confirm they fail**

```bash
cargo test -p shaperail-codegen --lib validator::tests::items_
```

Expected: the new tests fail (validator currently has no element-level rules beyond `items.is_none()`).

- [ ] **Step 4: Implement the rules**

In `shaperail-codegen/src/validator.rs`, find the existing block that emits `"resource '{res}': field '{name}' is type array but has no items"` (around line 85). Immediately after that block, add:

```rust
        // Element-level rules for items
        if let Some(items_spec) = &field.items {
            // No nested arrays — clearer error than the generic deny_unknown_fields.
            if items_spec.field_type == FieldType::Array {
                errors.push(err(&format!(
                    "resource '{res}': field '{name}' has nested array items; use type: json for nested arrays"
                )));
            }

            // Enum items require values: [...]
            if items_spec.field_type == FieldType::Enum && items_spec.values.is_none() {
                errors.push(err(&format!(
                    "resource '{res}': field '{name}' has enum items but no values; add `values: [...]` to items"
                )));
            }

            // format only valid for string element type
            if items_spec.format.is_some() && items_spec.field_type != FieldType::String {
                errors.push(err(&format!(
                    "resource '{res}': field '{name}' has items.format but items.type is not string"
                )));
            }

            // ref only valid on uuid element type
            if items_spec.reference.is_some() && items_spec.field_type != FieldType::Uuid {
                errors.push(err(&format!(
                    "resource '{res}': field '{name}' has items.ref but items.type is not uuid"
                )));
            }

            // ref must be in resource.field shape
            if let Some(reference) = &items_spec.reference {
                if !reference.contains('.') {
                    errors.push(err(&format!(
                        "resource '{res}': field '{name}' items.ref must be in 'resource.field' format, got '{reference}'"
                    )));
                }
            }
        }
```

The `err(...)` helper, `errors`, and `res` / `name` bindings already exist in this loop — copy the surrounding usage to confirm.

- [ ] **Step 5: Run the tests**

```bash
cargo test -p shaperail-codegen --lib validator::tests
```

Expected: all 6 new tests pass; pre-existing tests still pass.

- [ ] **Step 6: Update `diagnostics.rs` parallel rules**

Look at `shaperail-codegen/src/diagnostics.rs:118` — the existing array-needs-items diagnostic. Right after that block, add five parallel `Diagnostic` entries for the new validator rules with codes from Step 1:

```rust
        if let Some(items_spec) = &field.items {
            if items_spec.field_type == FieldType::Array {
                diagnostics.push(Diagnostic {
                    code: "SR0XX".to_string(),  // replace with picked code
                    error: format!("resource '{res}': field '{name}' has nested array items"),
                    fix: format!("change items to type: json (nested arrays are not supported)"),
                    example: format!("{name}: {{ type: json }}"),
                });
            }
            if items_spec.field_type == FieldType::Enum && items_spec.values.is_none() {
                diagnostics.push(Diagnostic {
                    code: "SR0XX".to_string(),
                    error: format!("resource '{res}': field '{name}' enum items missing values"),
                    fix: format!("add `values: [...]` to items"),
                    example: format!("{name}: {{ type: array, items: {{ type: enum, values: [a, b] }} }}"),
                });
            }
            if items_spec.format.is_some() && items_spec.field_type != FieldType::String {
                diagnostics.push(Diagnostic {
                    code: "SR0XX".to_string(),
                    error: format!("resource '{res}': field '{name}' items.format only valid on string"),
                    fix: format!("remove items.format or change items.type to string"),
                    example: format!("{name}: {{ type: array, items: {{ type: string, format: email }} }}"),
                });
            }
            if items_spec.reference.is_some() && items_spec.field_type != FieldType::Uuid {
                diagnostics.push(Diagnostic {
                    code: "SR0XX".to_string(),
                    error: format!("resource '{res}': field '{name}' items.ref requires items.type uuid"),
                    fix: format!("change items.type to uuid, or remove items.ref"),
                    example: format!("{name}: {{ type: array, items: {{ type: uuid, ref: organizations.id }} }}"),
                });
            }
            if let Some(reference) = &items_spec.reference {
                if !reference.contains('.') {
                    diagnostics.push(Diagnostic {
                        code: "SR0XX".to_string(),
                        error: format!("resource '{res}': field '{name}' items.ref must be 'resource.field'"),
                        fix: format!("write items.ref as 'resource_name.column_name'"),
                        example: format!("items: {{ type: uuid, ref: organizations.id }}"),
                    });
                }
            }
        }
```

Replace each `SR0XX` with the specific code chosen in Step 1.

- [ ] **Step 7: Build and test**

```bash
cargo build -p shaperail-codegen
cargo test -p shaperail-codegen
```

Expected: clean.

---

## Task 6: Runtime element validation in `check_field_rules`

**Files:**
- Modify: `shaperail-runtime/src/handlers/validate.rs`

- [ ] **Step 1: Write failing tests**

In `shaperail-runtime/src/handlers/validate.rs`, append to `#[cfg(test)] mod tests`:

```rust
    fn array_resource(items: shaperail_core::ItemsSpec, required: bool) -> ResourceDefinition {
        let mut schema = indexmap::IndexMap::new();
        schema.insert(
            "tags".to_string(),
            FieldSchema {
                field_type: FieldType::Array,
                primary: false,
                generated: false,
                required,
                unique: false,
                nullable: false,
                reference: None,
                min: None,
                max: None,
                format: None,
                values: None,
                default: None,
                sensitive: false,
                search: false,
                items: Some(items),
                transient: false,
            },
        );
        ResourceDefinition {
            resource: "items".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: None,
            relations: None,
            indexes: None,
        }
    }

    #[test]
    fn array_string_item_too_short() {
        let mut items = shaperail_core::ItemsSpec::of(FieldType::String);
        items.min = Some(serde_json::json!(3));
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert("tags".to_string(), serde_json::json!(["ab", "abcd"]));

        let result = validate_input(&data, &resource);
        let errors = match result {
            Err(ShaperailError::Validation(e)) => e,
            _ => panic!("expected validation error"),
        };
        assert!(errors.iter().any(|e| e.field == "tags[0]" && e.code == "too_short"));
        assert!(errors.iter().all(|e| e.field != "tags[1]"));
    }

    #[test]
    fn array_enum_item_invalid() {
        let mut items = shaperail_core::ItemsSpec::of(FieldType::Enum);
        items.values = Some(vec!["red".to_string(), "blue".to_string()]);
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert("tags".to_string(), serde_json::json!(["red", "purple"]));

        let result = validate_input(&data, &resource);
        let errors = match result {
            Err(ShaperailError::Validation(e)) => e,
            _ => panic!("expected validation error"),
        };
        assert!(errors.iter().any(|e| e.field == "tags[1]" && e.code == "invalid_enum"));
    }

    #[test]
    fn array_uuid_item_invalid() {
        let items = shaperail_core::ItemsSpec::of(FieldType::Uuid);
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert(
            "tags".to_string(),
            serde_json::json!(["00000000-0000-0000-0000-000000000001", "not-a-uuid"]),
        );

        let result = validate_input(&data, &resource);
        let errors = match result {
            Err(ShaperailError::Validation(e)) => e,
            _ => panic!("expected validation error"),
        };
        assert!(errors.iter().any(|e| e.field == "tags[1]" && e.code == "invalid_uuid"));
    }

    #[test]
    fn array_integer_item_too_large() {
        let mut items = shaperail_core::ItemsSpec::of(FieldType::Integer);
        items.max = Some(serde_json::json!(10));
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert("tags".to_string(), serde_json::json!([1, 5, 100]));

        let result = validate_input(&data, &resource);
        let errors = match result {
            Err(ShaperailError::Validation(e)) => e,
            _ => panic!("expected validation error"),
        };
        assert!(errors.iter().any(|e| e.field == "tags[2]" && e.code == "too_large"));
    }

    #[test]
    fn array_empty_passes() {
        let items = shaperail_core::ItemsSpec::of(FieldType::String);
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert("tags".to_string(), serde_json::json!([]));

        assert!(validate_input(&data, &resource).is_ok());
    }

    #[test]
    fn array_value_must_be_array() {
        let items = shaperail_core::ItemsSpec::of(FieldType::String);
        let resource = array_resource(items, false);
        let mut data = serde_json::Map::new();
        data.insert("tags".to_string(), serde_json::json!("not-an-array"));

        let result = validate_input(&data, &resource);
        let errors = match result {
            Err(ShaperailError::Validation(e)) => e,
            _ => panic!("expected validation error"),
        };
        assert!(errors.iter().any(|e| e.field == "tags" && e.code == "invalid_type"));
    }
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p shaperail-runtime --lib handlers::validate::tests::array_
```

Expected: all 6 fail (current `check_field_rules` doesn't iterate elements).

- [ ] **Step 3: Implement element-rule application**

In `shaperail-runtime/src/handlers/validate.rs`, at the end of `check_field_rules` (after the `FieldType::Uuid` block, around line 207), add:

```rust
    if field.field_type == FieldType::Array {
        if let Some(items_spec) = &field.items {
            match value {
                serde_json::Value::Array(elements) => {
                    for (idx, element) in elements.iter().enumerate() {
                        check_item_rules(name, idx, items_spec, element, errors);
                    }
                }
                _ => {
                    errors.push(FieldError {
                        field: name.to_string(),
                        message: format!("{name} must be an array"),
                        code: "invalid_type".to_string(),
                    });
                }
            }
        }
    }
```

Then add a new private helper at the bottom of the file (before `#[cfg(test)]`):

```rust
fn check_item_rules(
    field_name: &str,
    idx: usize,
    items: &shaperail_core::ItemsSpec,
    value: &serde_json::Value,
    errors: &mut Vec<FieldError>,
) {
    let path = format!("{field_name}[{idx}]");

    // String / enum length checks
    if matches!(items.field_type, FieldType::String | FieldType::Enum) {
        if let Some(s) = value.as_str() {
            if let Some(min) = items.min.as_ref().and_then(|v| v.as_u64()) {
                if (s.len() as u64) < min {
                    errors.push(FieldError {
                        field: path.clone(),
                        message: format!("{path} must be at least {min} characters"),
                        code: "too_short".to_string(),
                    });
                }
            }
            if let Some(max) = items.max.as_ref().and_then(|v| v.as_u64()) {
                if (s.len() as u64) > max {
                    errors.push(FieldError {
                        field: path.clone(),
                        message: format!("{path} must be at most {max} characters"),
                        code: "too_long".to_string(),
                    });
                }
            }
        }
    }

    // Numeric range checks
    if matches!(
        items.field_type,
        FieldType::Integer | FieldType::Bigint | FieldType::Number
    ) {
        if let Some(n) = value.as_f64() {
            if let Some(min) = items.min.as_ref().and_then(|v| v.as_f64()) {
                if n < min {
                    errors.push(FieldError {
                        field: path.clone(),
                        message: format!("{path} must be at least {min}"),
                        code: "too_small".to_string(),
                    });
                }
            }
            if let Some(max) = items.max.as_ref().and_then(|v| v.as_f64()) {
                if n > max {
                    errors.push(FieldError {
                        field: path.clone(),
                        message: format!("{path} must be at most {max}"),
                        code: "too_large".to_string(),
                    });
                }
            }
        }
    }

    // Enum allowlist
    if items.field_type == FieldType::Enum {
        if let (Some(allowed), Some(s)) = (&items.values, value.as_str()) {
            if !allowed.contains(&s.to_string()) {
                errors.push(FieldError {
                    field: path.clone(),
                    message: format!("{path} must be one of: {}", allowed.join(", ")),
                    code: "invalid_enum".to_string(),
                });
            }
        }
    }

    // UUID parse
    if items.field_type == FieldType::Uuid {
        if let Some(s) = value.as_str() {
            if uuid::Uuid::parse_str(s).is_err() {
                errors.push(FieldError {
                    field: path.clone(),
                    message: format!("{path} must be a valid UUID"),
                    code: "invalid_uuid".to_string(),
                });
            }
        }
    }

    // Email / URL format on string elements
    if items.format.as_deref() == Some("email") {
        if let Some(s) = value.as_str() {
            if !s.contains('@') || !s.contains('.') {
                errors.push(FieldError {
                    field: path.clone(),
                    message: format!("{path} must be a valid email address"),
                    code: "invalid_format".to_string(),
                });
            }
        }
    }
    if items.format.as_deref() == Some("url") {
        if let Some(s) = value.as_str() {
            if !s.starts_with("http://") && !s.starts_with("https://") {
                errors.push(FieldError {
                    field: path.clone(),
                    message: format!("{path} must be a valid URL"),
                    code: "invalid_format".to_string(),
                });
            }
        }
    }
}
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p shaperail-runtime --lib handlers::validate::tests
```

Expected: all 6 new array tests pass; existing tests still pass.

---

## Task 7: `validate_item_references` runtime existence check

**Files:**
- Modify: `shaperail-runtime/src/handlers/validate.rs`
- Modify: `shaperail-runtime/src/handlers/crud.rs`

- [ ] **Step 1: Add the new public async function**

Append to `shaperail-runtime/src/handlers/validate.rs` (above `#[cfg(test)]`):

```rust
/// Validates that every element of every `items.ref` array field exists in
/// the referenced table. Postgres-only. Runs after phase-2 validation,
/// before INSERT/UPDATE.
///
/// Issues at most one query per FK-array column per write, and only when
/// the field is present and non-empty.
pub async fn validate_item_references(
    data: &serde_json::Map<String, serde_json::Value>,
    resource: &ResourceDefinition,
    pool: &sqlx::PgPool,
) -> Result<(), ShaperailError> {
    let mut errors = Vec::new();

    for (name, field) in &resource.schema {
        let Some(items) = &field.items else { continue };
        let Some(reference) = &items.reference else { continue };
        if items.field_type != FieldType::Uuid {
            continue;
        }

        let Some(serde_json::Value::Array(elements)) = data.get(name) else { continue };
        if elements.is_empty() {
            continue;
        }

        let Some((table, column)) = reference.split_once('.') else { continue };

        // Parse all elements as UUIDs (any malformed element should already have
        // been caught by phase-1 validation; defensive parse here too).
        let mut uuids: Vec<uuid::Uuid> = Vec::with_capacity(elements.len());
        for element in elements {
            if let Some(s) = element.as_str() {
                if let Ok(u) = uuid::Uuid::parse_str(s) {
                    uuids.push(u);
                }
            }
        }
        if uuids.is_empty() {
            continue;
        }

        // SAFETY: table/column come from a validated `resource.field` reference
        // string in the resource YAML, which has already been parser-checked.
        // They are NOT user input at runtime.
        let sql = format!(
            "SELECT COUNT(DISTINCT \"{column}\") FROM \"{table}\" WHERE \"{column}\" = ANY($1::uuid[])"
        );
        let row: (i64,) = sqlx::query_as(&sql)
            .bind(&uuids)
            .fetch_one(pool)
            .await
            .map_err(|e| ShaperailError::Internal(format!("items.ref check failed: {e}")))?;

        let found = row.0 as usize;
        let distinct: std::collections::HashSet<_> = uuids.iter().collect();
        if found < distinct.len() {
            // Compute missing IDs by querying once more (cheap, capped at distinct.len()).
            let sql_present = format!(
                "SELECT \"{column}\" FROM \"{table}\" WHERE \"{column}\" = ANY($1::uuid[])"
            );
            let present_rows: Vec<(uuid::Uuid,)> = sqlx::query_as(&sql_present)
                .bind(&uuids)
                .fetch_all(pool)
                .await
                .map_err(|e| ShaperailError::Internal(format!("items.ref check failed: {e}")))?;
            let present: std::collections::HashSet<uuid::Uuid> =
                present_rows.into_iter().map(|(u,)| u).collect();
            let missing: Vec<uuid::Uuid> = distinct
                .into_iter()
                .filter(|u| !present.contains(u))
                .copied()
                .take(5)
                .collect();
            errors.push(FieldError {
                field: name.clone(),
                message: format!(
                    "{name} contains references that do not exist in {reference}: {}",
                    missing
                        .iter()
                        .map(|u| u.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                code: "invalid_reference".to_string(),
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ShaperailError::Validation(errors))
    }
}
```

- [ ] **Step 2: Wire it into the CRUD write paths**

In `shaperail-runtime/src/handlers/crud.rs`, find each call site of `validate_required_present` (and the equivalent in the no-controller path, `validate_input`). For each create / update / bulk_create write path, add immediately after the required-present check (and after `strip_transient_fields`):

```rust
    validate_item_references(&input_data, &resource, pool).await?;
```

`pool` is the `PgPool` already in scope at each handler. Ensure `validate_item_references` is brought into scope by extending the existing `use crate::handlers::validate::{...}` import block.

Concretely, the imports near line 26 of `crud.rs` currently look like:

```rust
use crate::handlers::validate::{
    strip_transient_fields, validate_input, validate_input_shape, validate_required_present,
};
```

Change to:

```rust
use crate::handlers::validate::{
    strip_transient_fields, validate_input, validate_input_shape, validate_item_references,
    validate_required_present,
};
```

And in each of the create/update/bulk_create handler bodies, add the `validate_item_references` call exactly once per handler, immediately before the `INSERT` / `UPDATE` SQL. Use `grep -n "validate_required_present\|validate_input\b" shaperail-runtime/src/handlers/crud.rs` to find every site.

- [ ] **Step 3: Add an integration test**

In `shaperail-runtime/tests/db_integration.rs`, find the existing `setup` / fixture pattern (look at the top 100 lines for a model). Add a test resource declaring an `items.ref` array field, then write two test cases:

```rust
// Pseudocode shape — adapt to existing fixture macros.
#[sqlx::test]
async fn items_ref_happy_path(pool: PgPool) {
    // Insert two organizations with known IDs.
    // Create a vendor row referencing both org IDs in a tags: uuid[] field.
    // Assert success.
}

#[sqlx::test]
async fn items_ref_missing_id_rejects(pool: PgPool) {
    // Insert one organization.
    // Try to create a vendor with tags: [<existing_org_id>, <random_uuid>].
    // Expect ShaperailError::Validation with code "invalid_reference" and the
    // random_uuid surfaced in the error message.
}
```

The exact mechanics depend on whether `db_integration.rs` already has helpers to spin up a resource. Read the existing test entries in that file before writing — copy structure from a happy-path / rejection pair already there (e.g. a unique-constraint or required-field test).

- [ ] **Step 4: Run the integration test**

```bash
docker compose up -d
DATABASE_URL=postgres://postgres:postgres@localhost:5432/shaperail_test \
  cargo test -p shaperail-runtime --test db_integration items_ref
```

Expected: both tests pass.

- [ ] **Step 5: Run the full workspace tests**

```bash
cargo test --workspace
```

Expected: clean.

---

## Task 8: OpenAPI element-level emission

**Files:**
- Modify: `shaperail-codegen/src/openapi.rs`

- [ ] **Step 1: Read current array branch**

```bash
sed -n '260,290p' /Users/Mahin/Desktop/shaperail/shaperail-codegen/src/openapi.rs
```

Identify the block where `items` is currently emitted as an empty `{}`.

- [ ] **Step 2: Write a failing test**

Add to `shaperail-codegen/src/openapi.rs` `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn openapi_array_items_emits_element_constraints() {
        let mut items = shaperail_core::ItemsSpec::of(shaperail_core::FieldType::String);
        items.min = Some(serde_json::json!(3));
        items.max = Some(serde_json::json!(3));

        let mut schema = indexmap::IndexMap::new();
        schema.insert(
            "currencies".to_string(),
            shaperail_core::FieldSchema {
                field_type: shaperail_core::FieldType::Array,
                items: Some(items),
                primary: false,
                generated: false,
                required: false,
                unique: false,
                nullable: false,
                reference: None,
                min: None,
                max: None,
                format: None,
                values: None,
                default: None,
                sensitive: false,
                search: false,
                transient: false,
            },
        );
        let resource = shaperail_core::ResourceDefinition {
            resource: "vendors".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: None,
            relations: None,
            indexes: None,
        };

        let spec = generate_openapi(&[resource]);
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("\"minLength\":3"), "minLength missing: {json}");
        assert!(json.contains("\"maxLength\":3"), "maxLength missing: {json}");
    }
```

If `generate_openapi` is named differently, find the entry point and adjust.

- [ ] **Step 3: Run test to confirm failure**

```bash
cargo test -p shaperail-codegen --lib openapi::tests::openapi_array_items_emits_element_constraints
```

- [ ] **Step 4: Implement element schema emission**

Find the `FieldType::Array =>` arm at line 268. Currently:

```rust
        FieldType::Array => {
            obj.insert("items".to_string(), serde_json::json!({}));
        }
```

Replace with:

```rust
        FieldType::Array => {
            let items_obj = if let Some(items) = &field.items {
                let mut item = serde_json::Map::new();
                let element_type = match items.field_type {
                    FieldType::String | FieldType::Enum | FieldType::Uuid | FieldType::File => "string",
                    FieldType::Integer => "integer",
                    FieldType::Bigint => "integer",
                    FieldType::Number => "number",
                    FieldType::Boolean => "boolean",
                    FieldType::Timestamp | FieldType::Date => "string",
                    FieldType::Json => "object",
                    FieldType::Array => "array",
                };
                item.insert("type".to_string(), serde_json::json!(element_type));
                if matches!(items.field_type, FieldType::Uuid) {
                    item.insert("format".to_string(), serde_json::json!("uuid"));
                }
                if matches!(items.field_type, FieldType::Timestamp) {
                    item.insert("format".to_string(), serde_json::json!("date-time"));
                }
                if matches!(items.field_type, FieldType::Date) {
                    item.insert("format".to_string(), serde_json::json!("date"));
                }
                if let Some(format) = &items.format {
                    item.insert("format".to_string(), serde_json::json!(format));
                }
                if let Some(values) = &items.values {
                    item.insert("enum".to_string(), serde_json::json!(values));
                }
                if let Some(min) = &items.min {
                    let key = if matches!(items.field_type, FieldType::String | FieldType::Enum) {
                        "minLength"
                    } else {
                        "minimum"
                    };
                    item.insert(key.to_string(), min.clone());
                }
                if let Some(max) = &items.max {
                    let key = if matches!(items.field_type, FieldType::String | FieldType::Enum) {
                        "maxLength"
                    } else {
                        "maximum"
                    };
                    item.insert(key.to_string(), max.clone());
                }
                serde_json::Value::Object(item)
            } else {
                serde_json::json!({})
            };
            obj.insert("items".to_string(), items_obj);
        }
```

- [ ] **Step 5: Run the test + module tests**

```bash
cargo test -p shaperail-codegen --lib openapi
```

Expected: clean.

---

## Task 9: JSON Schema element-level emission

**Files:**
- Modify: `shaperail-codegen/src/json_schema.rs`

- [ ] **Step 1: Find where array items are emitted**

```bash
grep -n "FieldType::Array\|items_schema\|\"items\"" /Users/Mahin/Desktop/shaperail/shaperail-codegen/src/json_schema.rs | head -20
```

If `json_schema.rs` only emits the *meta-schema* for resource YAML (not per-resource JSON Schema), there's nothing to change here — the meta-schema already accepts `items: { type: <T>, ... }` because YAML parsing now accepts both shapes upstream. In that case, **skip Task 9** and continue to Task 10.

If a per-resource JSON Schema generator exists for array fields, mirror Task 8's logic against it.

- [ ] **Step 2: Implement only if applicable** (see Step 1)

If applicable, mirror Task 8's element-schema emission against the per-resource JSON Schema generator.

- [ ] **Step 3: Run module tests**

```bash
cargo test -p shaperail-codegen --lib json_schema
```

---

## Task 10: Run full quality gate

- [ ] **Step 1: Format**

```bash
cargo fmt
```

- [ ] **Step 2: Clippy**

```bash
cargo clippy --workspace -- -D warnings
```

Expected: no warnings.

- [ ] **Step 3: Workspace tests**

```bash
cargo test --workspace
```

Expected: clean. If any test fails, fix and re-run; do not skip.

- [ ] **Step 4: Postgres integration tests**

```bash
docker compose up -d
DATABASE_URL=postgres://postgres:postgres@localhost:5432/shaperail_test \
  cargo test -p shaperail-runtime --test db_integration
docker compose down
```

Expected: all integration tests pass.

---

## Task 11: Documentation — public + internal mirror

**Files:**
- Modify: `agent_docs/resource-format.md`
- Modify: `docs/resource-guide.md`
- Modify: `agent_docs/codegen-patterns.md` (if it has an OpenAPI section)
- Modify: `docs/openapi.md`

- [ ] **Step 1: Update `agent_docs/resource-format.md` array section**

Find the line `| array | type[] | Vec<T> | add items: type |` (around line 44). Below the field-types table, add:

```markdown
### Array element constraints

`items:` accepts either a bare type name (shorthand) or a constraint map:

```yaml
schema:
  tags:        { type: array, items: string }                          # legacy shorthand
  currencies:  { type: array, items: { type: string, min: 3, max: 3 } }  # element-level constraints
  scores:      { type: array, items: { type: integer, min: 0, max: 100 } }
  flags:       { type: array, items: { type: enum, values: [a, b, c] } }
  org_ids:     { type: array, items: { type: uuid, ref: organizations.id } }
```

Element-level constraints are validated per element on every write. Errors surface as `<field>[<index>]` (e.g. `currencies[0]` for a too-short string at index 0).

`items.ref` performs a runtime existence check via `SELECT … WHERE id = ANY($1::uuid[])` and rejects the write with code `invalid_reference` if any element doesn't exist. Postgres only — non-Postgres backends are not supported for this feature.

Nested arrays are not supported. Use `type: json` for nested structure.
```

- [ ] **Step 2: Mirror in `docs/resource-guide.md`**

Find the equivalent public-facing array section. If it doesn't have one, locate the field types section and add a parallel "Array element constraints" subsection in user voice (less internal jargon, more "how do I express this in my project"). The Jekyll front matter (title/parent/nav_order) of the existing file stays unchanged.

- [ ] **Step 3: Update OpenAPI doc**

In `docs/openapi.md` (or wherever the public OpenAPI page lives), add a sentence:

> Array fields now emit element schemas reflecting `items.type`, `items.min`/`max`, `items.values`, and `items.format`. Previously, array `items` were rendered as an empty schema.

In `agent_docs/codegen-patterns.md`, mirror with the implementation note that `openapi.rs::FieldType::Array` consults `field.items` to build the element schema.

- [ ] **Step 4: Update `CHANGELOG.md` `[Unreleased]`**

Find the `[Unreleased]` section. Add:

```markdown
### Added
- **Element-level constraints on array `items:`.** `items:` now accepts a constraint map (`{ type: string, min: 3, max: 3 }`, `{ type: enum, values: [...] }`, `{ type: uuid, ref: organizations.id }`) in addition to the bare-name shorthand. Element validation runs per element with `<field>[<index>]` error paths; `items.ref` performs a runtime existence check on Postgres. ([#PR_NUMBER])

### Fixed
- **Migration numbering.** `shaperail migrate` now uses `max(numeric_prefix) + 1` instead of `count + 1`, so new migrations no longer collide with hand-written invariants migrations sitting past the highest auto-generated `_create_*` file. ([#PR_NUMBER])
```

Replace `#PR_NUMBER` with the actual PR number after pushing.

- [ ] **Step 5: Confirm no `CLAUDE.md` change needed**

The convention layer is unchanged. Skip this file.

---

## Task 12: Commit Issue I work and push

- [ ] **Step 1: Stage all Issue I changes (everything except migrate.rs)**

```bash
git status
git add shaperail-core/src/schema.rs shaperail-core/src/lib.rs \
        shaperail-core/Cargo.toml \
        shaperail-codegen/src/validator.rs shaperail-codegen/src/diagnostics.rs \
        shaperail-codegen/src/rust.rs shaperail-codegen/src/openapi.rs \
        shaperail-runtime/src/db/query.rs \
        shaperail-runtime/src/handlers/validate.rs \
        shaperail-runtime/src/handlers/crud.rs \
        shaperail-runtime/tests/db_integration.rs \
        agent_docs/resource-format.md docs/resource-guide.md \
        agent_docs/codegen-patterns.md docs/openapi.md \
        CHANGELOG.md
```

(Adjust the file list if Task 9 was skipped or other paths changed.)

- [ ] **Step 2: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat(shaperail-codegen): element-level constraints on array items

`items:` now accepts a constraint map alongside the bare-name
shorthand. Element validation runs per element in
check_field_rules with <field>[<index>] error paths; items.ref
performs a runtime existence check on Postgres before INSERT.
Validator rejects nested arrays, items.format on non-string,
items.ref on non-uuid, and malformed ref shapes. OpenAPI now
emits real element schemas instead of {}.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 3: Push the branch**

```bash
git push -u origin fix/items-spec-and-migration-numbering
```

- [ ] **Step 4: Open the PR**

```bash
gh pr create --title "fix: array items constraint maps + max-prefix migration numbering" --body "$(cat <<'EOF'
## Summary
- `feat(shaperail-codegen): element-level constraints on array items` — `items:` accepts both the bare-name shorthand and a constraint map. Element validation runs per element; `items.ref` performs a runtime existence check on Postgres.
- `fix(shaperail-cli): use max prefix for migration numbering` — `shaperail migrate` now uses `max(numeric_prefix) + 1`, ending collisions with hand-written invariants migrations.

Spec: `docs/superpowers/specs/2026-05-03-fix-array-items-and-migration-numbering-design.md`.

## Test plan
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] Postgres integration: `items.ref` happy path + missing-id rejection
- [ ] CLI migrate: gap pattern (`0008/0009/0010_handwritten`) yields `0011`
- [ ] OpenAPI emission: element constraints visible in generated spec

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-Review Notes

Spec coverage walk-through, before handoff:

1. **Goals (spec §Goals):** ✅ all five mapped to tasks.
2. **Non-goals (spec §Non-goals):** Task 5's validator covers nested-array rejection. No CHECK constraints emitted. No JSON-storage emulation. Bare-string shorthand preserved (Task 2 visitor). Proto unchanged (no task touches `proto.rs`).
3. **Schema model (spec §Schema model):** Task 2.
4. **Dual-form parsing (spec §Dual-form parsing):** Task 2 (visitor handles both forms).
5. **Validator rules (spec §Validator rules):** Task 5 implements all five rules listed in the spec table.
6. **Runtime validation (spec §Runtime validation):** Task 6.
7. **`items.ref` existence check (spec §items.ref existence check):** Task 7.
8. **Codegen impacts (spec §Codegen impacts):** rust.rs (Task 4), runtime/db/query.rs (Task 3), openapi.rs (Task 8), json_schema.rs (Task 9 — conditional), typescript.rs (covered by Task 4's `query_type` change since typescript.rs reads through it), proto.rs (unchanged per spec).
9. **Migration numbering fix (spec §Migration numbering fix):** Task 1.
10. **Testing (spec §Testing):** all six rows of the test table covered across Tasks 1, 2, 5, 6, 7, 8.
11. **Docs (spec §Docs & changelog):** Task 11.
12. **Commit plan (spec §Conventional-commit plan):** Task 1 commits `fix(shaperail-cli): ...`; Task 12 commits `feat(shaperail-codegen): ...`. Single PR (Task 12 Step 4).

The non-Postgres backend gate from the spec is intentionally a no-op at runtime today (the `PgPool` in `crud.rs` makes it trivially Postgres-only). The validator-side check is omitted by design — the spec's revision noted this.
