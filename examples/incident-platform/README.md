# Incident Platform Example

This example is the single-app reference for the parts of Shaperail that need
manual runtime wiring today.

It combines:

- tenant-scoped REST resources
- API-key alert ingestion
- GraphQL and gRPC transport setup
- Redis-backed jobs and event subscribers
- outbound and inbound webhook wiring
- WebSocket incident rooms
- file uploads backed by `type: file`
- response caching and structured logging

The goal is not to present a "special product template". It is a normal example
app that shows how a larger team can wire the current runtime surface together
without pretending the scaffold does more than it does.

## What is in this example

Resources:

- `services` — monitored systems and runbooks
- `incidents` — operator-managed incidents with severity and lifecycle status
- `alerts` — external alerts ingested through API keys or admin JWTs
- `attachments` — uploaded screenshots, logs, and artifacts for incidents

Manual runtime wiring:

- `src/main.rs` — custom bootstrap that extends the scaffolded app
- `src/runtime_extensions.rs` — controller registration, API-key store setup,
  worker handlers, WebSocket channel definition
- `channels/incidents.channel.yaml` — the channel declaration mirrored by the
  manual bootstrap

Supporting files:

- `migrations/*.sql` — schema for the four resources plus audit and event logs
- `requests.http` — REST and GraphQL walkthrough
- `seeds/services.yaml` — sample services
- `scripts/smoke.sh` — end-to-end verification script

## Current runtime reality

This standalone example intentionally owns more bootstrap wiring than a standard
scaffold:

- `shaperail generate` auto-registers declared controllers in normal scaffolded
  apps; this app builds a custom controller map in `src/runtime_extensions.rs`
  alongside its other runtime extensions
- this example keeps fields such as `created_by` explicit so its controllers
  can demonstrate policy checks; Shaperail's two-phase validation also supports
  omitting server-owned fields from `input` and injecting them in a
  before-controller
- API keys work only when you inject an `ApiKeyStore`
- event subscribers enqueue work, but you still need to start a worker
- inbound webhook routes are not registered automatically
- channel YAML files are not auto-loaded, so WebSocket routes are registered in
  `src/main.rs`
- outbound webhook delivery in this example uses a real HTTP client in
  `src/runtime_extensions.rs`; the target defaults to the local
  `/dev/webhook-sink` route so the flow is verifiable without external
  infrastructure

## Quick start

This example is already laid out as a standalone app under
`examples/incident-platform/`.

1. Move into the example and copy the env file:

```bash
cd examples/incident-platform
cp .env.example .env
```

2. Start local infrastructure:

```bash
docker compose up -d
```

3. Generate typed modules from the resource files:

```bash
cargo run --manifest-path ../../Cargo.toml -p shaperail-cli -- generate
```

4. Start the example app:

```bash
cargo run --features graphql,grpc
```

You can also use `shaperail generate` once you have the CLI installed, but the
repo-local command above avoids requiring a published install.

5. Mint development tokens as needed:

```bash
curl "http://127.0.0.1:3300/dev/token?user_id=00000000-0000-0000-0000-000000000001&role=admin&tenant_id=aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"
```

6. Run the end-to-end smoke test when you want a full verification:

```bash
./scripts/smoke.sh
```

## Environment

The example expects these environment variables:

- `DATABASE_URL`
- `JWT_SECRET`
- `WEBHOOK_SECRET`
- `PAGERDUTY_WEBHOOK_SECRET`
- `INCIDENT_INGEST_KEY`
- `INCIDENT_OUTBOUND_WEBHOOK_URL`

The provided `.env.example` contains local defaults for development and is
enough for local generation and startup. By default,
`INCIDENT_OUTBOUND_WEBHOOK_URL` points back to
`http://127.0.0.1:3300/dev/webhook-sink`.

## Example flows

### 1. Admin creates a monitored service

Use `POST /v1/services` with a JWT carrying `role=admin` and `tenant_id=<org>`.
The runtime injects `org_id` from the tenant claim. The create controller
verifies `created_by` matches the authenticated subject and generates a slug.

### 2. External monitoring system ingests an alert

Use `POST /v1/alerts` with `X-API-Key: <INCIDENT_INGEST_KEY>`.

Current limitation: the built-in API-key store maps only to `user_id` and
`role`, not a tenant ID. This example therefore requires the ingest payload to
include `org_id` explicitly.

### 3. Incident write triggers jobs, events, and broadcasts

Creating or updating an incident:

- writes the row
- emits automatic resource events
- emits extra endpoint events such as `incident.opened`
- enqueues endpoint jobs such as `notify_on_call`
- lets subscriber targets enqueue webhook and channel jobs
- lets the custom worker push incident events into WebSocket rooms

### 4. Operators upload evidence

Use `POST /v1/attachments` as multipart form data with:

- `org_id`
- `incident_id`
- `kind`
- `created_by`
- `file_url` as the file part

The runtime stores the uploaded file path in `file_url` and fills
`file_url_filename`, `file_url_mime_type`, and `file_url_size`.

Current limitation: multipart upload endpoints currently bypass controller hooks
and tenant auto-injection, so this example sends `org_id` and `created_by`
explicitly on attachment uploads.

### 5. Dashboard clients use REST, GraphQL, and WebSockets together

- REST remains the canonical CRUD surface
- GraphQL gives the dashboard a denser query shape
- WebSocket clients subscribe to `all-incidents` for fleet updates or
  `incident:<slug>` for a single incident room
- outbound webhook deliveries land on `GET/POST /dev/webhook-sink` by default,
  so you can inspect the received payloads locally

## GraphQL sample

```graphql
query IncidentBoard {
  list_incidents(limit: 20, offset: 0) {
    id
    title
    severity
    status
    room_key
    service {
      id
      name
      status
    }
  }
}
```

## gRPC sample

The current runtime supports list/stream/get/create/delete. For this example:

```bash
grpcurl -plaintext \
  -H "authorization: Bearer <admin-jwt>" \
  localhost:53051 list
```

Then inspect services such as:

```bash
grpcurl -plaintext \
  -H "authorization: Bearer <admin-jwt>" \
  -d '{"id":"22222222-2222-2222-2222-222222222222"}' \
  localhost:53051 shaperail.v1.incidents.IncidentService/GetIncident
```

## WebSocket sample

Connect with a JWT:

```text
ws://localhost:3300/ws/incidents?token=<jwt>
```

Subscribe to either:

- `all-incidents`
- `incident:payments-api-latency-spike`

Client message:

```json
{ "action": "subscribe", "room": "all-incidents" }
```

## Files to read first

- `src/main.rs`
- `src/runtime_extensions.rs`
- `resources/incidents.yaml`
- `resources/alerts.yaml`
- `requests.http`
- `scripts/smoke.sh`
