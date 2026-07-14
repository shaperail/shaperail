---
name: resource-format
description: Shaperail resource file format. Auto-loaded when editing resources/*.yaml or the Shaperail YAML parser.
---

## Canonical resource format

Use one spelling for every concept. The top-level key is `resource:`, files use
the `.yaml` extension, and standard CRUD endpoints rely on their canonical
method/path defaults.

```yaml
resource: users
version: 1

schema:
  id: { type: uuid, primary: true, generated: true }
  email: { type: string, format: email, required: true, unique: true, sensitive: true }
  role: { type: enum, values: [admin, member, viewer], default: member }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }

endpoints:
  list:
    auth: [member, admin]
    pagination: cursor

  create:
    auth: [admin]
    input: [email, role]
    controller: { before: normalize_email }

  publish:
    method: POST
    path: /users/:id/publish
    auth: [admin]
```

- Standard names `list`, `get`, `create`, `update`, and `delete` infer their
  canonical method and path. Custom endpoint names require both.
- Field types: `uuid`, `string`, `integer`, `number`, `boolean`, `timestamp`,
  `date`, `enum`, `json`, `array`, `file`.
- `integer` is 64-bit (`BIGINT`/`i64`). The removed `bigint` and `float`
  spellings are invalid.
- Auth is `public`, `owner`, or an array such as `[member, admin]`.
- Controllers live in `resources/<resource>.controller.rs` and use
  `controller: { before: fn_name, after: fn_name }`.
- Keep server-owned fields out of endpoint `input`; populate them in a
  before-controller.

Full specification: `agent_docs/resource-format.md`.
