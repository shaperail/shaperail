---
name: codegen-patterns
description: Shaperail code-generation patterns. Auto-loaded for shaperail-codegen and generated/ files.
---

## Input and output

Input: validated `shaperail_core::ResourceDefinition` values.

`shaperail generate` writes:

```text
generated/
├── mod.rs
└── <resource>.rs
```

Each resource file contains its record model and a typed `ResourceStore`
implementation. Controller source remains user-owned at
`resources/<resource>.controller.rs`; generation creates a stub but never
overwrites an existing controller.

## Critical rules

- Never hand-edit `generated/`; change the resource schema or generator.
- Generated database operations use `sqlx::query_as!` and bind parameters.
- Never emit `.unwrap()` or `.expect()`.
- Sensitive fields use `#[serde(skip_serializing)]`.
- Standard CRUD routes follow the resource convention; custom paths come from
  the validated schema.
- The runtime owns HTTP handlers and response envelopes. Generated files provide
  typed storage, not a parallel handler/service architecture.

## Determinism

The same ordered resource definitions must produce byte-identical output.
Preserve schema order and use deterministic collections for emitted artifacts.

Full patterns: `agent_docs/codegen-patterns.md`.
