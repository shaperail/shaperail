# Multi-Tenant SaaS Example

A project management API demonstrating Shaperail's automatic multi-tenancy.
Organizations are tenants; projects and tasks are scoped by `org_id`.

## Resources

| Resource | Tenant-scoped | Description |
| --- | --- | --- |
| `organizations` | No | Tenant entities (the orgs themselves) |
| `projects` | Yes (`org_id`) | Projects within an organization |
| `tasks` | Yes (`org_id`) | Tasks within projects, also scoped to org |

## How it works

1. `projects.yaml` and `tasks.yaml` declare `tenant_key: org_id`
2. JWTs include a `tenant_id` claim matching the user's organization
3. All queries for projects and tasks are automatically filtered by `org_id`
4. Users with role `super_admin` can access all organizations' data

## Controllers

Each resource declares controllers that enforce enterprise multi-tenant business
rules. Controller functions live in `resources/<resource>.controller.rs` and run
before or after the database operation.

### Plan-based resource limits

The `validate_project` controller checks the organization's plan before allowing
project creation. Free plans are limited to 3 projects, pro to 20, and
enterprise is unlimited. The limit query only counts non-deleted projects:

```sql
SELECT COUNT(*) FROM projects WHERE org_id = $1 AND deleted_at IS NULL
```

### Status transition enforcement

Controllers enforce strict state machines on resources:

- **Projects:** archived projects cannot be reopened (create a new one instead).
  Only admins can archive, and archiving is blocked while tasks are in progress.
- **Tasks:** status must follow `todo -> in_progress -> done -> archived`. Only
  the assignee or an admin can mark a task as done.
- **Plans:** transitions follow `free -> pro -> enterprise` with no skipping.
  Downgrading from enterprise to free requires support.

### Cross-resource validation

Controllers query across resource boundaries to maintain consistency:

- Creating a task checks the parent project's status (rejects if archived)
- Archiving a project checks for in-progress tasks (blocks if any exist)
- Members may self-assign tasks; tenant admins may assign another external
  subject. Add an identity-membership lookup if your application needs stronger
  assignee validation.
- Plan changes on organizations affect future project creation limits

### Tenant-scoped uniqueness checks

Uniqueness constraints are scoped to the tenant where appropriate:

- Organization names are globally unique (case-insensitive)
- Project names are unique within an organization (case-insensitive)
- Both checks exclude soft-deleted records

## Running

```bash
docker compose up -d
shaperail validate
shaperail migrate
shaperail seed
shaperail serve
```

## Testing tenant isolation

```bash
# Get a token for org-a member
TOKEN_A=$(curl -s localhost:3000/auth/token \
  -d '{"user_id":"user-1","role":"member","tenant_id":"ORG_A_UUID"}' \
  | jq -r .access_token)

# Get a token for org-b member
TOKEN_B=$(curl -s localhost:3000/auth/token \
  -d '{"user_id":"user-2","role":"member","tenant_id":"ORG_B_UUID"}' \
  | jq -r .access_token)

# User A only sees org A's projects
curl -H "Authorization: Bearer $TOKEN_A" localhost:3000/v1/projects

# User B only sees org B's projects
curl -H "Authorization: Bearer $TOKEN_B" localhost:3000/v1/projects

# super_admin sees everything
TOKEN_ADMIN=$(curl -s localhost:3000/auth/token \
  -d '{"user_id":"admin-1","role":"super_admin"}' \
  | jq -r .access_token)
curl -H "Authorization: Bearer $TOKEN_ADMIN" localhost:3000/v1/projects
```
