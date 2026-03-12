# Getting Started

## Prerequisites

Required:

- Rust 1.85+
- Docker with Compose support

Optional:

- `sqlx-cli` if you plan to run `shaperail migrate`
- `psql` and `redis-cli` for manual inspection

Check your machine with:

```bash
shaperail doctor
```

## Install

```bash
cargo install shaperail-cli
```

## Create Your First App

```bash
shaperail init my-app
cd my-app
```

The scaffold gives you:

- a working app shell
- a sample `posts` resource
- a starter migration
- `.env` with local defaults
- `docker-compose.yml` for Postgres and Redis

## Start Local Services

```bash
docker compose up -d
```

This is the default local development path. You should not need to create a
database manually. The generated compose file sets `POSTGRES_DB`,
`POSTGRES_USER`, and `POSTGRES_PASSWORD` so the app and database match out of
the box.

## Start The App

```bash
shaperail serve
```

Once the app boots, open:

- `http://localhost:3000/docs`
- `http://localhost:3000/openapi.json`
- `http://localhost:3000/health`

## Edit The Resource

The scaffold includes `resources/posts.yaml`. Change that file first instead of
editing generated Rust code.

Useful commands:

```bash
shaperail validate resources/posts.yaml
shaperail routes
shaperail export openapi --output openapi.json
```

## Change The Schema

When you add or remove fields:

```bash
shaperail migrate
shaperail serve
```

`shaperail serve` applies the SQL files already present in `migrations/`.

## First Things To Check If Something Fails

- If the app cannot connect to Postgres or Redis, run `docker compose ps`.
- If port `3000`, `5432`, or `6379` is already in use, change the host port in
  `docker-compose.yml` and update `.env` to match.
- If `shaperail migrate` fails, make sure `sqlx-cli` is installed.
