---
title: Migrations and schema changes
parent: Guides
nav_order: 4
---

Shaperail treats resource YAML as the schema source of truth, but the running
database still changes through SQL files in `migrations/`.

## Starting state

`shaperail init` creates:

- a starter resource
- an initial SQL migration

That means a new project should be able to boot with:

```bash
docker compose up -d
shaperail serve
```

without writing SQL by hand first.

## Workflow when a resource changes

1. Edit `resources/*.yaml`
2. Validate the resource file
3. Create a new migration
4. Review the generated SQL
5. Run the app

Commands:

```bash
shaperail validate resources/posts.yaml
shaperail migrate
shaperail serve
```

## Important distinction

- `shaperail migrate` creates new SQL migration files
- `shaperail serve` applies the SQL files already present in `migrations/`

## Review the SQL before commit

Generated SQL should not be treated as invisible build output. Check:

- table names
- `NOT NULL` constraints
- enum checks
- foreign keys
- indexes
- whether a delete route should be hard delete or soft delete

## Roll back a recent migration batch

```bash
shaperail migrate --rollback
```

Use this for local recovery if the latest migration batch needs to be reversed.

## Tooling note

Today, `shaperail migrate` relies on `sqlx-cli`:

```bash
cargo install sqlx-cli
```

## Example flow

The [Blog API example]({{ '/blog-api-example/' | relative_url }}) includes two checked-in
migrations that match its resource files, so you can inspect the schema-to-SQL
relationship directly.
