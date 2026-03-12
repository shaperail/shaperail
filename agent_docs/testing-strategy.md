# Shaperail Testing Strategy

## Layer-by-Layer Rules

### shaperail-core (unit tests only)
- Test every FieldType variant parses correctly from YAML
- Test validation logic for every field constraint
- Test error formatting and conversion
- No DB, no HTTP, no async — pure functions only
- Location: `shaperail-core/src/` inline `#[cfg(test)]` modules

### shaperail-codegen (unit + snapshot tests)
- Test YAML → ResourceDefinition parsing for valid and invalid inputs
- Snapshot test generated Rust code using `insta` crate
  - If generated code changes, snapshot must be explicitly approved
- Test that invalid resource files produce correct error messages
- Do NOT test that generated code compiles here — that's shaperail-runtime's job
- Location: `shaperail-codegen/tests/`

### shaperail-runtime (integration tests — require running Postgres + Redis)
- Use `sqlx::test` macro — spins up isolated DB per test, auto-rollback
- Test every generated endpoint: happy path, auth failure, validation failure, not found
- Test cache invalidation: verify Redis key is deleted after write
- Test soft delete: verify deleted records don't appear in list
- Test pagination: cursor and offset, edge cases (empty page, last page)
- Location: `shaperail-runtime/tests/`

### shaperail-cli (end-to-end tests)
- Test `shaperail init` produces correct file structure
- Test `shaperail generate` produces files that compile (`cargo check`)
- Test `shaperail validate` catches invalid resource files
- Use `assert_cmd` crate for CLI testing
- Location: `shaperail-cli/tests/`

## Test Naming Convention
```rust
#[test]
fn test_<thing>_<condition>_<expected_outcome>() { ... }

// Examples:
fn test_field_type_uuid_parses_correctly() { ... }
fn test_list_endpoint_without_auth_returns_401() { ... }
fn test_soft_delete_hides_record_from_list() { ... }
fn test_codegen_emits_correct_handler_for_crud_resource() { ... }
```

## Test Data Pattern
```rust
// Always use builder pattern for test fixtures
fn user_fixture() -> CreateUserInput {
    CreateUserInput {
        email: "test@example.com".into(),
        name: "Test User".into(),
        role: None,
        org_id: Uuid::new_v4(),
    }
}
```

## What Must Always Be Tested Before Commit
1. The specific function/module you changed
2. Any hook that touches the changed resource
3. The endpoint that calls the changed function
4. Run: `cargo test --workspace` and `cargo clippy -- -D warnings`
