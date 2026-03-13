---
title: Events and webhooks
parent: Guides
nav_order: 10
---

# Events and webhooks

Shaperail has a built-in event system that emits events on every data mutation,
routes them to configurable targets, and delivers outbound webhooks with
HMAC-SHA256 signatures. It also accepts inbound webhooks from external services.

All event processing is non-blocking. Events are enqueued as background jobs and
never delay the HTTP response to your client.

## Auto-emitted events

Every generated endpoint that creates, updates, or deletes a record
automatically emits an event named `<resource>.<action>`:

| Endpoint action | Event name        |
|-----------------|-------------------|
| `create`        | `users.created`   |
| `update`        | `users.updated`   |
| `delete`        | `users.deleted`   |

The event payload contains the resource name, action, full record data, a
unique `event_id`, and an ISO 8601 timestamp.

## Custom events

Add the `events:` key to any endpoint definition to emit additional events:

```yaml
endpoints:
  create:
    method: POST
    path: /users
    auth: [admin]
    input: [email, name, role, org_id]
    events: [user.created]
    jobs: [send_welcome_email]
```

Custom event names follow the same `<resource>.<action>` convention but you
choose the name.

## Event subscribers

In `shaperail.config.yaml`, the `events.subscribers` list maps event patterns
to one or more targets. Each subscriber has an `event` pattern and a list of
`targets`.

```yaml
events:
  subscribers:
    - event: "users.created"
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

    - event: "*.deleted"
      targets:
        - type: job
          name: cleanup_job
```

### Event patterns

Subscribers match events using these patterns:

| Pattern           | Matches                        |
|-------------------|--------------------------------|
| `users.created`   | Exact match only               |
| `users.*`         | Any event on the users resource|
| `*.created`       | Any resource's created event   |
| `*`               | Every event                    |

### Target types

| Type      | Key fields       | Behavior                                       |
|-----------|------------------|-------------------------------------------------|
| `job`     | `name`           | Enqueues a background job by name               |
| `webhook` | `url`            | POSTs the event payload to an external URL      |
| `channel` | `name`, `room`   | Broadcasts to a WebSocket channel and room      |
| `hook`    | `name`           | Executes a named server-side event handler function |

Note: the `hook` event target type runs asynchronously in response to events. It
is separate from endpoint-level [controllers]({{ '/controllers/' | relative_url }}),
which run synchronously within the HTTP request.

## Outbound webhooks

When a subscriber target has `type: webhook`, Shaperail delivers the event
payload as an HTTP POST to the specified URL.

### Signing

Every outbound request includes an `X-Shaperail-Signature` header:

```
X-Shaperail-Signature: sha256=<hex-encoded HMAC-SHA256 digest>
```

The digest is computed over the raw JSON request body using the secret from the
environment variable specified in `events.webhooks.secret_env`.

To verify on the receiving end, compute HMAC-SHA256 over the raw body with your
shared secret and compare the hex digest (use constant-time comparison).

### Webhook configuration

```yaml
events:
  webhooks:
    secret_env: WEBHOOK_SECRET   # env var holding the signing secret
    timeout_secs: 30             # HTTP timeout per delivery attempt
    max_retries: 3               # retry attempts on failure
```

All three fields have defaults (`WEBHOOK_SECRET`, `30`, `3`) so the entire
`webhooks:` block is optional if the defaults work for you.

### Retry behavior

Failed deliveries (non-2xx status or connection error) are retried up to
`max_retries` times with exponential backoff. Retries are managed through the
job queue, so they survive server restarts.

### Webhook delivery log

Every delivery attempt is recorded in the `shaperail_webhook_delivery_log`
table with:

| Column        | Description                               |
|---------------|-------------------------------------------|
| `delivery_id` | Unique ID for this attempt                |
| `event_id`    | The event that triggered delivery         |
| `url`         | Target webhook URL                        |
| `status_code` | HTTP status (0 if connection failed)      |
| `status`      | `success`, `failed`, or `pending`         |
| `latency_ms`  | Response time in milliseconds             |
| `error`       | Error message if delivery failed          |
| `attempt`     | Attempt number (1, 2, 3, ...)             |
| `timestamp`   | ISO 8601 timestamp                        |

Query delivery history for a specific event or list recent deliveries for
debugging.

## Inbound webhooks

Shaperail can receive webhooks from external services, verify their signatures,
and re-emit the payload as internal events.

### Configuration

```yaml
events:
  inbound:
    - path: /webhooks/stripe
      secret_env: STRIPE_WEBHOOK_SECRET
      events: ["payment.completed", "subscription.updated"]

    - path: /webhooks/github
      secret_env: GITHUB_WEBHOOK_SECRET
      events: []   # empty = accept all event types
```

Each entry registers a POST endpoint at the specified `path`. The `secret_env`
field names the environment variable holding the verification secret.

The `events` list filters which event types are accepted. An empty list accepts
all events.

### Signature verification

Inbound verification supports three header formats automatically:

| Service    | Header                    | Format                          |
|------------|---------------------------|---------------------------------|
| Shaperail  | `X-Shaperail-Signature`   | `sha256=<hex>`                  |
| GitHub     | `X-Hub-Signature-256`     | `sha256=<hex>`                  |
| Stripe     | `Stripe-Signature`        | `t=<timestamp>,v1=<signature>`  |

The handler checks headers in this order and uses the first one found. Stripe
signatures are verified by computing HMAC-SHA256 over `<timestamp>.<body>`.

Requests with invalid or missing signatures return 401 Unauthorized.

### Internal re-emission

Accepted inbound payloads are re-emitted as internal events named
`inbound.<event_type>`, where the event type is extracted from the `type`,
`event`, or `action` field in the request body. These events flow through the
same subscriber system as any other event.

## Event log

All emitted events are written to the `shaperail_event_log` table. This is an
append-only audit trail -- records are never updated or deleted.

| Column     | Description                       |
|------------|-----------------------------------|
| `event_id` | Unique event ID                  |
| `event`    | Event name (e.g., `users.created`)|
| `resource` | Resource name                    |
| `action`   | Action that triggered the event  |
| `data`     | Full record payload (JSONB)      |
| `timestamp`| ISO 8601 timestamp               |

The event log is written via the job queue, so it is also non-blocking.

## Full configuration example

```yaml
events:
  subscribers:
    - event: "users.created"
      targets:
        - type: job
          name: send_welcome_email
        - type: webhook
          url: "https://hooks.example.com/new-user"
        - type: channel
          name: notifications
          room: "org:{org_id}"

    - event: "orders.*"
      targets:
        - type: hook
          name: recalculate_totals

    - event: "*"
      targets:
        - type: job
          name: audit_logger

  webhooks:
    secret_env: WEBHOOK_SECRET
    timeout_secs: 30
    max_retries: 3

  inbound:
    - path: /webhooks/stripe
      secret_env: STRIPE_WEBHOOK_SECRET
      events: ["payment.completed", "subscription.updated"]
    - path: /webhooks/github
      secret_env: GITHUB_WEBHOOK_SECRET
      events: []
```
