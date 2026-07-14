Review the Shaperail resource file `$ARGUMENTS`.

Read `agent_docs/resource-format.md`, then run:

```bash
shaperail check "$ARGUMENTS" --json
shaperail explain "$ARGUMENTS"
```

In addition to every CLI diagnostic, verify:

- `resource`, `version`, and `schema` use canonical names.
- Standard CRUD endpoints omit redundant method/path; custom endpoint names
  declare both.
- Write `input` lists expose only client-writable fields.
- Server-owned and authenticated-subject fields are populated by a
  before-controller, not trusted from request input.
- `integer` is used for 64-bit integers and `number` for numeric values; removed
  `bigint` and `float` spellings are rejected.
- References use `ref: resources.id`, relations name valid schema fields, and
  frequently filtered/reference fields have indexes.
- Controller names exist in `resources/<resource>.controller.rs`.

Report findings by severity with exact field paths and concrete fixes.
