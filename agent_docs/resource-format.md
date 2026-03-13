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
```yaml
endpoints:
  list:
    method: GET
    path: /users                # actual route: /v{version}/users
    auth: [role1, role2]        # or: public
    filters: [field1, field2]
    search: [field1, field2]    # full-text search across these fields
    pagination: cursor           # cursor | offset
    sort: [field1, field2]
    cache: { ttl: 60, invalidate_on: [create, update, delete] }

  create:
    method: POST
    path: /users
    auth: [admin]
    input: [field1, field2]     # subset of schema fields accepted
    controller: { before: validate_org }  # Rust fn in resources/<resource>.controller.rs
    events: [user.created]      # emitted after successful write
    jobs: [job_name]            # enqueued after successful write
    upload: { field: avatar_url, storage: s3, max_size: 5mb, types: [jpg, png] }
```

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
