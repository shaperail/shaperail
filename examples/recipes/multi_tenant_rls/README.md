# Recipe: Multi-Tenant Row-Level Security

## WHEN to use this

Use this recipe when you are building a **SaaS product where multiple organizations share the same database** and you need strong data isolation guarantees:
- Each tenant (organization) must only see its own rows.
- Cross-tenant access should be invisible — a tenant trying to read another tenant's document must get **404**, not 403 or an empty list.
- You want zero custom code for the isolation: no `WHERE org_id = ?` scattered across handlers, no middleware you maintain.

The `tenant_key:` field declaration makes Shaperail enforce this automatically.

## What this gives you

Two resources:

### organizations (admin-only CRUD)

```
GET    /v1/organizations            → list (admin)
GET    /v1/organizations/:id        → get (admin)
POST   /v1/organizations            → create (admin)
PATCH  /v1/organizations/:id        → update (admin)
DELETE /v1/organizations/:id        → delete (admin)
```

### documents (tenant-isolated CRUD)

```
GET    /v1/documents                → list (member, admin) — scoped to caller's org
GET    /v1/documents/:id            → get (member, admin)  — 404 if different org
POST   /v1/documents                → create (member, admin)
PATCH  /v1/documents/:id            → update (admin, owner)
DELETE /v1/documents/:id            → delete (admin)
```

## How tenant_key works

```yaml
resource: documents
tenant_key: org_id
```

With this declaration the runtime:
1. **Injects** `org_id` from the caller's JWT claims into every write — the caller cannot supply a different `org_id` in the request body.
2. **Filters** every list query with `WHERE org_id = <caller_org_id>`.
3. **Scopes** every get/update/delete to the caller's org — a record belonging to a different org is returned as 404, not 403.

The 404-not-403 invariant is intentional: leaking that a record *exists* but is forbidden is an information disclosure. An attacker enumerating UUIDs should not be able to distinguish "this record belongs to another tenant" from "this record doesn't exist."

## When NOT to use this

- **Super-admin cross-tenant access**: `tenant_key` always scopes to the caller's org. If you need a super-admin role to see all tenants' data, implement that as a separate resource or a custom handler — do not put super-admin endpoints on a `tenant_key`-scoped resource.
- **Shared resources** (e.g. a global lookup table accessible to all tenants): omit `tenant_key`. Only use it on resources whose rows are owned by a single tenant.
- **User-level isolation** (each row belongs to a specific user, not an org): use `owner` in auth roles instead of `tenant_key`.

## Key design notes for LLM authors

1. **Do not expose `org_id` in `create.input`** when `tenant_key` is set. The runtime injects it automatically from the caller's claims. Adding it to `input:` would let callers write to a different org.

   ```yaml
   # WRONG — org_id in input lets callers choose their tenant
   create:
     input: [title, body, org_id, created_by]

   # CORRECT — org_id injected by runtime from JWT claim
   create:
     input: [title, body, created_by]
   ```

2. The `tenant_key` field (`org_id`) must be `type: uuid` and `ref: organizations.id`. The validator enforces this.

3. The organizations resource does NOT have `tenant_key` — organizations are not themselves tenant-scoped. Only child resources (documents, projects, tasks, etc.) get the `tenant_key` declaration.

4. The cross-tenant 404 behavior is tested in `tests/integration.rs` — the test requires `TestServer::with_role_and_org` which is not yet in the test-support library (see ignore comment in the test).
