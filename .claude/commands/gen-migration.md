Generate a sqlx migration for `$ARGUMENTS`.

1. Read the resource file and all existing files in `migrations/`.
2. Use the next numeric prefix above the highest existing prefix. Do not fill
   gaps or reuse a version.
   Never edit an existing migration: SQLx verifies applied-file checksums.
3. For a new resource with no initial migration, prefer
   `shaperail migrate`, which generates and applies the canonical
   `create_<resource>` migration.
4. For a schema change, write the required `ALTER TABLE`, index, constraint, or
   data-migration SQL explicitly. `shaperail migrate` does not infer diffs for
   an existing resource.
5. Keep the resource YAML as the source of truth and match Shaperail's type
   mappings (`integer` -> `BIGINT`, `number` -> `NUMERIC`).
6. Validate detection with `sqlx migrate info --source migrations`; apply only
   when the user requested it.

Never silently drop or rewrite data. Ask before a destructive or irreversible
migration.
