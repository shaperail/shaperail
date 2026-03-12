---
name: resource-format
description: SteelAPI resource file format. Auto-loaded when editing or creating .yaml files in resources/, or when implementing the YAML parser in steel-codegen.
---

## Resource File Quick Reference (exact PRD format)

Top-level key is `resource:` not `name:` — this is non-negotiable.

```yaml
resource: users    # ← "resource:", not "name:"
version: 1

schema:
  id: { type: uuid, primary: true, generated: true }
  email: { type: string, format: email, required: true, sensitive: true }
  role: { type: enum, values: [admin, member, viewer], default: member }

endpoints:
  list:
    method: GET       # ← method + path are required on every endpoint
    path: /users
    auth: [member, admin]
    pagination: cursor
  create:
    method: POST
    path: /users
    auth: [admin]
    input: [email, name]   # ← explicit input list
    hooks: [validate_org]
    events: [user.created]
    jobs: [send_welcome_email]
```

Field types: uuid, string, integer, bigint, number, boolean, timestamp, date, enum, json, array, file
Auth: `public` | `[role1, role2]` | `owner` | `[owner, admin]`
Pagination: `cursor` (default) | `offset`

Full spec: agent_docs/resource-format.md
