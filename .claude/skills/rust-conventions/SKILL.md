---
name: rust-conventions
description: SteelAPI Rust coding conventions. Auto-loaded when writing or editing any .rs file.
---

## Non-Negotiable Rules

**No .unwrap() or .expect() outside tests** — use `?` or match
**No raw query() calls** — always use `sqlx::query_as!` macro
**No hardcoded secrets** — always from env vars
**Error type** — always `SteelError` from steel-core, never String or Box<dyn Error>

## Required Derives
- Model structs: `Debug, Clone, Serialize, Deserialize, sqlx::FromRow`
- Input structs: `Debug, Deserialize, Validate`
- Enums: `Debug, Clone, Serialize, Deserialize, sqlx::Type`

## Naming
- Handlers: `list_users`, `get_user`, `create_user`, `update_user`, `delete_user`
- Queries: `find_by_id`, `find_all`, `insert`, `update_by_id`, `delete_by_id`
- Types: PascalCase — `CreateUserInput`, `UserRole`, `ListResponse<T>`

## Clippy
Must pass `cargo clippy -- -D warnings`. No `#[allow(...)]` without comment.
