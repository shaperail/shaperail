# Migrations and Schema Changes

Shaperail treats your resource schema as the source of truth, but the running
database still changes through SQL files in `migrations/`.

## Initial State

`shaperail init` creates:

- a starter resource
- an initial SQL migration

That means a new project can be started with:

```bash
docker compose up -d
shaperail serve
```

without writing SQL by hand.

## When You Change A Resource

Typical workflow:

1. Edit `resources/*.yaml`
2. Validate the file
3. Generate a new migration
4. Review the SQL
5. Run the app

Commands:

```bash
shaperail validate resources/posts.yaml
shaperail migrate
shaperail serve
```

## Important Distinction

- `shaperail migrate` creates and applies SQL migration files
- `shaperail serve` applies the SQL files that already exist in `migrations/`

## Review The SQL

Do not treat generated SQL as invisible.

Before committing:

- check table names
- check `NOT NULL` constraints
- check enum `CHECK` constraints
- check foreign keys
- check indexes
- check whether a delete endpoint should be hard delete or soft delete

## Roll Back

If you need to revert the last applied migration batch:

```bash
shaperail migrate --rollback
```

## Current Tooling Note

Today, `shaperail migrate` relies on `sqlx-cli`, so install it if you plan to
use schema-driven migration generation:

```bash
cargo install sqlx-cli
```

## Example

The example app in [examples/blog-api](../examples/blog-api/README.md) includes
two checked-in migrations that match its resource files.
