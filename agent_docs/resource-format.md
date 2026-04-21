# Shaperail Resource File Format

## IMPORTANT
This is the exact format from the PRD. Every parser, validator, and codegen
module must support this format precisely. Do not invent alternative syntax.

## File Location
Canonical convention: `resources/<resource-name>.yaml`

The CLI loads `*.yaml` resource files from `resources/`. `.yml` is not part of
the canonical Shaperail resource format.

## Top-Level Keys
```
resource:   # required — snake_case plural name
version:    # required — integer, starts at 1
schema:     # required — field definitions
db:         # optional (M14) — named database connection; default "default"
tenant_key: # optional (M18) — schema field (must be uuid) for tenant isolation
endpoints:  # optional — if omitted, no HTTP routes are generated
relations:  # optional
indexes:    # optional — additional DB indexes beyond schema defaults
```

## Schema Field Format (inline, compact)
```yaml
schema:
  <field_name>: { type: <type>, <constraint>: <value>, ... }
```

## Field Types
| Type        | SQL             | Rust Type              | Notes                        |
|-------------|-----------------|------------------------|------------------------------|
| `uuid`      | UUID            | Uuid                   | use for all IDs              |
| `string`    | TEXT/VARCHAR(n) | String                 | add `max:` for VARCHAR       |
| `integer`   | INTEGER         | i32                    |                              |
| `bigint`    | BIGINT          | i64                    |                              |
| `number`    | NUMERIC(p,s)    | f64                    |                              |
| `boolean`   | BOOLEAN         | bool                   |                              |
| `timestamp` | TIMESTAMPTZ     | DateTime<Utc>          | always with timezone         |
| `date`      | DATE            | NaiveDate              |                              |
| `enum`      | TEXT + CHECK    | generated enum         | requires `values: [...]`     |
| `json`      | JSONB           | serde_json::Value      |                              |
| `array`     | type[]          | Vec<T>                 | add `items: type`            |
| `file`      | TEXT (URL)      | FileRef                | stored in storage backend    |

## Field Constraints
```
primary: true      # primary key
generated: true    # auto-generate on insert (uuid/timestamp)
required: true     # NOT NULL, validated on input
unique: true       # DB unique constraint
nullable: true     # explicitly nullable; non-required fields are treated as optional in generated Rust types
ref: resource.id   # foreign key reference
min: N             # minimum value (number) or length (string)
max: N             # maximum value or length
format: email|url|uuid  # string format validation
values: [...]      # required for enum type
default: value     # default value
sensitive: true    # redacted in logs and error messages
```

## Route Prefixing
The `version` field at the top of each resource YAML drives automatic route
prefixing. All endpoint paths are prefixed with `/v{version}`.

Example: `version: 1` + `path: /users` produces the route `/v1/users`.

You write `path: /users` in the YAML; the framework registers `/v1/users` at
runtime. Do not include the version prefix in the `path:` value.

## Endpoint Format

### Convention-based defaults
For the five standard CRUD action names, `method` and `path` are **optional** —
they are inferred from the resource name:

| Action name | Default method | Default path         |
|-------------|---------------|---------------------|
| `list`      | GET           | `/<resource>`       |
| `get`       | GET           | `/<resource>/:id`   |
| `create`    | POST          | `/<resource>`       |
| `update`    | PATCH         | `/<resource>/:id`   |
| `delete`    | DELETE        | `/<resource>/:id`   |

You can still override `method` and `path` explicitly if needed. For custom
endpoint names, both fields are required.

```yaml
endpoints:
  # Convention-based: method/path inferred from action name
  list:
    auth: [role1, role2]        # or: public
    filters: [field1, field2]
    search: [field1, field2]    # full-text search across these fields
    pagination: cursor           # cursor | offset
    sort: [field1, field2]
    cache: { ttl: 60, invalidate_on: [create, update, delete] }
    rate_limit: { max_requests: 100, window_secs: 60 }

  create:
    auth: [admin]
    input: [field1, field2]     # subset of schema fields accepted
    controller: { before: validate_org }  # Rust fn in resources/<resource>.controller.rs
    events: [user.created]      # emitted after successful write
    jobs: [job_name]            # enqueued after successful write
    upload: { field: avatar_url, storage: s3, max_size: 5mb, types: [jpg, png] }

  # Custom endpoint: method and path are required
  publish:
    method: POST
    path: /users/:id/publish
    auth: [admin]
```

### Endpoint-level keys reference

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `auth` | string or array | No | Roles allowed to call this endpoint, or `public` |
| `filters` | array | No | Query-param filters exposed on list endpoints |
| `search` | array | No | Fields included in full-text search |
| `pagination` | string | No | `cursor` or `offset` |
| `sort` | array | No | Fields available for `?sort=` |
| `input` | array | No | Subset of schema fields accepted as input |
| `cache` | object | No | Per-endpoint response cache. `{ ttl: <seconds>, invalidate_on: [create, update, delete] }`. Requires Redis. |
| `rate_limit` | object | No | Per-endpoint rate limiting. `{ max_requests: <n>, window_secs: <n> }`. Requires Redis. Silently skipped if Redis is not configured. |
| `controller` | object | No | Before/after hooks. `{ before: fn_name }`, `{ after: fn_name }`, or both |
| `events` | array | No | Domain events emitted after a successful write |
| `jobs` | array | No | Background jobs enqueued after a successful write |
| `upload` | object | No | Multipart file upload config. `{ field, storage, max_size, types }` |
| `soft_delete` | boolean | No | Delete sets `deleted_at` instead of removing the row |
| `method` | string | Custom only | HTTP method — required for non-CRUD endpoint names |
| `path` | string | Custom only | Path template — required for non-CRUD endpoint names |

## Controller
Controllers replace the old `hooks:` field. A controller declaration attaches
custom Rust functions that run before and/or after the default handler logic.

### YAML syntax
```yaml
controller: { before: fn_name }              # before only
controller: { after: fn_name }               # after only
controller: { before: fn_before, after: fn_after }  # both
```

### File location
Controller implementations live in `resources/<resource>.controller.rs`.
For a resource named `users`, the file is `resources/users.controller.rs`.

### Function signature
```rust
pub async fn fn_name(ctx: &mut ControllerContext) -> Result<(), ShaperailError> {
    // custom logic
    Ok(())
}
```

See `agent_docs/hooks-system.md` (now the controller-system doc) for
`ControllerContext` fields and usage patterns.

## WASM Plugins (M19)
WASM plugins use the same `controller` field with a `wasm:` prefix on the path:

### YAML syntax
```yaml
controller: { before: "wasm:./plugins/my_validator.wasm" }
controller: { after: "wasm:./plugins/my_enricher.wasm" }
controller: { before: "wasm:./plugins/validate.wasm", after: "wasm:./plugins/enrich.wasm" }
```

### Plugin interface
WASM modules must export: `memory`, `alloc(i32)->i32`, `dealloc(i32,i32)`,
and `before_hook(i32,i32)->i64` or `after_hook(i32,i32)->i64`.

Plugins receive JSON context and return JSON result. See `examples/wasm-plugins/README.md`.

### Sandboxing
Plugins run with NO filesystem, network, env, or clock access (no WASI).
Execution is fuel-limited to prevent infinite loops.

## Multi-Tenancy (M18)
When `tenant_key` is set, Shaperail automatically:
- Filters all list queries by `tenant_key = auth_user.tenant_id`
- Verifies single-record fetches, updates, and deletes belong to the user's tenant
- Auto-injects `tenant_key` into create input data
- Scopes cache keys and rate limits per tenant
- Users with role `super_admin` bypass all tenant filtering

```yaml
resource: projects
version: 1
tenant_key: org_id    # must reference a uuid field in schema

schema:
  id: { type: uuid, primary: true, generated: true }
  org_id: { type: uuid, ref: organizations.id, required: true }
  name: { type: string, required: true }
```

The `tenant_id` is extracted from the JWT `tenant_id` claim.

## Auth Values
```
public               # no auth required
[role1, role2]       # JWT with one of these roles
owner                # JWT user ID matches record's created_by
[owner, admin]       # owner OR admin
```

## Relations Format
```yaml
relations:
  organization: { resource: organizations, type: belongs_to, key: org_id }
  orders:       { resource: orders, type: has_many, foreign_key: user_id }
  profile:      { resource: profiles, type: has_one, foreign_key: user_id }
```

## Complete Example
See resources/users.yaml

## shaperail.config.yaml Format
```yaml
project: my-api
port: 3000
workers: auto

database:
  type: postgresql
  host: ${SHAPERAIL_DB_HOST:localhost}
  port: 5432
  name: my_api_db
  pool_size: 20

cache:
  type: redis
  url: ${SHAPERAIL_REDIS_URL:redis://localhost:6379}

auth:
  provider: jwt
  secret_env: JWT_SECRET
  expiry: 24h
  refresh_expiry: 30d

storage:
  provider: s3
  bucket: ${SHAPERAIL_S3_BUCKET}
  region: ${SHAPERAIL_S3_REGION:us-east-1}

logging:
  level: ${SHAPERAIL_LOG_LEVEL:info}
  format: json
  otlp_endpoint: ${SHAPERAIL_OTLP_ENDPOINT:}
```

Interpolation rules:
- `${VAR}` → requires `VAR` to be set in the environment
- `${VAR:default}` → uses `default` when `VAR` is unset
