# AgentLedger — Design Spec

**Status:** Draft for approval
**Date:** 2026-05-01
**Framework:** Shaperail (single app, Postgres + Redis)
**Working name:** AgentLedger (rename later if desired)

---

## 1. Problem

In 2026, AI agents increasingly transact on behalf of organizations — calling paid APIs, signing up for SaaS, provisioning cloud resources, paying vendors via virtual cards or stablecoins. Existing tools cover slices:

- Card-issuing controls (Brex, Ramp, Mercury) — operate at the card level, not the agent level. No notion of "task" or "parent agent".
- LLM observability (Helicone, LangSmith, Vellum) — measure tokens and latency; do not produce auditor-grade financial records.
- ERPs / GL systems (NetSuite, QuickBooks, Sage Intacct) — receive aggregate journal entries; have no awareness of who or what caused the spend.
- Cloud cost (Vantage, CloudZero) — vendor-specific, no cross-vendor agent attribution.

Nobody owns the **agent-attributed subledger** category: the canonical financial record of every dollar an agent spends, with hierarchical policy enforcement before the spend, and double-entry posting after it, exportable to the GL of record.

## 2. Goal

Ship a production-grade subledger that:

1. **Attributes** every spend dollar to `(org, agent, task, customer, vendor, category)` with provable lineage.
2. **Enforces** budgets and approval policies at the organization, agent-tree, task-tree, and category levels — synchronously, before money moves where possible.
3. **Posts** every spend as a balanced double-entry journal entry the moment it settles.
4. **Reconciles** the ledger against vendor statements automatically and flags variance.
5. **Exports** to QuickBooks Online and NetSuite (v1: CSV + JSON; v1.5: native).
6. **Audits** with a per-org hash chain so any historical entry is tamper-evident.

Non-goals for v1: tax computation, payroll, AR/AP for non-agent spend, multi-book consolidation, embedded card issuance (we integrate with Stripe Issuing rather than reissue cards ourselves).

## 3. Primary user & buyer

**Finance team (Controller / FP&A / CFO) at a company with 10–1000 active AI agents.** Buyer signs the contract; daily users are the controller and the FP&A analyst. The hosted SaaS dashboard is the primary surface. The OpenAPI/TS SDK that Shaperail generates is the secondary developer surface (free side effect, no extra v1 work for hardening — public-API multi-tenant hardening is a v1.5 concern).

## 4. Architecture

### 4.1 Topology

Single Shaperail application:
- One Postgres for the canonical ledger and resource state.
- One Redis for caches, policy counters, jobs, and event streams.
- One Shaperail HTTP server exposing REST + OpenAPI 3.1 + a generated TS SDK.
- One MCP server (separate Rust binary in the same workspace) exposing the gateway tools (`authorize_intent`, `settle_intent`, `report_usage`). The MCP server is a thin wrapper over the same controllers.

Why monolithic: Shaperail's flat-abstraction principle (resource → runtime, max depth 2) and deterministic codegen reward keeping resources in one project. Single-Postgres ledgers are correct and fast for billions of rows when indexed properly. We can split read replicas later without changing the YAML.

### 4.2 Two ingestion paths, one ledger

```
Cooperative agent ──► POST /v1/spend_intents/authorize  (gateway, controller: authorize_intent)
                                │
                                │  signed token
                                ▼
Cooperative agent ──► POST /v1/spend_events/settle      (controller: settle_intent → post_journal_entry)
                                │
                                ▼
                        journal_entries + journal_lines (atomic, hash-chained)
                                ▲
                                │
Non-cooperative spend ─► POST /v1/vendors/{id}/webhook   (controller: ingest_vendor_event)
                                │
                                │  adapter dispatch
                                ▼
                          spend_events (orphan if no intent)
                                │
                                ▼  job: attribute_orphan
                          spend_events (attributed)
                                │
                                ▼
                          post_journal_entry
```

### 4.3 Money handling — the rule

Shaperail has no `decimal` type. We will **never** use `float` for money.

- All amounts stored as `amount_minor: integer` (minor units, e.g. cents, satoshi-equivalent for crypto stablecoins).
- All currencies stored as `currency: string` (ISO-4217 alpha-3 or ISO-4217-extended `USDC`/`USDT` for stablecoins).
- FX conversions stamp the rate used (`fx_rate_ppm`, parts-per-million integer) and the as-of date — never re-derived.
- Validation in every controller: reject `amount_minor < 0`, reject unknown currency, reject mismatched intent/settlement currency.

This rule is enforced once, in the `post_journal_entry` helper, and surfaced through resource validation by way of declared `min: 0` on every minor-unit field.

## 5. Resources

Seventeen YAML files under `resources/`. Each maps 1:1 to a generated Postgres table and a REST resource. All except `fx_rates` carry `tenant_key: org_id`.

| # | Resource | Purpose |
|---|----------|---------|
| 1 | `organizations` | Tenants. `base_currency`, `gl_provider`, plan. |
| 2 | `users` | Human operators. Roles: `super_admin`, `admin`, `finance`, `viewer`. |
| 3 | `agents` | AI agent identities. `parent_agent_id` (tree), `owner_user_id`, `mcp_secret_hash`, `status`. |
| 4 | `tasks` | Agent runs. `parent_task_id` (tree), `agent_id`, optional `customer_id`, `external_ref`, `status`. |
| 5 | `customers` | Optional, for COGS attribution and unit economics. |
| 6 | `vendors` | Spend sources. `kind` ∈ {`openai`, `anthropic`, `aws`, `stripe`, `modern_treasury`, `generic`}, `ingest_config_json`. |
| 7 | `accounts` | Chart of accounts. `code`, `kind` ∈ {`asset`, `liability`, `expense`, `revenue`, `equity`}, `parent_account_id`. |
| 8 | `policies` | Scoped budgets. `scope` ∈ {`org`, `agent`, `task`, `customer`, `category`}, `scope_id`, `period`, `cap_minor`, `currency`, `action` ∈ {`block`, `require_approval`, `warn`}. |
| 9 | `spend_intents` | Pre-auth tokens. `max_minor`, `expires_at`, `status`, `token_hash`. |
| 10 | `spend_events` | Immutable spend record. `amount_minor`, `vendor_ref` (unique with `vendor_id` for idempotency), `source` ∈ {`gateway`, `webhook`, `csv`, `api`}. |
| 11 | `journal_entries` | Double-entry header. `posting_date`, `source`, `source_id`, `prev_hash`, `entry_hash`. |
| 12 | `journal_lines` | Debit/credit lines. `side`, `amount_minor`, `account_id`, attribution FKs. Σdebit = Σcredit per entry (CHECK constraint). |
| 13 | `approval_requests` | Over-budget intents awaiting human review. |
| 14 | `reconciliations` | Match runs against a vendor period. `total_minor_vendor`, `total_minor_ledger`, `variance_minor`. |
| 15 | `recon_matches` | Per-line match between vendor statement row and `spend_event`. |
| 16 | `fx_rates` | Daily ECB/OXR rates. `base`, `quote`, `rate_ppm`, `as_of_date`. Cross-tenant. |
| 17 | `audit_events` | Auto-generated from `events:` declarations on every write endpoint. Append-only. |

### 5.1 Sample resource — `spend_intents.yaml`

```yaml
# yaml-language-server: $schema=https://shaperail.io/schema/resource.schema.json
resource: spend_intents
version: 1
tenant_key: org_id

schema:
  id:           { type: uuid, primary: true, generated: true }
  org_id:       { type: uuid, ref: organizations.id, required: true }
  agent_id:     { type: uuid, ref: agents.id, required: true }
  task_id:      { type: uuid, ref: tasks.id, required: true }
  vendor_id:    { type: uuid, ref: vendors.id, required: true }
  category:     { type: string, min: 1, max: 64, required: true }
  max_minor:    { type: integer, min: 0, required: true }
  currency:     { type: string, min: 3, max: 3, required: true }
  status:       { type: enum, values: [pending, approved, denied, expired, consumed, partially_consumed], default: pending }
  token_hash:   { type: string, min: 64, max: 64, sensitive: true }
  expires_at:   { type: timestamp, required: true }
  created_at:   { type: timestamp, generated: true }
  updated_at:   { type: timestamp, generated: true }

endpoints:
  list:
    auth: [admin, finance, viewer]
    filters: [agent_id, task_id, vendor_id, status]
    sort: [created_at]
    pagination: cursor

  get:
    auth: [admin, finance, viewer]

  authorize:
    method: POST
    path: /v1/spend_intents/authorize
    auth: [agent]
    input: [agent_id, task_id, vendor_id, category, max_minor, currency]
    controller: { before: authorize_intent, after: emit_intent_event }
    rate_limit: { max_requests: 200, window_secs: 1 }

relations:
  agent:  { resource: agents,  type: belongs_to, key: agent_id }
  task:   { resource: tasks,   type: belongs_to, key: task_id }
  vendor: { resource: vendors, type: belongs_to, key: vendor_id }

indexes:
  - fields: [org_id, agent_id, status]
  - fields: [org_id, task_id, status]
  - fields: [expires_at]
```

(The other sixteen resources follow the same shape and are listed in §11.)

## 6. Controllers

All under `resources/<name>.controller.rs`. Five pull their weight; the rest are glue.

### 6.1 `authorize_intent` (before `spend_intents.authorize`)

1. Validate currency matches agent's allowed currencies.
2. Walk the policy hierarchy: org → agent ancestors → task ancestors → category. Most-specific cap wins; less-specific caps are additive ceilings.
3. Read period counters from Redis (key: `cnt:{org_id}:{scope}:{scope_id}:{period}:{currency}`). Counter is a sum-of-`approved`+`consumed` minor units in the period window.
4. If `counter + max_minor` exceeds any cap with `action: block` → set status `denied`, return 402.
5. If exceeds a cap with `action: require_approval` → create `approval_request`, set status `pending`, return 202 with the approval request ID. Background notifier fires Slack/email.
6. Otherwise → mint short-lived token (15-minute TTL), store `token_hash`, increment Redis counter optimistically, return 200 with token.

The Redis counter is the hot path; it is reconciled from Postgres truth nightly by `enforce_period_budgets`.

### 6.2 `settle_intent` (before `spend_events.settle`)

1. Verify token signature + lookup intent by `token_hash`.
2. Idempotency check: if `(vendor_id, vendor_ref)` already exists in `spend_events`, return existing event.
3. Validate `actual_minor <= max_minor` and currency match. If `actual < max`, status becomes `partially_consumed` and the unused portion is released back to the counter.
4. Insert `spend_event`. Call `post_journal_entry`.
5. Return the settled event.

### 6.3 `ingest_vendor_event` (before `vendors.webhook`)

1. Verify webhook signature using `ingest_config_json.signing_secret`.
2. Adapter dispatch on `vendor.kind`:
   - `openai` / `anthropic` — extract `(amount, currency, occurred_at, agent_trace_header, task_trace_header, vendor_ref)` from the request body. Both providers honor a custom `X-Agent-Trace` header we set via SDK wrapper.
   - `aws` — pull from CUR (Cost & Usage Report) line items via tag-based attribution.
   - `stripe` — webhook for `issuing.authorization` and `charge.succeeded`, attribution from card metadata.
   - `modern_treasury` — webhook for `payment_order` events.
   - `generic` — accept normalized JSON; no attribution unless caller provides it.
3. Match to outstanding intent by `(agent_id, task_id, vendor_id, currency, amount_minor ≤ max_minor, time within token TTL)`. If matched → call `settle_intent` flow. If not → create orphan `spend_event` with `intent_id = null`, enqueue `attribute_orphan` job.

### 6.4 `post_journal_entry` (internal Rust helper, not a Shaperail controller)

1. Open Postgres transaction.
2. Compute lines per posting rule (see §7).
3. Assert Σdebit_minor = Σcredit_minor per currency. (CHECK constraint in DB also enforces this; controller catches it earlier.)
4. Compute `entry_hash = SHA256(org_id || posting_date || canonical_lines || source || source_id || prev_hash)` where `prev_hash` is the latest entry hash for this org. Read prev under row lock to serialize the chain.
5. Insert header + lines.
6. Commit. Emit `journal.posted` event.

### 6.5 `attribute_orphan` (job)

1. Read orphan `spend_event`.
2. Look at any trace headers we captured (vendor-specific). Match to `tasks` by `external_ref` or by header.
3. If unique match → attribute and post journal.
4. If ambiguous → create `approval_request` of kind `attribution_review`, leave event in `unattributed` status, notify finance.

## 7. Posting rules

Each spend event posts one `journal_entry` with at minimum two lines:

| Spend kind | Debit | Credit |
|---|---|---|
| Pay-as-you-go API (OpenAI, Anthropic) | `Expense / AI Compute / <vendor>` | `Accrued AI Vendor Liability / <vendor>` |
| Cloud (AWS / GCP) | `Expense / Cloud / <service>` | `Accrued Cloud Liability / <vendor>` |
| Card charge (Stripe Issuing) | `Expense / <category>` | `Cash / Operating Account` |
| Vendor invoice paid via wire | `Expense / <category>` | `Cash / Operating Account` |
| Pre-funded balance top-up | `Prepaid AI Vendor / <vendor>` | `Cash / Operating Account` |
| API usage against pre-funded balance | `Expense / AI Compute / <vendor>` | `Prepaid AI Vendor / <vendor>` |

Every line carries `agent_id`, `task_id`, `customer_id`, `category` so reports can roll up by any axis without joining back to `spend_events`.

When vendor invoices arrive at month-end, the recon job clears `Accrued AI Vendor Liability` against `Cash` with adjustment entries.

## 8. Policy hierarchy

A policy applies to one of: an org, an agent (and its descendants), a task (and its descendants), a customer, or a category. Multiple policies apply simultaneously; *any* `block` policy that would be exceeded denies the spend. Inheritance:

```
org cap          $10,000/month
  └ agent cap     $1,000/month   (Research-Agent-A)
      └ task cap   $50/run        (deep-research run #42)
```

The intent must satisfy all three caps. If `task` cap is missing, it inherits agent. If `agent` cap is missing, it inherits org. There is always an implicit org cap of `+inf` if none declared (warn-only mode).

## 9. Audit & cryptographic provenance

- `journal_entries.entry_hash` chains per-org. The latest hash for an org is its **head hash**.
- A nightly job `publish_head_hash` writes `(org_id, period_end, head_hash)` to wherever the customer configures: a Slack message, a webhook, an S3-bucket marker file, or (optional) an on-chain anchor. We do not run our own anchoring — we emit the hash and let the customer notarize it.
- Tampering with any historical line breaks the chain; any auditor can recompute and detect.
- `audit_events` table is append-only (no update/delete endpoints generated) and tracks every write across the system via Shaperail's `events:` declaration on each write endpoint.

## 10. Reporting (custom endpoints, all cached)

- `GET /v1/reports/agent_unit_economics?agent_id=...` — total spend by category, by period, with parent/child rollup.
- `GET /v1/reports/task_cost?task_id=...` — full cost of a task tree.
- `GET /v1/reports/customer_cogs?customer_id=...` — direct attributable spend per customer.
- `GET /v1/reports/budget_burndown?policy_id=...` — current period consumption vs cap.
- `GET /v1/reports/close_pack?period=2026-04` — JSON close package: trial balance, JE detail, recon summary, head hash. PDF rendering is handled by the `generate_close_pack` background job (uses `printpdf` crate) and stored to object storage; the endpoint returns a signed URL alongside the JSON.

All read-only, cache TTL 30s, invalidated by `journal.posted` events.

## 11. Resource inventory (full list)

For each resource I list its primary fields and the most important endpoint behaviors. Full YAML is generated during implementation.

1. `organizations` — id, name, plan ∈ {free, growth, enterprise}, base_currency, gl_provider ∈ {none, quickbooks_online, netsuite, csv}, head_hash.
2. `users` — id, org_id, email, role, password_hash. Standard auth resource.
3. `agents` — id, org_id, name, parent_agent_id (nullable, ref agents.id), owner_user_id, status ∈ {active, paused, revoked}, mcp_secret_hash.
4. `tasks` — id, org_id, agent_id, parent_task_id, customer_id, external_ref, status, started_at, ended_at.
5. `customers` — id, org_id, external_id, name.
6. `vendors` — id, org_id, kind, ingest_config_json, signing_secret_hash.
7. `accounts` — id, org_id, code, name, kind, parent_account_id, normal_side ∈ {debit, credit}.
8. `policies` — id, org_id, scope, scope_id, period, cap_minor, currency, action.
9. `spend_intents` — see §5.1.
10. `spend_events` — id, org_id, agent_id, task_id, vendor_id, intent_id, category, amount_minor, currency, vendor_ref (unique per vendor), occurred_at, source.
11. `journal_entries` — id, org_id, posting_date, source, source_id, prev_hash, entry_hash, description.
12. `journal_lines` — id, journal_entry_id, account_id, side, amount_minor, currency, agent_id, task_id, customer_id, category.
13. `approval_requests` — id, org_id, kind ∈ {budget, attribution_review}, intent_id (nullable), spend_event_id (nullable), reviewer_user_id, status, reason, decided_at.
14. `reconciliations` — id, org_id, vendor_id, period_start, period_end, total_minor_vendor, total_minor_ledger, variance_minor, status.
15. `recon_matches` — id, recon_id, spend_event_id, vendor_line_json, match_score, status.
16. `fx_rates` — id, base, quote, rate_ppm, as_of_date.
17. `audit_events` — id, org_id, actor_user_id, actor_agent_id, resource, action, before_json, after_json, occurred_at.

## 12. Background jobs

| Job | Cadence | Purpose |
|---|---|---|
| `reconcile_vendor` | Daily + on-demand | Pull statement, diff against ledger, create reconciliation + matches. |
| `enforce_period_budgets` | Hourly | Re-aggregate Postgres truth → Redis counters. Pause agents whose budget is breached. |
| `expire_intents` | Every 5 min | Mark intents past `expires_at`. |
| `attribute_orphan` | On enqueue | Heuristic attribution of orphan spend events. |
| `export_journal_to_gl` | Nightly | Push JE batch to QBO/NetSuite/CSV. |
| `generate_close_pack` | Monthly + on-demand | Emit close package PDF + JSON. |
| `publish_head_hash` | Daily | Emit per-org head hash to configured destination. |
| `notify_approval_pending` | On enqueue | Slack/email reviewers. |

## 13. Events

Emitted on Redis stream, consumed by webhook subscribers and the audit trail.

| Event | Trigger | Payload |
|---|---|---|
| `intent.authorized` | `authorize_intent` returns 200 | intent_id, token (one-time) |
| `intent.denied` | `authorize_intent` returns 402 | intent_id, reason |
| `intent.requires_approval` | `authorize_intent` returns 202 | intent_id, approval_request_id |
| `spend.settled` | `settle_intent` succeeds | spend_event_id, journal_entry_id |
| `spend.unattributed` | orphan created | spend_event_id, vendor_id |
| `spend.over_budget` | settled spend > caps (warn mode) | spend_event_id, policy_id |
| `policy.breached` | counter exceeds block cap | policy_id, scope_id |
| `agent.paused_for_budget` | enforce_period_budgets pauses agent | agent_id, policy_id |
| `recon.run_complete` | reconcile_vendor finishes | reconciliation_id |
| `recon.variance_detected` | variance_minor != 0 | reconciliation_id, variance_minor |
| `journal.posted` | post_journal_entry commits | journal_entry_id, entry_hash |

## 14. MCP server

Separate Rust binary `agentledger-mcp` in the same Cargo workspace, exposing:

- `authorize_intent(agent_id, task_id, vendor_id, category, max_minor, currency)` → token | denial | approval_request_id
- `settle_intent(token, actual_minor, vendor_ref)` → spend_event_id
- `report_usage(scope, scope_id, period)` → counters

The MCP server is a thin client over the same controllers (calls them in-process via the runtime crate). It does not duplicate logic.

## 15. Public API hardening (v1 scope)

For free, via Shaperail:
- OpenAPI 3.1 + TS SDK generated.
- JWT auth, role-based per endpoint.
- `rate_limit:` declared on every write endpoint.

Out of scope for v1 (deferred to v1.5):
- API-key issuance + rotation UI for external developers.
- Per-key rate limits (Shaperail's rate_limit is per-resource, not per-key).
- Idempotency-Key header convention beyond `vendor_ref` uniqueness.
- Webhook signing for outbound webhooks (we'll ship a static HMAC, not key rotation).

## 15.1 Auth roles

Shaperail's `auth:` list maps to roles declared in `shaperail.config.yaml`. We declare:
- `super_admin`, `admin`, `finance`, `viewer` — human users (JWT-issued).
- `agent` — non-human principals authenticated via per-agent MCP secret (HMAC-signed bearer). Resolves to `agent_id` and inherits the agent's `org_id` for tenant isolation.
- `owner` — Shaperail built-in; resolves to the record creator.

Endpoints that allow `auth: [agent]` are exclusively the gateway-path endpoints (`spend_intents.authorize`, `spend_events.settle`, `tasks.create`).

## 16. Open questions / explicit deferrals

1. **Tax & 1099-NEC handling for agent-as-payer scenarios** — out of scope v1. Add when a customer asks.
2. **Multi-book support (e.g. GAAP + IFRS or stat + tax)** — out of scope v1. Requires a `book_id` on every line; defer until enterprise pull.
3. **Crypto/stablecoin native rails** — accepted only via `generic` adapter today; first-class `usdc_evm` adapter deferred.
4. **In-house card issuing** — never. Stripe Issuing remains the rail.
5. **AI-driven categorization** — controllers categorize deterministically from vendor SKU today. ML-assisted categorization deferred.

## 17. Success criteria for v1

- A new customer can: register an org → create agents → declare policies → mint MCP secrets → wrap their LLM SDK → see live spend in the dashboard within a single onboarding session.
- 100% of spend events settle to a balanced journal entry (Σdebit = Σcredit) with no negative balances and no float arithmetic anywhere.
- Reconciliation against an OpenAI invoice closes with `variance_minor = 0` for the test fixture.
- `shaperail check --json` returns zero errors for all 17 resources.
- Generated OpenAPI spec validates against OpenAPI 3.1.
- The hash chain for the test org is verifiable end-to-end after 1000 simulated entries.

## 18. Out of scope for the design phase

- Frontend dashboard implementation (v1 dashboard is a separate project consuming the generated TS SDK).
- DevOps / Kubernetes manifests (Shaperail's `docker compose` is the dev rail; production deploy is a downstream concern).
- Pricing strategy (separate doc).
- Marketing site / docs.

---

**Next step after approval:** invoke `superpowers:writing-plans` to break implementation into a milestone-by-milestone plan, starting with M01 (organizations, users, agents, tasks — the identity backbone) before anything financial.
