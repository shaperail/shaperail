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
| float     | —                | Supports min, max, default                  |
| boolean   | —                | Supports default                            |
| timestamp | —                | Use generated:true for auto-timestamps      |
| enum      | values           | values is required                          |
| json      | —                | Unstructured JSON blob                      |
| array     | items            | items type is required                      |

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
| controller  | ✓    | ✓      | ✓   | ✓      | ✓      | ✓      |
| events      |      | ✓      |     | ✓      | ✓      |        |
| jobs        |      | ✓      |     | ✓      | ✓      |        |
| soft_delete |      |        |     |        | ✓      |        |
| upload      |      | ✓      |     |        |        |        |
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
| database   |          | Single DB: `type`, `host`, `port`, `name`      |
| databases  |          | Multi-DB map: `engine` (postgres/mysql/sqlite/mongodb), `url` |
| cache      |          | Redis: `url`                                   |
| auth       |          | `provider: jwt`, `secret_env: JWT_SECRET`      |
| storage    |          | `provider: s3/gcs/azure/local`, `bucket`       |
| logging    |          | `level`, `format: json/text`                   |
| events     |          | `backend: redis`                               |
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

## Error Codes

| Code  | Trigger                              | Fix                                            |
|-------|--------------------------------------|------------------------------------------------|
| SR001 | Empty resource name                  | Add `resource: <name>`                         |
| SR002 | Version < 1                          | Set `version: 1`                               |
| SR003 | Empty schema                         | Add at least one field                         |
| SR004 | No primary key                       | Add `primary: true` to one field               |
| SR005 | Multiple primary keys                | Remove `primary: true` from extras             |
| SR010 | Enum missing values                  | Add `values: [a, b]`                           |
| SR011 | Values on non-enum                   | Change type to `enum` or remove `values:`      |
| SR012 | ref on non-uuid field                | Change type to `uuid`                          |
| SR013 | ref wrong format                     | Use `ref: resource.field`                      |
| SR014 | Array missing items                  | Add `items: string`                            |
| SR015 | format on non-string                 | Remove or change type to `string`              |
| SR016 | PK not generated                     | Add `generated: true, required: true`          |
| SR020 | tenant_key field absent              | Add field to `schema:`                         |
| SR021 | tenant_key field wrong type          | Set `{ type: uuid, required: true }`           |
| SR040 | input/filter/search/sort field absent | Add to `schema:` or fix name                  |
| SR041 | soft_delete without deleted_at       | Add `deleted_at: { type: timestamp, nullable: true }` |
| SR060 | Relation missing resource            | Add `resource: <name>`                         |
| SR061 | belongs_to missing key               | Add `key: <field>`                             |
| SR062 | has_many/has_one missing foreign_key | Add `foreign_key: <field>`                     |
| SR070 | Index fields empty                   | Add at least one field                         |
| SR071 | Index field not in schema            | Fix field name                                 |
| SR072 | Index order invalid                  | Use `asc` or `desc`                            |
