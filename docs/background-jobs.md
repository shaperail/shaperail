---
title: Background jobs
parent: Guides
nav_order: 7
---

Shaperail includes a Redis-backed background job queue with priority levels,
automatic retries, and a dead letter queue for failed jobs.

## Declaring jobs on endpoints

Add a `jobs` array to any endpoint in your resource YAML. Each entry is the
name of a registered job handler.

```yaml
endpoints:
  create:
    method: POST
    path: /users
    auth: [admin]
    input: [email, name, role, org_id]
    jobs: [send_welcome_email]
```

When the endpoint completes successfully, each listed job is enqueued
automatically with the created record as the payload.

## Priority levels

Every job has a priority. Each level maps to a separate Redis list so the
worker always processes higher-priority jobs first.

| Priority | Redis key | Use case |
| --- | --- | --- |
| `critical` | `shaperail:jobs:queue:critical` | Payment processing, security alerts |
| `high` | `shaperail:jobs:queue:high` | Transactional email, webhooks |
| `normal` | `shaperail:jobs:queue:normal` | Welcome emails, notifications |
| `low` | `shaperail:jobs:queue:low` | Analytics, cleanup tasks |

The worker polls queues in strict priority order: critical, high, normal, low.
A job from a lower queue is only picked up when all higher queues are empty.

Jobs declared via `jobs:` on an endpoint default to `normal` priority.

## Job lifecycle

Every job moves through a fixed set of states:

```
pending --> running --> completed
                  \--> failed --> (retry) --> pending
                           \--> dead letter queue
```

- **pending** -- enqueued and waiting for the worker.
- **running** -- picked up by the worker; handler is executing.
- **completed** -- handler returned success.
- **failed** -- handler returned an error or the job timed out.

Job metadata is stored in a Redis hash at `shaperail:jobs:meta:{job_id}` and
expires after 7 days.

## Retry behavior

Failed jobs are retried with exponential backoff. The delay before each retry
is `2^attempt` seconds:

| Attempt | Backoff |
| --- | --- |
| 1 | 2s |
| 2 | 4s |
| 3 | 8s |

The default `max_retries` is **3**. You can override it when enqueuing with
custom options:

```rust
queue.enqueue_with_options(
    "send_welcome_email",
    payload,
    JobPriority::Normal,
    5,   // max_retries
    300, // timeout_secs
).await?;
```

If a retry is scheduled, the job status returns to `pending` and is pushed
back onto the same priority queue.

## Dead letter queue

Jobs that exhaust all retries are moved to the dead letter queue at
`shaperail:jobs:dead`. Each dead letter entry records:

- job ID and name
- original payload
- final error message
- total attempts
- timestamp of final failure

Once a job enters the dead letter queue its status is permanently set to
`failed`.

## Job timeout

Each job has a configurable timeout. If the handler does not return within the
timeout window, the job is treated as failed and follows the normal retry or
dead letter path.

| Setting | Default |
| --- | --- |
| `timeout_secs` | 300 (5 minutes) |

## Monitoring

Check the queue summary:

```bash
shaperail jobs:status
```

Check the status of a specific job:

```bash
shaperail jobs:status <job_id>
```

Without a job ID, the command prints queue depth by priority, the dead letter
count, and recent failures. With a job ID, it reads the metadata hash at
`shaperail:jobs:meta:{job_id}` and prints the current status, attempt count,
timestamps, and last error if any.

## Redis key reference

| Key pattern | Type | Contents |
| --- | --- | --- |
| `shaperail:jobs:queue:{priority}` | List | Serialized job envelopes |
| `shaperail:jobs:meta:{job_id}` | Hash | Job status, attempt count, timestamps |
| `shaperail:jobs:dead` | List | Dead letter entries |
