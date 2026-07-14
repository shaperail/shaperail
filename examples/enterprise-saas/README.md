# Enterprise SaaS Example — Billing & Subscription Management

This example demonstrates complex enterprise patterns built with Shaperail:
multi-tenant billing, approval workflows, audit trails, and cross-resource
business logic.

## What This Example Demonstrates

### Invoice Approval Workflow (State Machine)

Invoices follow a strict state machine enforced by the `enforce_invoice_workflow`
controller:

```
draft -> pending -> sent -> paid
draft -> void
sent  -> overdue
```

- Only `finance` or `admin` roles can transition `pending -> sent`
- Only `admin` can void invoices (`draft -> void`)
- Paid and voided invoices are immutable — no further edits allowed
- `sent_at` is auto-set when an invoice moves to `sent`
- `paid_at` is auto-set when an invoice moves to `paid`

### Payment Validation Pipeline

Payments are validated through multiple checks before acceptance:

1. **Invoice status check** — payments only accepted for `sent` or `overdue`
   invoices (not `draft`, `paid`, or `void`)
2. **Amount validation** — payment cannot exceed the remaining balance
   (invoice total minus existing completed/pending payments)
3. **Idempotency guard** — duplicate payments (same invoice + same amount
   within 5 minutes) are rejected
4. **Auto-completion** — when a payment is marked `completed` and the sum of
   all completed payments covers the invoice total, the invoice status
   automatically transitions to `paid`
5. **Immutability** — completed and refunded payments cannot be modified

### Audit Trail Pattern

Every significant state change writes to the `audit_logs` table:

- **Customer plan changes** — logged by `enforce_plan_change` with
  before/after plan values
- **Invoice updates** — logged by `audit_invoice_change` (after-controller)
  with input fields and resulting DB record
- **Auto-paid events** — logged by `enforce_payment_rules` when a payment
  completion triggers automatic invoice payment

Each audit entry captures the opaque authenticated subject, resource type,
resource ID, action name, before/after JSONB snapshots, client IP address, and
timestamp.

### Plan-Based Credit Limits

Customer credit limits are enforced based on plan tier in
`validate_customer`:

| Plan       | Max Credit Limit |
|------------|-----------------|
| free       | 0               |
| starter    | 50,000          |
| pro        | 500,000         |
| enterprise | unlimited       |

Plan transitions are also controlled:

- Plans can only change one tier at a time (no skipping from `free` to `pro`)
- Downgrades are blocked if the customer has outstanding invoices

### Multi-Tenancy

All three resources use `tenant_key: org_id`, which means:

- Every query is automatically scoped to the authenticated user's organization
- `org_id` is auto-filled from the JWT tenant claim
- Cross-tenant data access is impossible at the framework level

## Directory Structure

```
enterprise-saas/
├── README.md                           # this file
├── shaperail.config.yaml               # project config (Postgres, Redis, JWT)
├── docker-compose.yml                  # local Postgres + Redis
├── .env.example                        # environment variables template
├── requests.http                       # sample HTTP requests
├── resources/
│   ├── customers.yaml                  # customer resource definition
│   ├── customers.controller.rs         # validate_customer, enforce_plan_change
│   ├── invoices.yaml                   # invoice resource definition
│   ├── invoices.controller.rs          # prepare_invoice, enforce_invoice_workflow, audit_invoice_change
│   ├── payments.yaml                   # payment resource definition
│   └── payments.controller.rs          # validate_payment, enforce_payment_rules
├── migrations/
│   ├── 0001_create_customers.sql       # customers table
│   ├── 0002_create_invoices.sql        # invoices table with FK to customers
│   ├── 0003_create_payments.sql        # payments table with FK to invoices
│   ├── 0004_create_audit_logs.sql      # audit_logs table (JSONB snapshots)
│   └── 0005_use_opaque_subjects.sql    # safely upgrades authenticated-subject columns to TEXT
└── seeds/
    └── customers.yaml                  # sample customers (enterprise, starter, suspended)
```

## How to Run

1. Start the database and cache:

```bash
cd examples/enterprise-saas
docker compose up -d
```

2. Copy the environment file:

```bash
cp .env.example .env
```

3. Initialize and serve:

```bash
shaperail init enterprise-saas
cd enterprise-saas
shaperail serve
```

4. Run migrations and seed data:

```bash
shaperail migrate
shaperail seed
```

5. Open the API docs:

- `http://localhost:3000/docs` — interactive API documentation
- `http://localhost:3000/openapi.json` — OpenAPI 3.1 spec

6. Use `requests.http` to walk through the billing workflow (see the file for
   detailed step-by-step scenarios including expected failures).

## Patterns Showcased

| Pattern                     | Where                                      |
|-----------------------------|--------------------------------------------|
| State machine               | `invoices.controller.rs` — `enforce_invoice_workflow` |
| Role-based transitions      | Invoice send (finance), void (admin), refund (admin) |
| Cross-resource side effects | Payment completion auto-marks invoice as paid |
| Audit trail (after-controller) | `invoices.controller.rs` — `audit_invoice_change` |
| Idempotency guard           | `payments.controller.rs` — duplicate detection |
| Plan-based validation       | `customers.controller.rs` — credit limit enforcement |
| Tier transition rules       | `customers.controller.rs` — no skipping, no downgrade with debt |
| Auto-generated fields       | Invoice number (`INV-YYYYMMDD-0001`), `created_by`, `org_id` |
| Multi-tenancy               | `tenant_key: org_id` on all resources |
| Soft delete                 | Customers and invoices |

## Resources

- [Shaperail Resource Format](../../agent_docs/resource-format.md)
- [Controller System](../../agent_docs/hooks-system.md)
- [Multi-Tenant Example](../multi-tenant/)
- [Blog API Example](../blog-api/)
