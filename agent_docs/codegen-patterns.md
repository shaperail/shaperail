# Shaperail Code Generation Patterns

## Contract

The Rust generator receives validated
`shaperail_core::ResourceDefinition` values and writes deterministic modules to
the application-level `generated/` directory:

```text
generated/
├── mod.rs
├── users.rs
└── organizations.rs
```

`shaperail generate` is the only writer for these files. Users change resource
YAML or controller source, never generated Rust.

## Resource module

Each `generated/<resource>.rs` contains:

1. A serializable record struct containing persisted schema fields.
2. A `<Resource>Store` holding a `sqlx::PgPool`.
3. Typed collection-query helpers for declared list-like endpoints.
4. A `shaperail_runtime::db::ResourceStore` implementation for find, list,
   insert, update, and delete operations.

The runtime owns HTTP extraction, validation, authorization, response
envelopes, controller dispatch, events, and jobs. Do not generate a second
handler or service layer.

### Record pattern

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsersRecord {
    pub id: uuid::Uuid,
    #[serde(skip_serializing)]
    pub email: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

- Persisted fields appear in schema order.
- `sensitive: true` fields receive `#[serde(skip_serializing)]`.
- Optional fields use `Option<T>`.
- `integer` maps to `i64`; `number` maps to `f64`.
- Array element types map to typed `Vec<T>` values.

### Query pattern

Generated Postgres queries use `sqlx::query_as!` with bind parameters:

```rust
let row = sqlx::query_as!(
    UsersRecord,
    r#"
    SELECT
        "id" as "id!: uuid::Uuid",
        "email" as "email!: String",
        "created_at" as "created_at!: chrono::DateTime<chrono::Utc>"
    FROM "users"
    WHERE "id" = $1
    "#,
    id
)
.fetch_optional(&self.pool)
.await?
.ok_or(shaperail_core::ShaperailError::NotFound)?;
```

Rules:

- Never interpolate values into SQL.
- Quote schema-derived identifiers.
- Keep bind order and generated type annotations deterministic.
- Never emit `.unwrap()` or `.expect()`.
- Propagate sqlx failures through `ShaperailError`.
- Exclude `transient: true` fields from models and persistence.

## Registry module

`generated/mod.rs`:

- declares each generated resource module;
- builds the typed `StoreRegistry`;
- includes user-owned controller files with `#[path = "../resources/..."]`;
- registers declared controller functions;
- includes and registers declared job handlers;
- includes and registers custom endpoint handlers.

Controller files live at `resources/<resource>.controller.rs`. Job handlers live
under `jobs/`. Generation may create a missing stub but must never overwrite a
user-owned implementation.

## Determinism

The same validated resource list must produce byte-identical output.

- Preserve `IndexMap` declaration order from resource definitions.
- Use sorted or ordered collections whenever data does not already have schema
  order.
- Never include timestamps, absolute paths, random identifiers, or host state.
- Run rustfmt on emitted files, but do not make output depend on rustfmt being
  installed.

## Verification

Generator changes require:

1. Focused string/structure tests for the changed emission.
2. Snapshot updates when an existing snapshot intentionally changes.
3. A generated-project compile test.
4. `cargo clippy --workspace --all-targets -- -D warnings`.

Valid resource definitions must never produce Rust that fails to compile.
