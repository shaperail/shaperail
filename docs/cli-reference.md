---
title: CLI reference
parent: Reference
nav_order: 1
---

The `shaperail` binary is the operational interface to the framework. Most
projects follow the same loop: scaffold, edit resources, validate, migrate,
serve, then package.

## Core command loop

| Command | When to use it | What it changes |
| --- | --- | --- |
| `shaperail init <name>` | Start a new app | Creates the scaffold, sample resource, migration, env file, and Docker Compose setup |
| `shaperail validate <file>` | Check one resource while editing | Validates schema and endpoint semantics without starting the app |
| `shaperail validate` | Check the whole project | Validates every resource in the project |
| `shaperail routes` | Review generated route surface | Prints routes with auth requirements |
| `shaperail export openapi --output openapi.json` | Review or publish the contract | Writes the deterministic OpenAPI 3.1 spec |
| `shaperail migrate` | Schema changed | Creates a new SQL migration based on current resource definitions |
| `shaperail seed` | Populate dev data | Loads YAML fixtures from `seeds/` into the database in a transaction |
| `shaperail jobs:status` | Check background work | Shows Redis queue depths and dead letter count |
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

### Authoring and validation

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

`migrate` creates new SQL files. `--rollback` reverts the last applied migration
batch when you need to back out a recent change locally.

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
shaperail build
shaperail build --docker
```

Use `serve` during development. Use `serve --check` in smoke tests and CI for a
cheap project-level validation. Use `build --docker` when you want the release
image contract for a user app.

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
```

Connects to Redis and displays the current queue depth for each priority level
(critical, high, normal, low) and the dead letter queue count.

## Practical notes

- `shaperail migrate` currently relies on `sqlx-cli`.
- `shaperail seed` requires `DATABASE_URL` set in the environment or `.env`.
- `shaperail serve` uses the `.env` and config values in the current project.
- The scaffolded app already serves browser docs and the raw OpenAPI document.
- `build --docker` is aimed at the generated user app, not the framework repo.
