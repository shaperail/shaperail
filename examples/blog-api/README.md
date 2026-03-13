# Blog API Example

This example shows the files a Shaperail user actually authors:

- `resources/*.yaml`
- `migrations/*.sql`
- `shaperail.config.yaml`
- `.env`
- `docker-compose.yml`

Use it with a normal scaffolded app.

## Quick Start

```bash
shaperail init blog-api
cd blog-api
```

Then copy these files into your app:

- `examples/blog-api/shaperail.config.yaml`
- `examples/blog-api/docker-compose.yml`
- `examples/blog-api/.env.example` as `.env`
- `examples/blog-api/resources/*.yaml`
- `examples/blog-api/migrations/*.sql`

After that:

```bash
docker compose up -d
shaperail serve
```

Open:

- `http://localhost:3000/docs`
- `http://localhost:3000/openapi.json`

## What This Example Covers

- public blog post reads
- protected post creation and updates
- owner-based post and comment updates through `created_by`
- post/comment relations
- cursor pagination on posts
- offset pagination on comments
- soft delete on posts
- single-database config (`database:`); for multi-DB use `databases:` in config and optional `db:` on resources (see [Configuration reference](https://shaperail.dev/configuration/#databases-multi-database))

## Files

- [resources/posts.yaml](./resources/posts.yaml)
- [resources/comments.yaml](./resources/comments.yaml)
- [migrations/0001_create_posts.sql](./migrations/0001_create_posts.sql)
- [migrations/0002_create_comments.sql](./migrations/0002_create_comments.sql)
- [requests.http](./requests.http)

## Notes

- `owner` auth works by comparing the token user ID to `created_by`
- this example keeps reads public and requires auth only for writes
- the app still uses the standard Rust scaffold created by `shaperail init`
- resources omit `db:` so they use the default connection; with `databases:` in config you can set `db: <name>` per resource
