---
title: CLI reference
parent: Reference
nav_order: 1
---

The `shaperail` binary is the operational interface to the framework. Most
projects follow the same loop: scaffold, edit resources, validate, migrate,
serve, then package.

## All commands

| Command | Description |
| --- | --- |
| `shaperail init <name>` | Scaffold a new project (resources, migrations, config, Docker Compose, .env). |
| `shaperail generate` | Run codegen for all resource files; write generated Rust and artifacts to `generated/`. |
| `shaperail serve [--port PORT] [--check] [--workspace]` | Start the dev server (with hot reload via cargo-watch). Use `--check` to validate without starting. Use `--workspace` to start all services declared in `shaperail.workspace.yaml`. |
| `shaperail build [--docker]` | Build release binary. With `--docker`, build a scratch-based Docker image. |
| `shaperail validate [path]` | Validate resource file(s). Default path: `resources`. |
| `shaperail test [-- args...]` | Run generated and custom tests (`cargo test` with optional args). |
| `shaperail migrate [--rollback]` | Generate missing initial create-table migrations and apply SQL migrations through `sqlx-cli`. `--rollback` reverts the last applied batch. |
| `shaperail seed [path]` | Load fixture YAML from `seeds/` (or given path) into the database. Default path: `seeds`. |
| `shaperail export openapi [--output FILE]` | Emit OpenAPI 3.1 spec to stdout or to a file. Array fields now emit element schemas reflecting `items.type`, `items.min`/`max`, `items.values`, and `items.format`. Previously, array `items` were rendered as an empty schema. |
| `shaperail export sdk --lang <lang> [--output DIR]` | Generate client SDK (e.g. `--lang ts` for TypeScript). |
| `shaperail export json-schema [--output FILE]` | Emit JSON Schema for resource YAML files (for IDE/LLM validation). |
| `shaperail explain <file> [--format <text\|json>]` | Dry-run: show what a resource YAML file will produce (routes, table, relations, validations, OpenAPI fragments). |
| `shaperail check [path] [--json]` | Validate with structured fix suggestions and error codes. `--json` for LLM-friendly output. |
| `shaperail diff` | Show what codegen would change without writing files (dry-run diff). |
| `shaperail doctor` | Check system deps: Rust, PostgreSQL, Redis, sqlx-cli; print fix instructions. |
| `shaperail routes` | Print all routes with auth requirements. |
| `shaperail jobs:status [job_id]` | Show job queue depth and recent failures; or inspect a specific job by ID. |
| `shaperail resource create <name> [--archetype TYPE]` | Scaffold a new resource YAML file and initial migration. Archetypes: basic (default), user, content, tenant, lookup. |

Every command supports `--help`.

> **LLM tip:** Use `shaperail explain <file> --format json` to get a machine-readable
> summary of any resource — routes, validations, and OpenAPI fragments — without
> parsing human text output. The JSON shape is stable across patch releases.

## Core command loop

| Command | When to use it | What it changes |
| --- | --- | --- |
| `shaperail init <name>` | Start a new app | Creates the scaffold, sample resource, migration, env file, and Docker Compose setup |
| `shaperail validate <file>` | Check one resource while editing | Validates schema and endpoint semantics without starting the app |
| `shaperail validate` | Check the whole project | Validates every resource in the project |
| `shaperail routes` | Review generated route surface | Prints routes with auth requirements |
| `shaperail export openapi --output openapi.json` | Review or publish the contract | Writes the deterministic OpenAPI 3.1 spec |
| `shaperail migrate` | New resources or unapplied SQL files exist | Generates missing initial `create_<resource>` migrations, then applies all migrations |
| `shaperail seed` | Populate dev data | Loads YAML fixtures from `seeds/` into the database in a transaction |
| `shaperail jobs:status [job_id]` | Check background work | Shows queue summary by default or inspects a specific job |
| `shaperail serve` | Run locally | Applies existing migrations and starts the development server |
| `shaperail serve --check` | Smoke test a scaffolded app | Verifies the generated app compiles and the config is coherent |
| `shaperail build --docker` | Package a deployable image | Builds the user app as a scratch-based Docker image |

## Command groups

### Project setup

```bash
shaperail doctor
shaperail init my-app
```

Use `doctor` before the first install or if a teammate reports environment
issues. Use `init` to create a project that is ready for Docker-first local
development.

### Resource authoring

```bash
shaperail resource create comments
shaperail validate resources/comments.yaml
```

Use `resource create` to scaffold a valid starting point, then edit the YAML
to add fields, endpoints, and relations.

### Validation and inspection

```bash
shaperail validate resources/posts.yaml
shaperail validate
shaperail routes
shaperail export openapi --output openapi.json
```

These commands should be part of the normal edit loop whenever resource files
change.

### Database workflow

```bash
shaperail migrate
shaperail migrate --rollback
shaperail seed
shaperail seed seeds/
```

`migrate` is currently best thought of as an initial-migration generator plus a
`sqlx migrate run` wrapper. It does not diff later schema edits into `ALTER
TABLE` SQL for you. `--rollback` reverts the last applied migration batch when
you need to back out a recent local change.

`seed` loads YAML fixture files from the `seeds/` directory into the database.
Each file maps to a table by filename (e.g., `seeds/users.yaml` inserts into
the `users` table). All inserts run in a single transaction — if any record
fails, everything rolls back. Reads `DATABASE_URL` from the environment or
`.env` file.

Seed file format:

```yaml
# seeds/users.yaml
- email: alice@example.com
  name: Alice
  role: admin
  org_id: "550e8400-e29b-41d4-a716-446655440000"
- email: bob@example.com
  name: Bob
  role: member
  org_id: "550e8400-e29b-41d4-a716-446655440000"
```

### Running and packaging

```bash
shaperail serve
shaperail serve --check
shaperail serve --workspace
shaperail build
shaperail build --docker
```

Use `serve` during development. Use `serve --check` in smoke tests and CI for a
cheap project-level validation. Use `serve --workspace` from a workspace root to
start all services declared in `shaperail.workspace.yaml` (see
[Multi-service workspaces]({{ '/multi-service/' | relative_url }})). Use
`build --docker` when you want the release image contract for a user app.

## Suggested daily workflow

```bash
docker compose up -d
shaperail validate
shaperail migrate
shaperail seed          # optional — load dev fixtures
shaperail serve
```

When the app is already running and you just want to inspect the contract:

```bash
shaperail routes
shaperail export openapi --output openapi.json
```

### Monitoring

```bash
shaperail jobs:status
shaperail jobs:status <job_id>
```

Connects to Redis and displays the current queue depth for each priority level
(critical, high, normal, low), the dead letter queue count, and recent
failures. If you pass a job ID, it prints the stored metadata for that job
instead of the summary view.

## explain

```bash
shaperail explain resources/posts.yaml
shaperail explain resources/posts.yaml --format json
```

Parses and validates a resource file, then prints a dry-run summary without
writing any files. Useful for verifying intent before running `generate` or
`migrate`.

The text output sections (in order): routes, table schema, relations,
validations, and compact OpenAPI fragments.

### --format \<text\|json\>

Default: `text`. Pass `--format json` to get a machine-readable JSON object
whose shape is stable across patch releases — field names are part of the CLI
contract. This lets an LLM or script consume `explain` output programmatically
without parsing the human-readable rendering.

**Top-level JSON fields:**

| Field | Type | Description |
|---|---|---|
| `resource` | string | Resource name |
| `version` | number | Schema version |
| `db` | string \| null | Named database connection |
| `tenant_key` | string \| null | Tenant isolation key |
| `routes` | array | One entry per endpoint with method, path, action, auth, filters, search, sort, pagination, cache\_ttl\_seconds, rate\_limit, soft\_delete, upload, controller, events, jobs |
| `table` | object | `{ name, columns[] }` — each column has name, type, nullable, primary\_key, unique, generated, references, default, sensitive |
| `relations` | array | Each entry: name, type, resource, key, foreign\_key |
| `validations` | object | Keyed by field name; value is an array of compact constraint strings (e.g. `["required", "min=1", "max=200"]`) |
| `openapi` | object | Keyed by action name; each entry has `request` (field list or null), `responses` (status → summary map), `auth` (roles array) |
| `indexes` | array | Each entry: fields, order, unique |

**Example:**

```bash
shaperail explain resources/incidents.yaml --format json | jq '.validations'
# {
#   "title": ["required", "min=1", "max=200"],
#   "severity": ["enum [sev1, sev2, sev3, sev4]", "default=\"sev3\""],
#   ...
# }
```

For a full OpenAPI 3.1 specification, use `shaperail export openapi` instead.

## Practical notes

- `shaperail migrate` currently relies on `sqlx-cli`.
- Later schema edits still require handwritten SQL migration files today.
- `shaperail seed` requires `DATABASE_URL` set in the environment or `.env`.
- `shaperail serve` uses the `.env` and config values in the current project.
- The scaffolded app already serves browser docs and the raw OpenAPI document.
- `build --docker` is aimed at the generated user app, not the framework repo.
