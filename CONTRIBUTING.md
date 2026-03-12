# Contributing to Shaperail

Thank you for your interest in contributing to Shaperail.

## Getting Started

```bash
git clone https://github.com/muhammadmahindar/shaperail.git
cd shaperail
docker compose up -d   # start Postgres + Redis
cargo build --workspace
cargo test --workspace
```

## Development Workflow

1. Fork the repo and create a branch from `main`
2. Make your changes
3. Ensure all checks pass:
   ```bash
   cargo fmt
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   ```
4. Submit a pull request

## Quality Gate

Every PR must pass:

- `cargo fmt --check` — formatting
- `cargo clippy --workspace -- -D warnings` — linting with zero warnings
- `cargo test --workspace` — all tests pass

## Project Structure

| Crate | What goes here |
|-------|---------------|
| `shaperail-core` | Shared types (`FieldType`, `ResourceDefinition`, `ShaperailError`) |
| `shaperail-codegen` | YAML parsing, validation, OpenAPI/SDK generation |
| `shaperail-runtime` | Actix-web server, DB, cache, auth, jobs, events, storage |
| `shaperail-cli` | CLI commands (`shaperail init`, `shaperail serve`, etc.) |

## Design Rules

These are non-negotiable. PRs that violate them will be rejected:

1. **One Way** — No aliases, no alternative syntax, no shortcuts
2. **Explicit Over Implicit** — Nothing executes unless declared in the resource file
3. **Flat Abstraction** — Resource (layer 1) maps to runtime (layer 2). Max depth: 2
4. **Schema Is Source of Truth** — Generate code from schema, never reverse
5. **Compiler as Safety Net** — Every generated Rust file must compile and pass clippy

## Code Style

- No `.unwrap()` or `.expect()` in non-test code
- All public types have `///` doc comments
- Use `thiserror` for error types
- Use `sqlx::query_as!` macro — no raw `query()` calls
- Errors must be human-readable, not raw serde/parse errors

## Testing

- Unit tests in each module
- Integration tests via `sqlx::test` for database code
- Snapshot tests via `insta` for parser/codegen output
- `assert_cmd` for CLI commands

## Commit Messages

Format: `feat(crate-name): description`

Examples:
- `feat(shaperail-core): add ChannelDefinition type`
- `fix(shaperail-runtime): handle null fields in response serialization`
- `docs: update README with WebSocket examples`

## License

By contributing, you agree that your contributions will be dual-licensed under MIT and Apache-2.0.
