# shaperail-cli

The developer-facing CLI for [Shaperail](https://shaperail.io).

## Install

```bash
cargo install shaperail-cli
```

This installs the `shaperail` binary.

## Commands

```
shaperail init <name>          Scaffold a new Shaperail project
shaperail generate             Generate Rust code from resource YAML files
shaperail serve                Start dev server with hot reload
shaperail build                Build release binary
shaperail build --docker       Build scratch-based Docker image
shaperail validate             Validate all resource files
shaperail test                 Run generated + custom tests
shaperail migrate              Generate missing initial migrations and apply SQL files
shaperail migrate --rollback   Rollback last migration batch
shaperail seed                 Load fixture YAML files into database
shaperail export openapi       Export OpenAPI 3.1 spec
shaperail export sdk --lang ts Generate TypeScript client SDK
shaperail export json-schema   Export the resource YAML JSON Schema
shaperail check --json         Validate with structured diagnostics
shaperail explain <file>       Inspect resolved resource behavior
shaperail diff                 Preview generated changes
shaperail llm-context --json   Dump project-aware context for an LLM
shaperail resource create      Scaffold a canonical resource
shaperail doctor               Check system dependencies
shaperail routes               Print all routes with auth requirements
shaperail jobs:status          Show job queue depth and recent failures
```

Project commands automatically load `.env` from the current directory.
Variables already set in the shell take precedence.

## Quick Start

```bash
shaperail init my-app
cd my-app
docker compose up -d
shaperail generate
shaperail migrate
shaperail serve
```

`shaperail migrate` currently generates missing initial `create_<resource>`
migrations and then applies SQL files through `sqlx-cli`. Later schema changes
still require handwritten migration SQL.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE).
