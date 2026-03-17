---
title: Examples
nav_order: 4
has_children: true
permalink: /examples/
---

# Examples

Examples that show how real Shaperail applications are structured. Repository
examples link directly to the checked-in code under `examples/`, and in-guide
walkthroughs cover patterns that are documented rather than checked into the
repo.

## Available examples

| Example | Description |
| --- | --- |
| [**Blog API**]({{ '/blog-api-example/' | relative_url }}) | Two resources (posts, comments) with controllers: slug generation, edit rules, comment rate limiting, XSS prevention. Public reads, protected writes, owner-based updates, relations, cursor/offset pagination, soft delete. The checked-in project lives in [examples/blog-api](https://github.com/shaperail/shaperail/tree/main/examples/blog-api). |
| [**Enterprise SaaS**](https://github.com/shaperail/shaperail/tree/main/examples/enterprise-saas) | Billing and subscription management example with invoice workflow enforcement, payment validation, audit logs, plan-based credit limits, and tenant-scoped business rules. |
| [**Incident platform**](https://github.com/shaperail/shaperail/tree/main/examples/incident-platform) | Single app showing API-key alert ingest, file uploads, jobs/workers, event subscribers, inbound and outbound webhooks, WebSocket rooms, GraphQL/gRPC wiring, and manual runtime bootstrap in `src/main.rs`. |
| [**Multi-service workspace**](https://github.com/shaperail/shaperail/tree/main/examples/multi-service) | Two services (users-api, orders-api) showing workspace layout, dependency-ordered startup, and validated saga definitions for order creation. |
| [**Multi-tenant SaaS**](https://github.com/shaperail/shaperail/tree/main/examples/multi-tenant) | Three resources (organizations, projects, tasks) with controllers: plan-based project limits, status transition enforcement, cross-resource validation, tenant-scoped uniqueness. Shows `tenant_key`, JWT tenant claims, `super_admin` bypass. |
| [**WASM plugins**](https://github.com/shaperail/shaperail/tree/main/examples/wasm-plugins) | Controller hooks written in TypeScript and Python compiled to WASM. Includes email validation and input normalization examples with the full plugin interface documented. |
| [**Controller walkthrough**]({{ '/controllers/' | relative_url }}) | Documentation walkthrough in the Controllers guide. Complements the repository examples with a complete multi-resource billing walkthrough focused on controller patterns and testing guidance. |

The repository currently includes these example directories under `examples/`:
[`blog-api`](https://github.com/shaperail/shaperail/tree/main/examples/blog-api),
[`enterprise-saas`](https://github.com/shaperail/shaperail/tree/main/examples/enterprise-saas),
[`incident-platform`](https://github.com/shaperail/shaperail/tree/main/examples/incident-platform),
[`multi-service`](https://github.com/shaperail/shaperail/tree/main/examples/multi-service),
[`multi-tenant`](https://github.com/shaperail/shaperail/tree/main/examples/multi-tenant),
and [`wasm-plugins`](https://github.com/shaperail/shaperail/tree/main/examples/wasm-plugins).

If you wire controllers into a running app today, manual controller
registration is still required.
