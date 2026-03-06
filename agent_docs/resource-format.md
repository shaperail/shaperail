# SteelAPI Resource File Format

## IMPORTANT
This is the exact format from the PRD. Every parser, validator, and codegen
module must support this format precisely. Do not invent alternative syntax.

## File Location
`resources/<resource-name>.resource.yaml`

## Top-Level Keys
```
resource:   # required — snake_case plural name
version:    # required — integer, starts at 1
schema:     # required — field definitions
endpoints:  # optional — defaults to full CRUD if omitted
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
nullable: true     # explicitly nullable (non-required fields are nullable by default)
ref: resource.id   # foreign key reference
min: N             # minimum value (number) or length (string)
max: N             # maximum value or length
format: email|url|uuid  # string format validation
values: [...]      # required for enum type
default: value     # default value
sensitive: true    # redacted in logs and error messages
```

## Endpoint Format
```yaml
endpoints:
  list:
    method: GET
    path: /users
    auth: [role1, role2]     # or: public
    filters: [field1, field2]
    search: [field1, field2] # full-text search across these fields
    pagination: cursor        # cursor | offset
    sort: [field1, field2]
    cache: { ttl: 60, invalidate_on: [create, update, delete] }

  create:
    method: POST
    path: /users
    auth: [admin]
    input: [field1, field2]  # subset of schema fields accepted
    hooks: [hook_fn_name]    # Rust functions in hooks/<resource>.hooks.rs
    events: [user.created]   # emitted after successful write
    jobs: [job_name]         # enqueued after successful write
    upload: { field: avatar_url, storage: s3, max_size: 5mb, types: [jpg, png] }
```

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
See resources/users.resource.yaml

## steel.config.yaml Format
```yaml
project: my-api
port: 3000
workers: auto

database:
  type: postgresql
  host: ${STEEL_DB_HOST:localhost}
  port: 5432
  name: my_api_db
  pool_size: 20

cache:
  type: redis
  url: ${STEEL_REDIS_URL:redis://localhost:6379}

auth:
  provider: jwt
  secret_env: JWT_SECRET
  expiry: 24h
  refresh_expiry: 30d

storage:
  provider: s3
  bucket: ${STEEL_S3_BUCKET}
  region: ${STEEL_S3_REGION:us-east-1}

logging:
  level: ${STEEL_LOG_LEVEL:info}
  format: json
  otlp_endpoint: ${STEEL_OTLP_ENDPOINT:}
```
