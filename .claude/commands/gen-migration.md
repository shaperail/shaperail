Generate a sqlx migration for: $ARGUMENTS

Steps:
1. Read the resource file at `resources/$ARGUMENTS.yaml`
2. Read existing migrations in `migrations/` to understand current schema state
3. Determine what changed (new table, new column, dropped column, index change, etc.)
4. Generate two files:
   - `migrations/<timestamp>_<description>.up.sql`   — forward migration
   - `migrations/<timestamp>_<description>.down.sql` — rollback migration
5. Rules for SQL generation:
   - Always use `IF NOT EXISTS` / `IF EXISTS` for safety
   - Always add indexes for fields marked `index: true` or used as foreign keys
   - Soft delete table must have index on `deleted_at`
   - Enum types: create `CREATE TYPE ... AS ENUM` before the table that uses it
   - Timestamp fields default: `DEFAULT NOW()`
   - UUID primary keys: `DEFAULT gen_random_uuid()`
6. Verify migration syntax with `sqlx migrate info` (dry run)
7. If verification passes, report the generated file paths and content.
