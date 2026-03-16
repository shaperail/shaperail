---
title: Examples
nav_order: 4
has_children: true
permalink: /examples/
---

# Examples

Complete examples that show how a real Shaperail application is structured.

## Available examples

| Example | Description |
| --- | --- |
| [**Blog API**]({{ '/blog-api-example/' | relative_url }}) | Two resources (posts, comments), public reads, protected writes, owner-based updates via `created_by`, relations, cursor and offset pagination, soft delete. Includes `resources/*.yaml`, `migrations/*.sql`, and `shaperail.config.yaml`. |
| **Multi-service workspace** | Two services (users-api, orders-api) in a workspace with a distributed saga. Demonstrates `shaperail.workspace.yaml`, service dependencies, shared config, and saga definitions. |
| **Multi-tenant SaaS** | Two resources (projects, tasks) with `tenant_key: org_id` for automatic row-level isolation. Shows JWT tenant claims, per-tenant caching, and `super_admin` bypass. |
| **WASM plugins** | Controller hooks written in TypeScript and Python compiled to WASM. Includes email validation and input normalization examples with the full plugin interface documented. |

Source files live in the repository under `examples/`. Use them as a reference when building your own applications.
