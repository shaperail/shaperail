---
title: Shaperail Quick Reference
nav_exclude: true
---

# Shaperail Quick Reference

Terse lookup tables. For patterns and examples, see [llm-guide.md](llm-guide.md).

---

## Field Types

| Type      | Required sub-keys | Notes                                       |
|-----------|------------------|---------------------------------------------|
| uuid      | —                | Use for PKs and FKs                         |
| string    | —                | Supports format, min, max                   |
| integer   | —                | Supports min, max, default                  |
| number    | —                | Supports min, max, default                  |
| boolean   | —                | Supports default                            |
| timestamp | —                | Use generated:true for auto-timestamps      |
| date      | —                | Date without a time component               |
| enum      | values           | values is required                          |
| json      | —                | Unstructured JSON blob                      |
| array     | items            | items type is required                      |
| file      | —                | Object-storage file reference               |

## Endpoint Keys by Type

| Key         | list | create | get | update | delete | custom |
|-------------|:----:|:------:|:---:|:------:|:------:|:------:|
| auth        | ✓    | ✓      | ✓   | ✓      | ✓      | ✓      |
| input       |      | ✓      |     | ✓      |        | ✓      |
| filters     | ✓    |        |     |        |        |        |
| search      | ✓    |        |     |        |        |        |
| sort        | ✓    |        |     |        |        |        |
| pagination  | ✓    |        |     |        |        |        |
| cache       | ✓    | ✓      | ✓   |        |        | ✓      |
| controller  | ✓    | ✓      | ✓   | ✓      | ✓      | before only |
| events      |      | ✓      |     | ✓      | ✓      |        |
| jobs        |      | ✓      |     | ✓      | ✓      |        |
| soft_delete |      |        |     |        | ✓      |        |
| upload      |      | ✓      |     |        |        |        |
| rate_limit  | ✓    | ✓      | ✓   | ✓      | ✓      | ✓      |
| method      |      |        |     |        |        | ✓      |
| path        |      |        |     |        |        | ✓      |

## Relation Types

| Type       | Required key | Description                                    |
|------------|-------------|------------------------------------------------|
| belongs_to | key         | FK is on **this** resource                      |
| has_many   | foreign_key | FK is on the **other** resource, returns list   |
| has_one    | foreign_key | FK is on the **other** resource, returns one    |

## Config Keys (`shaperail.config.yaml`)

| Key        | Required | Description                                    |
|------------|----------|------------------------------------------------|
| project    | ✓        | Project name string                            |
| port       |          | HTTP port (default 3000)                       |
| workers    |          | `auto` or integer                              |
| databases  |          | Multi-DB map: `engine` (postgres/mysql/sqlite/mongodb), `url` |
| cache      |          | Redis: `url`                                   |
| auth       |          | `provider: jwt`, `secret_env: JWT_SECRET`      |
| storage    |          | `provider: s3/gcs/azure/local`, `bucket`       |
| logging    |          | `level`, `format: json/text`                   |
| events     |          | Subscribers plus inbound/outbound webhooks     |
| protocols  |          | List: `[rest, graphql, grpc]`                  |

## CLI Commands

| Command                               | Description                                           |
|---------------------------------------|-------------------------------------------------------|
| `shaperail init <name>`               | Scaffold new project                                  |
| `shaperail serve [--port N]`          | Start dev server with hot reload                      |
| `shaperail generate`                  | Run codegen for all resources                         |
| `shaperail check [path] [--json]`     | Validate with structured fix suggestions              |
| `shaperail explain <file>`            | Show routes, table schema, relations                  |
| `shaperail diff`                      | Show codegen changes (dry run)                        |
| `shaperail llm-context [--resource N] [--json]` | Dump project context for LLM           |
| `shaperail migrate [--rollback]`      | Apply or rollback SQL migrations                      |
| `shaperail seed [path]`               | Load fixture YAML into database                       |
| `shaperail routes`                    | List routes with auth requirements                    |
| `shaperail export openapi`            | Output OpenAPI 3.1 spec                               |
| `shaperail export sdk --lang ts`      | Generate TypeScript SDK                               |
| `shaperail export json-schema`        | Output JSON Schema for resource YAML                  |
| `shaperail resource create <name> [--archetype basic\|user\|content\|tenant\|lookup]` | Scaffold resource |
| `shaperail doctor`                    | Check system dependencies                             |

## Archetypes

| Archetype | Fields included                                                 |
|-----------|-----------------------------------------------------------------|
| basic     | id, created_at, updated_at                                      |
| user      | id, email, name, role, password_hash, created_at, updated_at   |
| content   | id, title, body, status, author_id, created_at, updated_at     |
| tenant    | id, name, plan, created_at, updated_at (+ tenant isolation)    |
| lookup    | id, code, label, active, sort_order                            |

## Diagnostics

Run `shaperail check --json`. Diagnostics include the stable code, severity,
source span when available, canonical fix, valid example, and permanent
`doc_url`. See the [error reference](errors/) for the complete registry.
