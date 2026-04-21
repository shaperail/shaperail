---
title: Events and webhooks
parent: Guides
nav_order: 10
---

# Events and webhooks

Shaperail has event and webhook primitives in the runtime, but only part of the
end-to-end pipeline is scaffolded today.

## What happens automatically today

When the generated app has Redis configured, write handlers can emit events
through the `EventEmitter` created in the scaffold.

Two event paths exist:

- automatic resource events such as `users.created`, `users.updated`,
  `users.deleted`
- extra endpoint events declared under `events:`

Example:

```yaml
endpoints:
  create:
    method: POST
    path: /users
    auth: [admin]
    input: [email, name, role, org_id]
    events: [user.created]
```

Those emissions are non-blocking because the emitter enqueues internal jobs such
as:

- `shaperail:event_log`
- `shaperail:webhook_deliver`
- `shaperail:channel_broadcast`
- `shaperail:hook_execute`

## What is not scaffolded yet

The generated app does **not** automatically provide:

- a worker that consumes the queued event jobs
- registered handlers for webhook delivery, channel broadcast, or hook
  execution
So the emitter and config parsing are present, but subscriber execution still
requires manual wiring.

## Subscribers

Subscriber config is still the canonical declaration format:

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
```

Supported event patterns:

| Pattern | Matches |
| --- | --- |
| `users.created` | exact match |
| `users.*` | any event on one resource |
| `*.created` | one action across resources |
| `*` | every event |

Supported target types:

| Type | Current behavior |
| --- | --- |
| `job` | enqueues the named job |
| `webhook` | enqueues an internal webhook-delivery job |
| `channel` | enqueues an internal channel-broadcast job |
| `hook` | enqueues an internal hook-execution job |

Those internal jobs still need worker handlers in your app.

## Outbound webhooks

The runtime includes a webhook dispatcher that can:

- read a signing secret from `events.webhooks.secret_env`
- compute `X-Shaperail-Signature: sha256=<hex>`
- build a delivery request payload

```yaml
events:
  webhooks:
    secret_env: WEBHOOK_SECRET
    timeout_secs: 30
    max_retries: 3
```

Current limitation: the runtime helper builds signed requests, but the normal
scaffold does not register a real HTTP delivery handler for
`shaperail:webhook_deliver`. Delivery is therefore a manual worker integration
step today.

## Inbound webhooks

The runtime also includes `configure_inbound_routes(...)`, which can register
POST endpoints that:

- verify Shaperail, GitHub, or Stripe signature formats
- extract an event type from the JSON body
- re-emit the payload as `inbound.<event_type>`

Example config:

```yaml
events:
  inbound:
    - path: /webhooks/stripe
      secret_env: STRIPE_WEBHOOK_SECRET
      events: ["payment.completed", "subscription.updated"]
    - path: /webhooks/github
      secret_env: GITHUB_WEBHOOK_SECRET
      events: []
```

The scaffolded app calls `configure_inbound_routes(...)` automatically at
startup by reading the `events.inbound:` section in `shaperail.config.yaml`,
so these routes are live as soon as they are declared — no manual wiring
required.

## Practical guidance

- Treat `events:` and `events.subscribers:` as the source of truth for event
  declarations.
- Expect endpoint writes to enqueue event work automatically when Redis is
  configured.
- Expect delivery, broadcast, and hook execution to be manual until you add
  worker wiring yourself.
- Inbound webhook routes are auto-registered from `shaperail.config.yaml` at
  startup — no manual wiring required.
