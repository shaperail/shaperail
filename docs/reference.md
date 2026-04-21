---
title: Reference
nav_order: 4
has_children: true
permalink: /reference/
---

# Reference

Canonical reference for the Shaperail resource format, project configuration, CLI, and API contract.

## Reference pages

| Page | Description |
| --- | --- |
| [**Resource guide**]({{ '/resource-guide/' | relative_url }}) | Resource YAML: top-level keys, API versioning, multi-database (`db:`), multi-tenancy (`tenant_key:`), schema fields, field types, endpoints, controllers (Rust and WASM plugins), relations, indexes. The schema contract that drives codegen and runtime. |
| [**CLI reference**]({{ '/cli-reference/' | relative_url }}) | All `shaperail` commands: init, generate, validate, migrate, seed, serve, build, export openapi/sdk, doctor, routes, jobs:status. |
| [**Configuration reference**]({{ '/configuration/' | relative_url }}) | `shaperail.config.yaml`: project, port, workers, database, databases (multi-DB), cache, auth, storage, logging, events. Environment variable interpolation. |
| [**API responses and query parameters**]({{ '/api-responses/' | relative_url }}) | Response envelope (data/meta), error format, filtering, sorting, search, pagination (cursor/offset), field selection, relation loading, cache bypass. |
| [**Resource archetypes**]({{ '/archetypes/' | relative_url }}) | The 5 resource archetypes (basic, user, content, tenant, lookup): fields, endpoints, relations, indexes, and when to use each. |

Use these pages when you need the exact syntax, allowed values, or behavior of a feature.

## For AI assistants

These files are designed to be loaded as context into an AI assistant or IDE copilot. They are not part of the site navigation but are publicly accessible.

| File | Description |
| --- | --- |
| [**LLM Guide**]({{ '/llm-guide/' | relative_url }}) | Single-file AI context: resource format, field types, endpoint config, relations, auth, jobs, events, WebSockets, controllers, CLI, and config — everything an LLM needs to generate valid Shaperail resources on the first pass. |
| [**LLM Quick Reference**]({{ '/llm-reference/' | relative_url }}) | Terse lookup tables for field types, endpoint keys, config keys, and error codes. Use alongside the LLM Guide for fast token-efficient lookups. |
