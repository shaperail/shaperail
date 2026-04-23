---
title: Shaperail Production Readiness Roadmap
date: 2026-04-23
status: approved
---

# Shaperail Production Readiness Roadmap

## Context

Shaperail is at v0.9.0. Core CRUD, authentication, caching, background jobs, WebSockets, GraphQL, file storage, observability, and the full CLI are complete and auto-wired. The question is: what remains before real teams can confidently build complex backends with it?

This document maps the remaining work across four tiers toward full production readiness for all four target use cases: SaaS platforms, event-driven systems, real-time apps, and API-first products.

## Approach

Milestone-driven with tiers. Each tier has a clear exit criterion, and "ready for early adopters" is declared after Tier 1+2 — not after everything is done.

```
Tier 1: Core Correctness  →  Tier 2: DX Polish  →  Tier 3: Reliability  →  Tier 4: Publishing
         (early adopters can start here)                                       (public launch)
```

---

## Tier 1 — Core Correctness

Close the gaps that cause a real team to hit a wall mid-project.

### 1. gRPC Update RPC

**Gap:** `shaperail-runtime/src/grpc/server.rs` returns `Status::unimplemented()` for the Update RPC. List, Get, Create, Delete, and Stream all work.

**Fix:** Implement the Update RPC following the same pattern as the REST `update` handler in `crud.rs` — auth enforcement, field validation against the schema, partial update semantics (only declared `input:` fields), and cache invalidation on success.

**Exit criterion:** A gRPC client can call Update on any resource and receive the updated record. Cross-protocol auth consistency test passes (same credentials get same result via REST, GraphQL, and gRPC).

### 2. Event Subscriber Auto-Wiring

**Gap:** Shaperail auto-emits events and auto-dispatches outbound webhooks. But consuming events internally (e.g., running onboarding logic when `user.created` fires) requires manual wiring outside the schema.

**Fix:** Add a `subscribers:` key to the resource YAML endpoint declaration. Example:

```yaml
endpoints:
  create:
    events: [user.created]
    subscribers:
      - event: user.created
        handler: send_welcome_email
```

At startup, the runtime reads the `subscribers:` declarations across all resources and registers handlers the same way controllers and jobs are registered today — no `main.rs` wiring needed.

**Exit criterion:** A handler declared under `subscribers:` is automatically called when the matching event fires. Invalid subscriber declarations are caught by `shaperail check` with an actionable error code.

### 3. Saga Auto-Wiring

**Gap:** `SagaDefinition`, `SagaStep`, and `SagaExecutionStatus` exist in `shaperail-core` and `workspace_parser.rs` understands sagas. But the runtime does not drive the saga state machine — the user orchestrates steps manually.

**Fix:** The runtime takes over saga execution: persists saga state to the database, advances steps on success, triggers declared compensating actions on failure, and exposes saga status via a standard route (`GET /v1/sagas/:id`). Sagas are declared in `workspace.yaml` alongside service definitions.

**Exit criterion:** A saga with three steps (cross-service create operations) runs to completion when all steps succeed. When step 2 fails, steps 1's compensating action fires automatically. Saga state is queryable via the standard route.

### 4. Custom Endpoint Auto-Wiring

**Gap:** Non-CRUD endpoints (`/auth/login`, `/users/invite`, `/stripe/webhook`) require raw Actix-web handler code with no framework support for auth enforcement, input validation, rate limiting, or response envelopes.

**Fix:** Add a `custom:` endpoint type to the resource YAML:

```yaml
endpoints:
  invite:
    type: custom
    method: POST
    path: /invite
    auth: [admin]
    rate_limit: { max_requests: 10, window_secs: 60 }
    input: [email, role]
    handler: invite_user
```

The framework generates the boilerplate (auth check, input validation against declared fields, rate limit check, response envelope wrapping). The user writes only the `invite_user` function body in a controller file.

**Exit criterion:** A custom endpoint declared in YAML enforces its auth rules, validates declared inputs, and applies rate limiting without any boilerplate in the handler function. `shaperail explain` shows custom endpoints alongside CRUD routes.

---

## Tier 2 — Developer Experience Polish

Make the first-hour experience reliable enough that a new developer succeeds without asking for help.

### 1. Richer Diagnostics for Common YAML Mistakes

**Gap:** `shaperail check` has 72 error codes but messages aren't consistently actionable for the most common beginner mistakes.

**Fix:** Audit the top 10 most likely beginner errors and ensure each has: the exact file + line number, what was found, what was expected, and a one-line fix suggestion. Priority mistakes:
- Referencing a field that doesn't exist in the schema
- Using an unsupported field type
- `auth: admin` instead of `auth: [admin]`
- Declaring a relation with a missing or mismatched `foreign_key`
- Using `ref:` pointing to a resource that doesn't exist

This directly serves the AI-native goal: LLMs get structured feedback they can act on without human interpretation.

**Exit criterion:** Each of the top 10 mistakes produces an error with a correct fix suggestion. Verified by unit tests in `shaperail-codegen/src/diagnostics.rs`.

### 2. Onboarding Gauntlet Test

**Gap:** The CLI has smoke tests but no end-to-end onboarding scenario that simulates a brand new developer.

**Fix:** Add an integration test that runs the full flow: `shaperail init myapp` → edit resource YAML → `shaperail generate` → `cargo build` → `shaperail serve --check`. Every step is asserted. This becomes the primary regression gate — if it breaks, the release is blocked.

**Exit criterion:** The gauntlet test passes on a clean machine (no pre-existing project state) in CI. Any future change that breaks it is caught before merge.

### 3. New Archetypes

**Gap:** The five existing archetypes (`basic`, `user`, `content`, `tenant`, `lookup`) don't cover event-driven or job-queue patterns.

**Fix:** Add three new archetypes to `shaperail resource create <name> --archetype <type>`:
- `event-source` — resource with `events:` on every mutation and a `subscribers:` section pre-filled with a placeholder handler
- `job` — resource with `status`, `retry_count`, `payload` (json), `processed_at` fields and a background job handler registered
- `webhook` — inbound webhook resource with HMAC validation pre-configured and an event log relation

**Exit criterion:** All three archetypes scaffold valid YAML that passes `shaperail check` and generates compilable Rust code.

### 4. `shaperail explain` Relation Graph

**Gap:** `shaperail explain <file>` shows routes, table schema, and indexes but not the full relation graph or what routes relations generate.

**Fix:** Extend the output to include: which resources this one links to via `relations:`, which foreign keys are enforced at the DB level, and what nested routes the relations produce (e.g., `GET /v1/users/:id/orders`).

**Exit criterion:** `shaperail explain resources/users.yaml` shows all relation-generated routes alongside direct routes.

### 5. Hot Reload Stability on Invalid YAML

**Gap:** If a resource YAML edit introduces a validation error during `shaperail serve`, the dev server may crash instead of recovering gracefully.

**Fix:** On file change, run validation before applying the reload. If validation fails, print the diagnostic with the error code, keep the server running with the last valid state, and log "waiting for valid resource file." Retry automatically when the file is saved again.

**Exit criterion:** Introducing a deliberate YAML error during `shaperail serve` prints an error and the server continues serving requests. Fixing the error triggers a clean reload.

---

## Tier 3 — Reliability Validation

Establish confidence that the framework holds up under real traffic and adversarial conditions.

### 1. PRD Benchmark Validation

**Gap:** Idle memory (≤60 MB) and cold start (<100ms) haven't been measured. DB-layer benchmarks (80K req/s cached read, 20K req/s write) require a real Postgres + Redis setup.

**Fix:** Run a full benchmark suite on a release build:
- Idle memory: measure RSS after server startup, before any requests
- Cold start: time from process launch to first successful `/health` response
- DB read (cached): benchmark `GET /v1/users/:id` with Redis warm
- DB write: benchmark `POST /v1/users` under concurrent load

Document results in `docs/performance.md` alongside the PRD targets.

**Exit criterion:** All four targets met and documented. Any miss is treated as a blocking bug, not a known limitation.

### 2. Job Queue Stress Test

**Gap:** No test verifies job queue behavior under high load or edge cases (handler panic, Redis backpressure, dead-letter capture).

**Fix:** Add a stress test that: enqueues 10,000 jobs in a burst, verifies all are processed or dead-lettered, panics one handler intentionally and verifies the job lands in the dead-letter queue without crashing the worker, and measures throughput (jobs/sec).

**Exit criterion:** 10,000 jobs process without data loss. Panicking handler produces a dead-letter entry. `shaperail jobs:status` shows accurate counts throughout.

### 3. WebSocket Room Load Test

**Gap:** No test verifies broadcast performance or resource cleanup at scale.

**Fix:** Add a load test that opens 1,000 concurrent WebSocket connections to one room, broadcasts 100 messages, disconnects 500 clients randomly, and verifies: all connected clients received all messages sent before their disconnect, no channel handles are leaked after disconnect, and memory usage returns to baseline after all connections close.

**Exit criterion:** Test passes. Memory profile shows no leak. Throughput meets or exceeds the REST JSON response benchmark.

### 4. Edge Case Integration Tests

**Gap:** Happy-path and common failure mode tests exist. Missing: saga rollback, event retry on webhook failure, concurrent update conflicts, cache stampede.

**Fix:** Add integration tests for:
- Saga rollback: step 2 of 3 fails → step 1's compensating action runs → saga status is `failed`
- Event retry: webhook endpoint returns 500 three times → event is marked failed, not silently dropped
- Concurrent updates: two simultaneous `PATCH` requests on the same row → one succeeds, one gets a 409 or last-write-wins depending on config
- Cache stampede: 100 concurrent reads on a cold cache key → only one DB query fires, others wait for the first to populate

**Exit criterion:** All four tests pass consistently (no flakiness over 10 runs).

### 5. Cross-Protocol Auth Consistency Test

**Gap:** No test verifies that auth rules are enforced identically across REST, GraphQL, and gRPC.

**Fix:** Add a test that exercises the same resource with the same credentials via all three protocols and asserts identical outcomes: a `member` role gets a 200 on allowed endpoints and a 403 on restricted ones, consistently across REST, GraphQL mutation, and gRPC RPC.

**Exit criterion:** Auth consistency test passes. Any protocol where auth is not enforced is treated as a security bug.

---

## Tier 4 — Publishing Prep

Make it possible for someone who finds Shaperail for the first time to install and use it without help.

### 1. crates.io Packaging

All four crates need complete `[package]` metadata: description, license (MIT or Apache-2.0), keywords, categories, homepage, repository, documentation link. Path dependencies in `Cargo.toml` must be replaced with version dependencies before publish. Verify `cargo install shaperail-cli` works from a clean machine with no prior knowledge of the repo.

**Exit criterion:** `cargo install shaperail-cli` succeeds on a fresh Linux machine. All four crates publish without warnings.

### 2. Pre-Built Binaries and Install Script

GitHub Actions release workflow builds `shaperail` binaries for:
- macOS x86_64 + aarch64
- Linux x86_64 + aarch64
- Windows x86_64

Binaries are attached to GitHub Releases with SHA256 checksums. The install script (`curl -fsSL https://shaperail.io/install.sh | sh`) detects platform, downloads the correct binary, verifies checksum, and installs to `~/.local/bin` or `/usr/local/bin`.

**Exit criterion:** Install script works on all three platforms. Checksum mismatch aborts with a clear error.

### 3. Public Docs Site

Deploy `docs/` as a static site (GitHub Pages or Vercel). Custom domain at `shaperail.io`. Landing page communicates the value proposition in three seconds. `llm-guide.md` and `llm-reference.md` are prominently linked — they are the primary differentiator for AI-native development.

**Exit criterion:** `https://shaperail.io/docs` is live. All 40+ doc pages resolve. No broken links.

### 4. README Overhaul

Root `README.md` converts a skeptic in one scroll:
- The resource YAML example from `CLAUDE.md` leads (it's the most compelling thing in the repo)
- The `init → serve → API call` flow shown in under 20 lines
- Performance numbers from Tier 3 benchmarks
- Single clear link to the docs site

No walls of text. No feature lists that read like a changelog.

**Exit criterion:** Someone unfamiliar with the project reads the README and can answer: what is this, why would I use it, how do I start?

### 5. Launch Checklist

Before any public announcement:
- [ ] All four tiers complete
- [ ] `cargo install shaperail-cli` works on a clean machine
- [ ] `shaperail init myapp && cd myapp && shaperail serve` produces a running API with no errors
- [ ] At least one example project in `examples/` is fully runnable with `docker compose up` and documented
- [ ] Docs site is live with no broken links
- [ ] crates.io pages for all four crates are live
- [ ] GitHub Release with pre-built binaries exists for the launch version

---

## Summary

| Tier | Focus | Exit Criterion |
|------|-------|---------------|
| 1 | Core Correctness | gRPC Update works, event subscribers auto-wire, sagas auto-drive, custom endpoints declared in YAML |
| 2 | DX Polish | New developer ships in under an hour, hot reload never crashes, onboarding gauntlet passes in CI |
| 3 | Reliability | All PRD benchmarks met, job queue and WebSocket load tests pass, edge case integration tests green |
| 4 | Publishing | `cargo install shaperail-cli` works, docs site live, install script works on all platforms |

**Early adopter declaration:** After Tier 1 + Tier 2 complete, Shaperail is ready for teams who want to build on it with the understanding that load testing and publishing are still in progress.

**Public launch:** After all four tiers complete and the launch checklist is signed off.
