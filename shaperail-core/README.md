# shaperail-core

Shared type definitions for the [Shaperail](https://github.com/muhammadmahindar/shaperail) framework.

This crate provides the foundational types that all other Shaperail crates depend on:

- **`ResourceDefinition`** — The parsed representation of a resource YAML file
- **`FieldType`** — All supported schema types (uuid, string, integer, enum, json, etc.)
- **`FieldSchema`** — Field definition with constraints (required, unique, min/max, etc.)
- **`EndpointSpec`** — Endpoint configuration (method, path, auth, cache, hooks, events)
- **`AuthRule`** — Authentication rules (Public, Roles, Owner)
- **`RelationSpec`** — Relationships between resources (belongs_to, has_many, has_one)
- **`ShaperailError`** — Standardized error type with HTTP status codes
- **`ProjectConfig`** — Parsed `shaperail.config.yaml` project configuration
- **`ChannelDefinition`** — WebSocket channel configuration

## Usage

This crate is used internally by `shaperail-codegen` and `shaperail-runtime`. You typically don't need to depend on it directly unless you're building custom tooling around Shaperail.

```toml
[dependencies]
shaperail-core = "0.2"
```

```rust
use shaperail_core::{ResourceDefinition, FieldType, ShaperailError};
```

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE).
