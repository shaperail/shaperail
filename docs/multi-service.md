---
title: Multi-service workspaces
parent: Guides
nav_order: 13
---

# Multi-service workspaces

Shaperail supports running multiple services as a coordinated workspace. Each
service has its own `shaperail.config.yaml`, resources, and database, but they
share a Redis-backed service registry for discovery and can call each other
through auto-generated typed clients.

## Workspace layout

A workspace is a directory containing a `shaperail.workspace.yaml` and one
subdirectory per service:

```
my-platform/
├── shaperail.workspace.yaml
├── sagas/
│   └── create_order.saga.yaml
└── services/
    ├── users-api/
    │   ├── shaperail.config.yaml
    │   ├── resources/
    │   │   └── users.yaml
    │   ├── src/main.rs
    │   └── Cargo.toml
    └── orders-api/
        ├── shaperail.config.yaml
        ├── resources/
        │   └── orders.yaml
        ├── src/main.rs
        └── Cargo.toml
```

## `shaperail.workspace.yaml`

```yaml
workspace: my-platform

services:
  users-api:
    path: services/users-api
    port: 3001
  orders-api:
    path: services/orders-api
    port: 3002
    depends_on: [users-api]

shared:
  cache:
    type: redis
    url: redis://localhost:6379
  auth:
    provider: jwt
    secret_env: JWT_SECRET
    expiry: 24h
```

### Fields

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `workspace` | string | yes | -- | Workspace name. |
| `services` | map | yes | -- | Named services. Each key is the service name. |
| `shared` | object | no | -- | Configuration inherited by all services. |

### Service fields

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `path` | string | yes | -- | Relative path from workspace root to the service directory. |
| `port` | integer | no | `3000` | HTTP port for this service. Must be unique across services. |
| `depends_on` | list | no | `[]` | Services that must start before this one. |

### Shared config

The `shared` block supports `cache` and `auth` sections, using the same format
as `shaperail.config.yaml`. Services inherit these values unless they override
them in their own config.

## Starting a workspace

```bash
cd my-platform
shaperail serve --workspace
```

Services start in dependency order. If `orders-api` depends on `users-api`,
the users service starts first. Each service runs as a separate process with
its own port.

## Service registry

When services start, they register in Redis with their name, port, resource
list, and protocols. Other services discover peers through the registry.

Registry keys use the format `shaperail:services:<name>`. Each entry includes:

- Service name and URL
- Resource names exposed by the service
- Enabled protocols (rest, graphql, grpc)
- Health status (starting, healthy, unhealthy, stopped)
- Registration and last heartbeat timestamps

Services send heartbeats every 10 seconds. Registry entries expire after 35
seconds if heartbeats stop.

## Typed inter-service clients

Shaperail generates typed HTTP clients from peer service resource definitions.
The generated client provides methods for every endpoint the peer declares,
with request/response types matching the resource schema.

A generated client for `users-api` looks like:

```rust
let client = UsersApiClient::new("http://localhost:3001")
    .with_auth(token);

let users = client.users_list().await?;
let user = client.users_get("user-id").await?;
let new_user = client.users_create(&UsersInput {
    name: Some(json!("Alice")),
    ..Default::default()
}).await?;
```

Type mismatches between services become compile errors — if `users-api` removes
a field, any service calling that field through the typed client will fail to
compile.

## Distributed sagas

For multi-step operations that span services, define saga YAML files in the
`sagas/` directory:

```yaml
# sagas/create_order.saga.yaml
saga: create_order
version: 1
steps:
  - name: reserve_inventory
    service: inventory-api
    action: POST /v1/reservations
    compensate: DELETE /v1/reservations/:id
    timeout_secs: 5
  - name: charge_payment
    service: payments-api
    action: POST /v1/charges
    compensate: POST /v1/charges/:id/refund
    timeout_secs: 10
  - name: create_order_record
    service: orders-api
    action: POST /v1/orders
    compensate: DELETE /v1/orders/:id
    timeout_secs: 5
```

### Saga fields

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `saga` | string | yes | -- | Saga name (unique within workspace). |
| `version` | integer | no | `1` | Saga version. |
| `steps` | list | yes | -- | Ordered list of steps. |

### Step fields

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `name` | string | yes | -- | Step name (unique within saga). |
| `service` | string | yes | -- | Target service name. |
| `action` | string | yes | -- | Forward action: `METHOD /path` (e.g. `POST /v1/items`). |
| `compensate` | string | yes | -- | Compensating action for rollback. |
| `input` | object | no | -- | JSON input mapping. |
| `timeout_secs` | integer | no | `30` | Step timeout in seconds. |

Steps execute sequentially. If any step fails, compensating actions run in
reverse order to roll back completed steps.

## Validation rules

- Workspace must have at least one service.
- Service ports must be unique.
- `depends_on` references must point to services that exist in the workspace.
- Services cannot depend on themselves.
- Circular dependencies are rejected.
- Saga step names must be unique within a saga.
- Saga actions must follow `METHOD /path` format (e.g. `POST /v1/items`).
