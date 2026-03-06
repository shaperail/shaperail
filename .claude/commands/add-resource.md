Add a new SteelAPI resource named: $ARGUMENTS

Before starting, read:
- agent_docs/resource-format.md   (YAML schema reference)
- agent_docs/codegen-patterns.md  (Rust output patterns)
- agent_docs/architecture.md      (where files go)

Steps:
1. Create `resources/$ARGUMENTS.yaml` using the full resource format (schema, endpoints, auth, cache, events)
2. Generate `steel-runtime/src/generated/$ARGUMENTS/`:
   - mod.rs, model.rs, handlers.rs, queries.rs, validation.rs, routes.rs
3. Register the new resource routes in `steel-runtime/src/app.rs`
4. Generate a sqlx migration in `migrations/` for the new table
5. Create stub hook files in `steel-runtime/src/hooks/$ARGUMENTS/` for any declared hooks
6. Write integration tests in `steel-runtime/tests/test_$ARGUMENTS.rs` covering:
   - list (200, empty, pagination)
   - get (200, 404)
   - create (201, 422 validation failure, 401 auth failure)
   - update (200, 404, 403 forbidden)
   - delete (200, 404)
7. Run `cargo build` — fix ALL errors before proceeding
8. Run `cargo clippy -- -D warnings` — fix ALL warnings
9. Run `cargo test --workspace` — fix ALL test failures

Do not stop until all three commands pass cleanly.
