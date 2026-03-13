---
title: Configuration reference
parent: Reference
nav_order: 3
---

Every Shaperail project has a single configuration file named
`shaperail.config.yaml` in the project root. This file controls the HTTP
server, database, cache, authentication, storage, logging, and event routing
for the entire service.

## Minimal config

The only required field is `project`:

```yaml
project: my-app
```

With nothing else specified, the server starts on port 3000 with automatic
worker detection and no database, cache, or auth.

## Full annotated example

```yaml
project: my-api
port: 8080
workers: 4

database:
  type: postgresql
  host: ${DB_HOST:localhost}
  port: 5432
  name: my_api_db
  pool_size: 20

cache:
  type: redis
  url: redis://${REDIS_HOST:localhost}:6379

auth:
  provider: jwt
  secret_env: JWT_SECRET
  expiry: 24h
  refresh_expiry: 30d

storage:
  provider: s3
  bucket: my-bucket
  region: us-east-1

logging:
  level: info
  format: json
  otlp_endpoint: http://localhost:4317

events:
  subscribers:
    - event: "user.created"
      targets:
        - type: webhook
          url: "https://example.com/hooks/user-created"
        - type: job
          name: send_welcome_email
        - type: channel
          name: notifications
          room: "org:{org_id}"
        - type: hook
          name: validate_org
  webhooks:
    secret_env: WEBHOOK_SECRET
    timeout_secs: 30
    max_retries: 3
  inbound:
    - path: /webhooks/stripe
      secret_env: STRIPE_WEBHOOK_SECRET
      events: ["payment.completed", "subscription.updated"]
```

## Section reference

### `project`

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `project` | string | yes | -- | Project name. Used for logging, Docker image tags, and the generated crate name. |

### `port`

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `port` | integer | no | `3000` | TCP port the HTTP server binds to. |

### `workers`

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `workers` | `"auto"` or integer | no | `auto` | Number of Actix-web worker threads. `auto` uses the number of CPU cores. |

### `database`

Optional. Single-database (legacy) mode. When omitted, no database pool is
created. **Ignored when `databases` is set** — use `databases` for
multi-database or to name your primary connection explicitly.

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `type` | string | yes | -- | Database engine. Use `postgresql`. |
| `host` | string | no | `localhost` | Database server hostname. |
| `port` | integer | no | `5432` | Database server port. |
| `name` | string | yes | -- | Database name. |
| `pool_size` | integer | no | `20` | Maximum connections in the sqlx pool. |

### `databases` (multi-database)

Optional. Named database connections for multi-database projects. When set,
the server uses an ORM-backed store and routes each resource to the connection
named by its `db:` key (or `default` when omitted).

You must include a connection named **`default`**; migrations run against the
`default` connection. Use `${VAR}` or `${VAR:default}` in URLs for environment
variable interpolation.

```yaml
databases:
  default:
    engine: postgres
    url: ${DATABASE_URL}
    pool_size: 20
  analytics:
    engine: postgres
    url: postgres://user:pass@analytics-db.example.com/analytics
    pool_size: 10
```

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| *name* | object | yes | -- | Connection name (e.g. `default`, `analytics`). Resources select via `db: <name>`. |
| `engine` | string | yes | -- | One of: `postgres`, `mysql`, `sqlite`. |
| `url` | string | yes | -- | Connection URL (e.g. `postgres://...`, `mysql://...`, `file:data.db`). |
| `pool_size` | integer | no | `20` | Maximum connections in the pool for this database. |

Supported engines:

- **postgres** — PostgreSQL. Full CRUD, filters, sort, pagination, migrations.
- **mysql** — Planned; config accepted, runtime support in progress.
- **sqlite** — Planned; config accepted, runtime support in progress.

When `databases` is present, `database` is ignored and `DATABASE_URL` is only
used if you reference it inside a `databases.*.url` value (e.g. `default`).

### `cache`

Optional. When omitted, no Redis connection is created.

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `type` | string | yes | -- | Cache backend. Use `redis`. |
| `url` | string | yes | -- | Redis connection URL (e.g., `redis://localhost:6379`). |

### `auth`

Optional. When omitted, endpoints that declare `auth` will fail validation.

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `provider` | string | yes | -- | Auth strategy. Use `jwt`. |
| `secret_env` | string | yes | -- | Name of the environment variable holding the signing secret. |
| `expiry` | string | yes | -- | Token lifetime (e.g., `24h`, `60m`). |
| `refresh_expiry` | string | no | -- | Refresh token lifetime (e.g., `30d`). Omit to disable refresh tokens. |

### `storage`

Optional. When omitted, file upload endpoints are unavailable.

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `provider` | string | yes | -- | Storage backend: `s3`, `gcs`, or `local`. |
| `bucket` | string | no | -- | Bucket or container name. Required for `s3` and `gcs`. |
| `region` | string | no | -- | Cloud region (e.g., `us-east-1`). Required for `s3`. |

### `logging`

Optional. Defaults to `info`-level JSON logs with no OTLP export.

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `level` | string | no | `info` | Log level: `debug`, `info`, `warn`, or `error`. |
| `format` | string | no | `json` | Output format: `json` or `pretty`. |
| `otlp_endpoint` | string | no | -- | OpenTelemetry collector endpoint (e.g., `http://localhost:4317`). Omit to disable trace export. |

### `events`

Optional. Controls event subscribers, outbound webhook settings, and inbound
webhook endpoints.

#### `events.subscribers`

A list of event routing rules. Each entry maps an event name to one or more
targets.

```yaml
events:
  subscribers:
    - event: "user.created"
      targets:
        - type: job
          name: send_welcome_email
        - type: webhook
          url: "https://example.com/hooks/user-created"
        - type: channel
          name: notifications
          room: "org:{org_id}"
        - type: hook
          name: validate_org
```

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `event` | string | yes | Event name pattern (e.g., `user.created`, `*.deleted`). |
| `targets` | list | yes | One or more dispatch targets. |

Each target has a `type` field that determines the remaining fields:

| Target type | Fields | Description |
| --- | --- | --- |
| `job` | `name` | Enqueue a background job by name. |
| `webhook` | `url` | POST to an external URL. |
| `channel` | `name`, `room` (optional) | Broadcast to a WebSocket channel. `room` scopes the broadcast. |
| `hook` | `name` | Execute a server-side event handler function by name. |

Note: the `hook` event target type in subscriber configuration is separate from
endpoint-level business logic. For synchronous request-lifecycle logic (input
validation, response enrichment), use `controller:` on endpoints — see
[Controllers]({{ '/controllers/' | relative_url }}).

#### `events.webhooks`

Global settings for outbound webhook delivery.

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `secret_env` | string | no | `WEBHOOK_SECRET` | Environment variable holding the HMAC signing secret. |
| `timeout_secs` | integer | no | `30` | HTTP timeout in seconds per delivery attempt. |
| `max_retries` | integer | no | `3` | Maximum retry attempts for failed deliveries. |

#### `events.inbound`

A list of inbound webhook endpoints that Shaperail registers as routes.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `path` | string | yes | URL path (e.g., `/webhooks/stripe`). |
| `secret_env` | string | yes | Environment variable holding the verification secret. |
| `events` | list of strings | no | Event names this endpoint accepts. Empty means all events. |

## Environment variable interpolation

Use `${VAR}` to inject an environment variable at parse time. Use
`${VAR:default}` to provide a fallback when the variable is unset.

```yaml
project: ${APP_NAME:my-app}
database:
  type: postgresql
  host: ${DB_HOST:localhost}
  name: ${DB_NAME}
```

Rules:

- `${DB_NAME}` -- if `DB_NAME` is not set, the parser returns an error naming
  the missing variable.
- `${DB_HOST:localhost}` -- if `DB_HOST` is not set, `localhost` is used.
- `${}` -- empty placeholders are rejected.
- Unterminated `${...` without a closing `}` is rejected.

Interpolation happens before YAML parsing, so the substituted value becomes
part of the raw YAML text.

## Validation rules

Shaperail rejects invalid configuration at startup with a clear error message.

- **`project` is required.** Omitting it produces a "missing field" error.
- **Unknown fields are rejected.** Every section uses `deny_unknown_fields`. A
  typo like `databse:` instead of `database:` produces an "unknown field" error
  listing the valid alternatives.
- **Type mismatches fail.** Setting `port: "not-a-number"` or `workers: []`
  produces a deserialization error.
- **Missing env vars fail.** A `${VAR}` reference with no default and no
  matching environment variable halts parsing with a message naming the
  variable.
