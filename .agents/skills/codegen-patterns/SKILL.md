---
name: codegen-patterns
description: SteelAPI code generation patterns. Auto-loaded when working on steel-codegen or generated/ output files.
---

## Generator Input → Output
Input: `ResourceDefinition` from steel-core
Output: Rust files written to `generated/src/<resource>/`

## Output Files Per Resource
```
generated/src/<resource>/
├── mod.rs        — re-exports
├── model.rs      — structs + enums
├── handlers.rs   — Actix-web handlers
├── queries.rs    — sqlx query functions
└── routes.rs     — route registration
```

## Critical Rules
- Always use `sqlx::query_as!` macro — never `query()` directly
- Never emit `.unwrap()` or `.expect()`
- Model structs must derive: `Debug, Clone, Serialize, Deserialize, sqlx::FromRow`
- Input structs must derive: `Debug, Deserialize, Validate`
- List response always: `{ "data": [...], "meta": { "cursor", "has_more" } }`
- Route paths match PRD format: `/users`, `/users/:id`
- Handler names: `list_<resource>`, `get_<resource>`, `create_<resource>` etc.

## Determinism Rule
Same ResourceDefinition must always produce byte-identical output.
Use `indexmap` for ordered iteration, never HashMap.

Full patterns: agent_docs/codegen-patterns.md
