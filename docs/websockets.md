---
title: WebSockets
parent: Guides
nav_order: 8
---

Shaperail provides real-time WebSocket support through channel definitions.
Channels are declared in YAML, support JWT auth on upgrade, room-based
subscriptions, and cross-instance broadcasting via Redis pub/sub.

## Channel definition

Create a file at `channels/<name>.channel.yaml`:

```yaml
channel: notifications
auth: [member, admin]
rooms: true
hooks:
  on_connect: [log_connect]
  on_disconnect: [log_disconnect]
  on_message: [validate_message]
```

| Field     | Type            | Required | Description                                      |
|-----------|-----------------|----------|--------------------------------------------------|
| `channel` | string          | yes      | Channel name. Determines the WebSocket path.     |
| `auth`    | string or list  | no       | Auth rule. Omit or set `public` for open access. |
| `rooms`   | bool            | no       | Enable room subscriptions. Default: `false`.      |
| `hooks`   | object          | no       | Lifecycle hooks (see below).                     |

Unknown fields are rejected. There is one canonical format -- no aliases.

## Connection

Clients connect at:

```
ws://<host>/ws/<channel>?token=<jwt>
```

The server validates the JWT **before** completing the WebSocket upgrade.
If auth fails, the client receives an HTTP 401 or 403 -- the handshake never
completes.

- Channels with `auth: public` or no `auth` field accept connections without a token.
- Role-based channels (`auth: [admin, member]`) require a valid JWT whose role
  matches at least one entry in the list.

## Client messages

All client-to-server messages are JSON with an `action` field.

### subscribe

```json
{ "action": "subscribe", "room": "org:123" }
```

Joins the specified room. Requires `rooms: true` in the channel definition.

### unsubscribe

```json
{ "action": "unsubscribe", "room": "org:123" }
```

Leaves the specified room.

### message

```json
{ "action": "message", "room": "org:123", "data": { "text": "hello" } }
```

Sends a message to all subscribers of the room. The `data` field accepts any
valid JSON value. Requires `rooms: true`.

### pong

```json
{ "action": "pong" }
```

Responds to a server ping. Resets the heartbeat timer.

## Server messages

All server-to-client messages are JSON with a `type` field.

### broadcast

```json
{
  "type": "broadcast",
  "room": "org:123",
  "event": "user.created",
  "data": { "id": "abc" }
}
```

Delivers an event to all clients subscribed to the room.

### subscribed

```json
{ "type": "subscribed", "room": "org:123" }
```

Acknowledgement after a successful room subscription.

### unsubscribed

```json
{ "type": "unsubscribed", "room": "org:123" }
```

Acknowledgement after leaving a room.

### error

```json
{ "type": "error", "message": "Room subscriptions not enabled for this channel" }
```

Returned for invalid actions, malformed JSON, or permission failures.

### ping

```json
{ "type": "ping" }
```

Server heartbeat. The client must respond with a `pong` message.

## Room subscriptions

Rooms are logical groups within a channel. Use them to scope broadcasts --
for example, one room per organization or per document.

```json
{ "action": "subscribe", "room": "org:123" }
```

A session can subscribe to multiple rooms simultaneously. When a session
disconnects, all its room subscriptions are cleaned up automatically.

Rooms are created on demand when the first session subscribes and removed
when the last session unsubscribes.

## Broadcasting from the event system

When a resource endpoint fires an event (e.g., `user.created`), the runtime
publishes a `broadcast` message to the matching room via Redis pub/sub. All
connected instances then deliver it to locally subscribed clients.

Example flow:

1. A `POST /users` endpoint declares `events: [user.created]`.
2. The event system publishes to Redis channel `shaperail:ws:notifications`.
3. Every server instance picks up the message and broadcasts it to clients
   subscribed to the target room.

## Cross-instance support

Shaperail uses Redis pub/sub to synchronize broadcasts across multiple server
instances. Each instance subscribes to Redis channels matching the pattern
`shaperail:ws:<channel>`.

When a message is published:

1. The originating instance publishes a `PubSubMessage` (JSON) to Redis.
2. All instances (including the originator) receive it via their subscriber task.
3. Each instance routes the message to locally connected clients in the
   target room.

If the Redis publish fails, the message falls back to local-only broadcast.
This means single-instance deployments work without Redis, but multi-instance
deployments require it.

## Heartbeat

The server sends a `ping` message every **30 seconds**. If the client does not
respond with a `pong` within **60 seconds**, the server closes the connection.

Clients should handle `ping` messages and reply promptly:

```
Server: { "type": "ping" }
Client: { "action": "pong" }
```

Any incoming frame from the client (text, protocol-level ping/pong) also resets
the heartbeat timer.

## Lifecycle hooks

Hooks run at specific points in the connection lifecycle. Declare them in the
channel definition:

```yaml
hooks:
  on_connect: [log_connect]
  on_disconnect: [log_disconnect]
  on_message: [validate_message]
```

| Hook            | When it runs                          |
|-----------------|---------------------------------------|
| `on_connect`    | After the WebSocket upgrade succeeds. |
| `on_disconnect` | When the session closes.              |
| `on_message`    | When the server receives a text frame from the client. |

Each hook field accepts a list of hook function names. Hooks execute in
declaration order.
