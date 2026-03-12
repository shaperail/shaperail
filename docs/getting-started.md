---
title: Getting started
parent: Guides
nav_order: 1
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
curl -fsSL https://shaperail.io/install.sh | sh
```

## Scaffold a project

```bash
shaperail init my-app
cd my-app
```

The scaffold includes:

- `resources/posts.yaml` as a starter resource
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

- `http://localhost:3000/docs`
- `http://localhost:3000/openapi.json`
- `http://localhost:3000/health`
- `http://localhost:3000/health/ready`

## Make your first change

Open `resources/posts.yaml` and change the schema or endpoint contract first.
Avoid editing generated Rust files by hand.

Useful commands while iterating:

```bash
shaperail validate resources/posts.yaml
shaperail routes
shaperail export openapi --output openapi.json
```

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

## Related pages

- [CLI reference]({{ '/cli-reference/' | relative_url }})
- [Resource guide]({{ '/resource-guide/' | relative_url }})
- [Blog API example]({{ '/blog-api-example/' | relative_url }})
