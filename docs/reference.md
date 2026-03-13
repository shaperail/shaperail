---
title: Reference
nav_order: 3
has_children: true
permalink: /reference/
---

# Reference

Canonical reference for the Shaperail resource format, project configuration, CLI, and API contract.

## Reference pages

| Page | Description |
| --- | --- |
| [**Resource guide**]({{ '/resource-guide/' | relative_url }}) | Resource YAML: top-level keys, API versioning, multi-database (`db:`), schema fields, field types, endpoints, controllers, relations, indexes. The schema contract that drives codegen and runtime. |
| [**CLI reference**]({{ '/cli-reference/' | relative_url }}) | All `shaperail` commands: init, generate, validate, migrate, seed, serve, build, export openapi/sdk, doctor, routes, jobs:status. |
| [**Configuration reference**]({{ '/configuration/' | relative_url }}) | `shaperail.config.yaml`: project, port, workers, database, databases (multi-DB), cache, auth, storage, logging, events. Environment variable interpolation. |
| [**API responses and query parameters**]({{ '/api-responses/' | relative_url }}) | Response envelope (data/meta), error format, filtering, sorting, search, pagination (cursor/offset), field selection, relation loading, cache bypass. |

Use these pages when you need the exact syntax, allowed values, or behavior of a feature.
