# Shaperail User Guide

Shaperail is meant to be used from the CLI outward:

```bash
cargo install shaperail-cli
shaperail init my-app
cd my-app
docker compose up -d
shaperail serve
```

The files in this `docs/` directory are the user-facing guide. They describe the
workflow for someone installing and using the framework. The files in
`agent_docs/` are maintainer docs for building the framework itself.

## Start Here

- [Getting Started](./getting-started.md)
- [Resource Guide](./resource-guide.md)
- [Auth and Ownership](./auth-and-ownership.md)
- [Migrations and Schema Changes](./migrations-and-schema-changes.md)
- [Docker Deployment](./docker-deployment.md)

## Example App

Use the example in [examples/blog-api](../examples/blog-api/README.md) after
you run `shaperail init blog-api`. It shows:

- two related resources: `posts` and `comments`
- public read endpoints and protected write endpoints
- `owner`-based updates via a `created_by` field
- cursor pagination on posts and offset pagination on comments
- a soft-deleted resource (`posts`)

## Recommended Learning Path

1. Follow [Getting Started](./getting-started.md) until you can open
   `http://localhost:3000/docs`.
2. Read [Resource Guide](./resource-guide.md) so you understand which files are
   the source of truth.
3. Use [examples/blog-api](../examples/blog-api/README.md) as your first real
   project shape.
4. When you change schema files, use
   [Migrations and Schema Changes](./migrations-and-schema-changes.md).

## What You Actually Write

In a normal Shaperail app, the user-authored files are:

- `resources/*.yaml`
- `migrations/*.sql`
- `shaperail.config.yaml`
- `.env`
- `docker-compose.yml`

The scaffolded Rust application exists to load those files, run the runtime,
and serve your API plus the generated docs.
