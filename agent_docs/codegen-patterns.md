# Shaperail Code Generation Patterns

## Principle
The generator converts ResourceDefinition → Rust source files.
Input: `shaperail-core::ResourceDefinition` (already validated)
Output: Rust modules written to `shaperail-runtime/src/generated/<resource>/`

## Output File Structure Per Resource
```
shaperail-runtime/src/generated/users/
├── mod.rs          # re-exports everything
├── model.rs        # serde struct, sqlx FromRow
├── handlers.rs     # Actix-web handler functions
├── queries.rs      # sqlx query functions
├── validation.rs   # input validation logic
└── routes.rs       # route registration
```

## model.rs Pattern
```rust
// Always derive these — no exceptions
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: UserRole,         // enum types get their own type
    pub org_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

// Input structs — always separate from the model
#[derive(Debug, Deserialize, Validate)]
pub struct CreateUserInput {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    pub role: Option<UserRole>,   // optional fields use Option<T>
    pub org_id: Uuid,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateUserInput {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,     // PATCH: all fields optional
    pub metadata: Option<Value>,
}
```

## queries.rs Pattern
```rust
// Always use sqlx query_as! macro for compile-time verification
// Never use raw string queries

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>, ShaperailError> {
    sqlx::query_as!(
        User,
        "SELECT * FROM users WHERE id = $1 AND deleted_at IS NULL",
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(ShaperailError::from)
}

pub async fn list(pool: &PgPool, params: &ListParams) -> Result<Vec<User>, ShaperailError> {
    // Cursor pagination — preferred over offset for large tables
    // Offset pagination only when explicitly declared in resource file
    sqlx::query_as!(
        User,
        r#"
        SELECT * FROM users
        WHERE deleted_at IS NULL
          AND ($1::uuid IS NULL OR id < $1)
        ORDER BY created_at DESC
        LIMIT $2
        "#,
        params.cursor,
        params.limit as i64,
    )
    .fetch_all(pool)
    .await
    .map_err(ShaperailError::from)
}
```

## handlers.rs Pattern
```rust
// Handler signature is always the same shape
pub async fn get_user(
    path: web::Path<Uuid>,
    state: web::Data<AppState>,
    auth: AuthenticatedUser,   // extractor — fails with 401 if not authenticated
) -> impl Responder {
    let id = path.into_inner();

    match queries::find_by_id(&state.db, id).await {
        Ok(Some(user)) => HttpResponse::Ok().json(user),
        Ok(None) => ShaperailError::NotFound("user".into()).into_response(),
        Err(e) => e.into_response(),
    }
}

// List handler always uses this response envelope
#[derive(Serialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub meta: PaginationMeta,
}
```

## Error Handling Rule
NEVER use `.unwrap()` or `.expect()` in generated code.
ALWAYS propagate with `?` or explicit `match`.
ALWAYS use `ShaperailError` variants — never raw `String` errors.

## Enum Pattern
```rust
// Resource enums always implement these traits
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Member,
    Viewer,
}
```

## Route Registration Pattern
```rust
// routes.rs — always follows this exact shape
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/users")
            .route("", web::get().to(handlers::list_users))
            .route("", web::post().to(handlers::create_user))
            .route("/{id}", web::get().to(handlers::get_user))
            .route("/{id}", web::patch().to(handlers::update_user))
            .route("/{id}", web::delete().to(handlers::delete_user))
    );
}
```

## What the Generator MUST NOT Do
- Generate code that uses `.unwrap()` or `.expect()`
- Generate `pub` fields on input structs that bypass validation
- Generate SQL with string interpolation (always use `$1`, `$2` placeholders)
- Generate code that imports from outside `shaperail-core` or `shaperail-runtime`
- Generate files that don't compile with `cargo clippy -- -D warnings`

## Field nullability and Option wrapping

The codegen emits `Option<T>` only for columns that the database actually
permits to be NULL. Columns backed by `NOT NULL` SQL — including
`primary: true`, `required: true`, `default:`, and `generated: true` —
emit the bare type `T`. `nullable: true` always wins over the other flags:
a column declared `nullable: true, default: "x"` is genuinely nullable
in SQL, so the struct field is `Option<T>`.

### The rule

```
field is Option<T> in the codegen struct
    iff
nullable = true
    OR
(NOT primary AND NOT required AND default is None AND generated = false)
```

`model_field_is_optional` and `field_is_required` in
`shaperail-codegen/src/rust.rs` are exact inverses of this predicate.

### Behavior matrix

| YAML | DB column | Codegen type |
|---|---|---|
| `{ type: integer, required: true }` | `BIGINT NOT NULL` | `i64` |
| `{ type: integer, required: true, default: 0 }` | `BIGINT NOT NULL DEFAULT 0` | `i64` |
| `{ type: integer, default: 0 }` | `BIGINT NOT NULL DEFAULT 0` | `i64` |
| `{ type: timestamp, generated: true }` | `TIMESTAMP NOT NULL DEFAULT NOW()` | `DateTime<Utc>` |
| `{ type: string, nullable: true }` | `VARCHAR NULL` | `Option<String>` |
| `{ type: string, nullable: true, default: "x" }` | `VARCHAR NULL DEFAULT 'x'` | `Option<String>` |
| `{ type: uuid, primary: true, generated: true }` | `UUID PRIMARY KEY DEFAULT gen_random_uuid()` | `Uuid` |

### Input-body invariant

Input payloads for `create` and `update` are not affected by this rule.
A field with `default:` is still optional in the request body — the caller
may omit it and the database fills the default. Only the SELECT-side
`pub struct` flips type, since that struct is constructed from a row that
the database has already populated.

## OpenAPI Generation

The OpenAPI generator walks each `ResourceDefinition` and emits an OpenAPI 3.1
schema object per field.

For `FieldType::Array`, the generator consults `field.items` to build the
element schema: `items.type` is mapped to OpenAPI primitives, and `format`,
`enum`, `minLength`/`maxLength` (string/enum), or `minimum`/`maximum`
(numeric) are emitted on the element schema when present. An `items` value that
is a bare type name (legacy shorthand) produces an element schema with only
`type` set. An `items` value that is a constraint map produces a fully
annotated element schema. Previously, array `items` were rendered as an empty
schema `{}`.
