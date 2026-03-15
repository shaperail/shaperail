---
title: gRPC
parent: Guides
nav_order: 13
---

When you enable gRPC, Shaperail exposes the same resources as your REST API over a gRPC server on a separate port. The resource YAML is the source of truth: types, fields, RPCs, and auth are derived from the same schema.

## Enabling gRPC

In `shaperail.config.yaml`, add `grpc` to the `protocols` list:

```yaml
project: my-app
protocols: [rest, grpc]
# ... database, cache, auth, etc.
```

If you omit `protocols`, only REST is enabled. With `protocols: [rest, grpc]`, the framework starts a Tonic gRPC server alongside the HTTP server.

## Configuration

Optional `grpc:` section in `shaperail.config.yaml`:

```yaml
grpc:
  port: 50051       # gRPC server port (default: 50051)
  reflection: true   # enable server reflection for grpcurl (default: true)
```

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `port` | integer | `50051` | Port for the gRPC server. Separate from the HTTP port. |
| `reflection` | boolean | `true` | Enable gRPC server reflection. When true, tools like `grpcurl` can discover services without `.proto` files. |

## Proto generation

Shaperail generates `.proto` files from your resource definitions. Each resource produces:

- A message type with all schema fields
- Request/response messages for each declared endpoint
- A gRPC service with RPCs matching your endpoints

Example: a `users` resource with `list`, `get`, `create`, `delete` endpoints generates:

```protobuf
syntax = "proto3";
package shaperail.v1.users;

message User {
  string id = 1;
  string email = 2;
  string name = 3;
  string role = 4;
}

service UserService {
  rpc ListUsers(ListUsersRequest) returns (ListUsersResponse);
  rpc StreamUsers(ListUsersRequest) returns (stream User);
  rpc GetUser(GetUserRequest) returns (GetUserResponse);
  rpc CreateUser(CreateUserRequest) returns (CreateUserResponse);
  rpc DeleteUser(DeleteUserRequest) returns (DeleteUserResponse);
}
```

Proto files are auto-generated and should not be hand-edited. They are produced for client-side use; the server handles requests dynamically.

### Type mapping

| Shaperail type | Protobuf type |
| --- | --- |
| `uuid` | `string` |
| `string` | `string` |
| `integer` | `int32` |
| `bigint` | `int64` |
| `number` | `double` |
| `boolean` | `bool` |
| `timestamp` | `google.protobuf.Timestamp` |
| `date` | `string` |
| `enum` | `string` |
| `json` | `google.protobuf.Struct` |
| `array` | `google.protobuf.ListValue` |
| `file` | `string` |

## Streaming RPCs

Every resource with a `list` endpoint gets two RPCs:

- **`ListUsers`** — unary RPC returning all matching records in a single response.
- **`StreamUsers`** — server-streaming RPC yielding one record at a time. Use this for large result sets.

Both accept the same request message with filter, search, cursor, page_size, and sort fields.

## Authentication

gRPC uses the same JWT auth as REST and GraphQL:

- Set the `authorization` metadata key to `Bearer <token>` on gRPC calls.
- The server extracts and validates the JWT, then enforces the same RBAC rules declared on endpoints.
- Unauthenticated requests to protected endpoints receive `UNAUTHENTICATED` (code 16).
- Insufficient permissions return `PERMISSION_DENIED` (code 7).

Example with grpcurl:

```bash
grpcurl -plaintext \
  -H "authorization: Bearer eyJ..." \
  -d '{"id": "550e8400-..."}' \
  localhost:50051 shaperail.v1.users.UserService/GetUser
```

## Health checks

The gRPC server includes the standard `grpc.health.v1.Health` service. Each resource service is registered as serving.

```bash
grpcurl -plaintext localhost:50051 grpc.health.v1.Health/Check
```

Returns `SERVING` when the server is healthy.

## Server reflection

When `reflection: true` (the default), the server supports gRPC server reflection. This lets tools like `grpcurl` list and call services without needing `.proto` files:

```bash
# List all services
grpcurl -plaintext localhost:50051 list

# Describe a service
grpcurl -plaintext localhost:50051 describe shaperail.v1.users.UserService
```

## Error mapping

Shaperail errors map to gRPC status codes:

| Shaperail error | gRPC code |
| --- | --- |
| Not found | `NOT_FOUND` (5) |
| Unauthorized | `UNAUTHENTICATED` (16) |
| Forbidden | `PERMISSION_DENIED` (7) |
| Validation | `INVALID_ARGUMENT` (3) |
| Conflict | `ALREADY_EXISTS` (6) |
| Rate limited | `RESOURCE_EXHAUSTED` (8) |
| Internal | `INTERNAL` (13) |

## Same schema as REST

Resource YAML drives REST, GraphQL, and gRPC. You do not define types or services separately. Changes to schema, endpoints, relations, or auth are reflected in all protocols after you regenerate and restart.

## Summary

| Feature | Supported |
| --- | --- |
| Unary RPCs: get, create, delete | Yes |
| Unary RPCs: list (filters, pagination) | Yes |
| Server-streaming RPCs: stream list | Yes |
| Auth: JWT via metadata, RBAC | Yes |
| Health check: grpc.health.v1 | Yes |
| Server reflection | Yes (default on) |
| Proto generation from resource schema | Yes |
