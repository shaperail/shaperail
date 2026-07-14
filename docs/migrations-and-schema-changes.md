---
title: Migrations and schema changes
parent: Guides
nav_order: 4
---

Shaperail treats resource YAML as the schema source of truth, but the database
still changes through SQL files in `migrations/`.

## Current behavior

`shaperail migrate` currently does two things:

1. Creates a missing initial `create_<resource>` migration for each resource
   that does not already have one.
2. Runs `sqlx migrate run --source migrations` to apply all unapplied SQL files.

Important limitation: it does **not** diff later schema edits and it does
**not** generate `ALTER TABLE`, `DROP COLUMN`, rename, or type-change
migrations from resource changes.

That means the first-table creation path is scaffolded, but follow-up schema
changes are manual SQL today.

## Starting state

`shaperail init` creates:

- a starter resource
- an initial SQL migration

So a new project should boot with:

```bash
docker compose up -d
shaperail serve
```

without writing SQL by hand first.

## Normal workflow

### First migration for a new resource

1. Add the resource YAML.
2. Validate it.
3. Run `shaperail migrate`.
4. Review the generated `create_<resource>` SQL.
5. Run the app.

```bash
shaperail validate resources/users.yaml
shaperail migrate
shaperail serve
```

If a matching `create_<resource>` migration already exists, `shaperail migrate`
prints a skip message for that resource instead of generating another file.

### Later schema changes

After the initial create migration exists, treat schema changes as a manual SQL
workflow:

1. Edit `resources/*.yaml`
2. Validate the YAML
3. Add a new `.sql` file in `migrations/`
4. Review and test the SQL
5. Apply it with `shaperail migrate` or `sqlx migrate run --source migrations`

## Important distinction

- `shaperail migrate` generates only missing initial create-table migrations
  automatically.
- `shaperail migrate` also applies existing SQL files through `sqlx-cli`.
- `shaperail serve` applies the SQL files already present in `migrations/`
  before starting the HTTP server.

## Manual migration examples

### Add a column

Resource change:

```yaml
schema:
  # ...existing fields...
  bio: { type: string, max: 2000 }
```

Manual SQL:

```sql
ALTER TABLE users ADD COLUMN bio TEXT;
```

### Add a required column to a table with existing rows

If existing rows already exist, add the column in two steps.

Step 1:

```sql
ALTER TABLE users ADD COLUMN status TEXT DEFAULT 'active';
UPDATE users SET status = 'active' WHERE status IS NULL;
```

Step 2:

```sql
ALTER TABLE users ALTER COLUMN status SET NOT NULL;
```

If the field is an enum in YAML, also add the matching SQL constraint or type
used by your schema.

### Rename a column

Shaperail does not auto-detect renames. Write the rename manually:

```sql
ALTER TABLE users RENAME COLUMN full_name TO display_name;
```

Update the resource YAML in the same change so codegen and SQL stay aligned.

### Change a column type

```sql
ALTER TABLE items
ALTER COLUMN score TYPE DOUBLE PRECISION
USING score::DOUBLE PRECISION;
```

Review casts carefully. If the cast can fail, backfill or clean the data first.

### Drop a column

```sql
ALTER TABLE users DROP COLUMN IF EXISTS legacy_field;
```

Check all resource relations, controllers, and downstream consumers before
dropping data.

### Add an index

```sql
CREATE INDEX idx_users_org_id_role ON users (org_id, role);
CREATE UNIQUE INDEX idx_users_email ON users (email);
```

For large tables, consider `CREATE INDEX CONCURRENTLY` and put it in its own
migration file.

## Testing migrations safely

### Throwaway database

Use a temporary Postgres instance and point `DATABASE_URL` at it:

```bash
docker run --rm -d --name pg-test -e POSTGRES_PASSWORD=test -p 5499:5432 postgres:16
export DATABASE_URL=postgresql://postgres:test@localhost:5499/postgres
shaperail migrate
docker stop pg-test
```

### Apply SQL directly with sqlx

If you want to test only the SQL application step:

```bash
sqlx migrate run --source migrations
```

### Validate SQL syntax manually

Run a migration inside a transaction in `psql`:

```bash
psql "$DATABASE_URL"
BEGIN;
\i migrations/0003_add_users_bio.sql
ROLLBACK;
```

## Rollback

### Revert the last applied batch

```bash
shaperail migrate --rollback
```

This calls `sqlx migrate revert --source migrations`.

### Manual rollback migration

If you need a more controlled rollback, write a reverse migration file:

```sql
-- migrations/0004_revert_add_bio.sql
ALTER TABLE users DROP COLUMN IF EXISTS bio;
```

## Safeguards

1. Review every generated or handwritten SQL file before committing it.
2. Do not assume resource edits automatically produce follow-up migration SQL.
3. Never edit or delete an applied migration file. SQLx stores its checksum and
   rejects changed history; add a new numbered migration instead.
4. Test destructive changes on a copy of real data first.
5. Keep the YAML and SQL changes in the same commit so schema drift is visible.
