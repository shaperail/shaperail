---
name: rust-conventions
description: Shaperail Rust conventions. Auto-loaded when writing or editing Rust files.
---

## Non-negotiable rules

- Do not use `.unwrap()` or `.expect()` in production paths. Propagate errors
  with `?` or handle them explicitly.
- Use `shaperail_core::ShaperailError` at framework and controller boundaries.
- Never hard-code credentials or secrets; load them from configuration or the
  environment.
- Generated resource SQL uses `sqlx::query_as!` with bind parameters. Generic
  runtime infrastructure may use sqlx's dynamic APIs when the schema is only
  known at runtime.
- Preserve type safety. Do not use unchecked casts to bypass compiler errors.

## Controllers

```rust
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

pub async fn normalize_email(ctx: &mut Context) -> ControllerResult {
    if let Some(email) = ctx.input.get_mut("email") {
        if let Some(value) = email.as_str() {
            *email = serde_json::json!(value.trim().to_lowercase());
        }
    }
    Ok(())
}
```

Use `AuthenticatedUser.sub`; it is an opaque JWT subject, not automatically a
database user ID. Verify an application row before writing it to a foreign key.

Run `cargo fmt` after edits and require
`cargo clippy --workspace --all-targets -- -D warnings` before completion.
