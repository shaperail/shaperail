---
title: Getting started
nav_order: 2
---

This guide gets you from zero to a running Shaperail service with browser docs,
OpenAPI output, health checks, and a working local Postgres plus Redis setup.

## What you need

Required:

- Rust 1.85+
- Docker with Compose support

Optional:

- `sqlx-cli` if you plan to create new migrations with `shaperail migrate`
- `psql` and `redis-cli` if you want to inspect services manually

Check your workstation with:

```bash
shaperail doctor
```

## Install the CLI

Cargo is the canonical install path:

```bash
cargo install shaperail-cli
```

If you prefer a release binary on macOS or Linux:

```bash
curl -fsSL https://shaperail.dev/install.sh | sh
```

## Scaffold a project

```bash
shaperail init my-app
cd my-app
```

The scaffold includes:

- `resources/posts.yaml` as a starter resource
- `controllers/` directory for business logic (see [Controllers]({{ '/controllers/' | relative_url }}))
- `migrations/` with an initial SQL migration
- `docker-compose.yml` for Postgres and Redis
- `.env` wired to that compose file
- a generated app shell that serves docs, health checks, and metrics

## Boot local services

```bash
docker compose up -d
```

This is the intended local path. A Shaperail user should not have to create a
database manually before the first run.

## Run the app

```bash
shaperail serve
```

Verify the generated surfaces:

- `http://localhost:3000/docs` — interactive API docs
- `http://localhost:3000/openapi.json` — OpenAPI 3.1 spec
- `http://localhost:3000/health` — liveness check
- `http://localhost:3000/health/ready` — readiness (DB + Redis)
- `http://localhost:3000/v1/posts` — your first versioned API endpoint

## Add a new resource

Use the `resource create` command to scaffold a valid YAML file and migration:

```bash
shaperail resource create comments
```

The `--archetype` flag scaffolds a resource pre-filled with common fields and
endpoints for a specific use case:

```
Available archetypes: basic (default), user, content, tenant, lookup
Example: shaperail resource create blog_posts --archetype content
```

Then edit `resources/comments.yaml` to add your fields and run:

```bash
shaperail validate
shaperail migrate
shaperail serve
```

Avoid editing files in `generated/` by hand — they are overwritten on every
`shaperail generate` and `shaperail serve`.

## Make changes to an existing resource

Open `resources/posts.yaml` and change the schema or endpoint contract first.

Useful commands while iterating:

```bash
shaperail validate resources/posts.yaml
shaperail routes
shaperail export openapi --output openapi.json
```

If you are changing an existing table rather than adding a brand-new resource,
remember that follow-up migration SQL is still manual today. `shaperail
migrate` only generates missing initial create-table migrations automatically.

## Load seed data

If you have fixture files in a `seeds/` directory, load them after migration:

```bash
shaperail seed
```

Each YAML file maps to a table by filename (e.g., `seeds/users.yaml` inserts
into `users`). All inserts run in a single transaction.

## When the schema changes

If you add or remove fields, create a migration and then run the app again:

```bash
shaperail migrate
shaperail serve
```

`shaperail serve` applies the SQL files that already exist in `migrations/`.

## Troubleshooting

| Problem | What to check |
| --- | --- |
| App cannot connect to Postgres or Redis | Run `docker compose ps` and confirm both services are healthy |
| Port `3000`, `5432`, or `6379` is busy | Change the host-side port in `docker-compose.yml` and update `.env` to match |
| `shaperail migrate` fails | Install `sqlx-cli` and confirm `DATABASE_URL` points at the same local Postgres service |
| Docs page loads but API calls fail | Confirm `.env`, Docker ports, and `DATABASE_URL` are aligned |

## Optional features

The scaffolded project includes only the Tier 1 stack: REST + Postgres + Redis.
Advanced capabilities are available as Cargo feature flags on `shaperail-runtime`:

| Feature | What it adds |
| --- | --- |
| `graphql` | GraphQL endpoint via async-graphql |
| `grpc` | gRPC server via tonic |
| `wasm-plugins` | WASM controller hooks via wasmtime |
| `multi-db` | Named multi-database runtime support; scaffolded apps wire SQL engines automatically |
| `observability-otlp` | OpenTelemetry OTLP span export |

Enable them in your `Cargo.toml`:

```toml
shaperail-runtime = { version = "0.7.0", default-features = false, features = ["graphql"] }
```

See [GraphQL]({{ '/graphql/' | relative_url }}), [gRPC]({{ '/grpc/' | relative_url }}),
and [Troubleshooting]({{ '/troubleshooting/' | relative_url }}) for details.

## Related pages

- [CLI reference]({{ '/cli-reference/' | relative_url }})
- [Resource guide]({{ '/resource-guide/' | relative_url }})
- [Blog API example]({{ '/blog-api-example/' | relative_url }})
- [Troubleshooting]({{ '/troubleshooting/' | relative_url }})
