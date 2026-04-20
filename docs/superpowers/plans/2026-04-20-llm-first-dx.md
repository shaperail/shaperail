# LLM-First Developer Experience Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Shaperail usable by AI models (Claude, ChatGPT) with minimal context, a single reference file, and self-correcting CLI errors.

**Architecture:** Three pillars — (1) `docs/llm-guide.md` as the canonical single-file context, (2) `docs/llm-reference.md` as a dense quick-lookup table, and (3) a new `shaperail llm-context` CLI command that dumps project state in 50–100 lines for brownfield use. The `Diagnostic` struct already has `fix` and `example` fields; no changes to error output format needed.

**Tech Stack:** Rust (shaperail-cli, shaperail-core, shaperail-codegen), Markdown (docs), clap (CLI flag definitions), serde_json (JSON output).

---

## Task 1: Create `docs/llm-guide.md`

**Files:**
- Create: `docs/llm-guide.md`

- [ ] **Step 1: Write `docs/llm-guide.md` with the exact content below**

Create `/Users/Mahin/Desktop/shaperail/docs/llm-guide.md` with this exact content:

```markdown
# Shaperail LLM Guide

Load this file as your sole context. You do not need other docs to build in Shaperail.

---

## 1. Resource File Structure

Every resource is a YAML file at `resources/<name>.yaml`.

```yaml
resource: <name>      # snake_case plural noun (required)
version: <int>        # >= 1 — sets route prefix /v{n}/... (required)
db: <db_name>         # named DB from config.databases (optional)
tenant_key: <field>   # schema field name for row-level tenant isolation (optional)
schema: ...           # map of field definitions (required, must include a primary key)
endpoints: ...        # map of endpoint definitions (optional)
relations: ...        # map of relation definitions (optional)
indexes: ...          # list of index definitions (optional)
```

---

## 2. Field Types

| Type      | Requires        | Valid Options                                                         |
|-----------|-----------------|-----------------------------------------------------------------------|
| uuid      |                 | primary, generated, required, unique, ref, sensitive                  |
| string    |                 | required, unique, min, max, format, sensitive                         |
| integer   |                 | required, unique, min, max, default                                   |
| float     |                 | required, min, max, default                                           |
| boolean   |                 | required, default                                                     |
| timestamp |                 | generated, required, nullable                                         |
| enum      | values          | values (required), default, required                                  |
| json      |                 | required, nullable                                                    |
| array     | items           | items (required — e.g. `items: string`), required                    |

`format` valid values: `email`, `url`, `slug`, `phone` (string fields only).  
`ref` format: `resource_name.field_name` — the field must be `type: uuid`.

---

## 3. Field Options Reference

| Option    | Type    | Applies to           | Effect                                                          |
|-----------|---------|----------------------|-----------------------------------------------------------------|
| primary   | bool    | uuid                 | Marks as primary key                                            |
| generated | bool    | uuid, timestamp      | Auto-generated (UUID v7 / NOW()) — do not include in input     |
| required  | bool    | any                  | NOT NULL in DB, required in create/update input                |
| unique    | bool    | any                  | UNIQUE constraint                                               |
| nullable  | bool    | timestamp, json      | Allows null — overrides `required`                              |
| min       | number  | string, int, float   | Min length (string) or minimum value (numbers)                 |
| max       | number  | string, int, float   | Max length (string) or maximum value (numbers)                 |
| format    | string  | string only          | Validation format: email / url / slug / phone                  |
| values    | list    | enum only            | Allowed enum values — required when `type: enum`               |
| default   | any     | enum, bool, int      | Default value. For enum must be one of `values`                |
| ref       | string  | uuid only            | Foreign key reference in `resource.field` format               |
| items     | string  | array only           | Element type — required when `type: array`                     |
| sensitive | bool    | uuid, string         | Redacted in logs, omitted from list responses                  |

---

## 4. Endpoints

### Convention-based (method + path inferred from action name)

| Action | Method | Path               |
|--------|--------|--------------------|
| list   | GET    | /v{n}/{resource}   |
| create | POST   | /v{n}/{resource}   |
| get    | GET    | /v{n}/{resource}/{id} |
| update | PATCH  | /v{n}/{resource}/{id} |
| delete | DELETE | /v{n}/{resource}/{id} |

For custom endpoints, provide `method:` and `path:` explicitly.

### Valid Keys per Endpoint Type

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

Key details:
- `auth`: list of role names from your auth config, or `owner` (matches record creator)
- `pagination`: `cursor` (default) or `offset` — no other values
- `cache`: `{ ttl: <seconds> }`
- `controller`: `{ before: <fn_name> }` and/or `{ after: <fn_name> }` — fn in `resources/<name>.controller.rs`
- `input`: list of field names from `schema:` — not field definitions
- `sort`: list of field names that clients can sort by
- `filters`: list of field names that clients can filter on
- `search`: list of string/text field names for full-text search

---

## 5. Controller Pattern

Reference a controller in an endpoint:

```yaml
endpoints:
  create:
    auth: [admin]
    input: [email, name, org_id]
    controller: { before: validate_org }
```

The function lives in `resources/<resource_name>.controller.rs` (same directory as the YAML):

```rust
// resources/users.controller.rs
use shaperail_runtime::ControllerContext;

// before hook — return Err("message") to abort with HTTP 422
pub async fn validate_org(ctx: &mut ControllerContext) -> Result<(), String> {
    let org_id = ctx.input["org_id"].as_str().ok_or("org_id required")?;
    if org_id.is_empty() {
        return Err("org_id must not be empty".into());
    }
    Ok(())
}

// after hook — ctx.output has the created/updated record
pub async fn notify_team(ctx: &ControllerContext) -> Result<(), String> {
    let _id = &ctx.output["id"];
    // fire side effects here
    Ok(())
}
```

`ControllerContext` fields:
| Field       | Type                  | Available in   | Description                              |
|-------------|-----------------------|----------------|------------------------------------------|
| input       | serde_json::Value     | before + after | Request body (before) / saved record (after) |
| output      | serde_json::Value     | after only     | The record returned by the operation     |
| user_id     | Option<uuid::Uuid>    | before + after | Authenticated user, None if no auth      |
| tenant_id   | Option<uuid::Uuid>    | before + after | Current tenant, None if no multi-tenancy |
| resource    | &str                  | before + after | Resource name (e.g., "users")            |

---

## 6. Relations

```yaml
relations:
  # belongs_to — this resource holds the foreign key
  org:     { resource: organizations, type: belongs_to, key: org_id }

  # has_many — the other resource holds the foreign key
  posts:   { resource: posts, type: has_many, foreign_key: author_id }

  # has_one — same as has_many but returns a single record
  profile: { resource: profiles, type: has_one, foreign_key: user_id }
```

Rules:
- `belongs_to` requires `key:` — the FK field name **on this resource's schema**
- `has_many` / `has_one` require `foreign_key:` — the FK field name **on the related resource**
- `resource:` must exactly match the `resource:` name in the related YAML file
- Relations do NOT auto-create schema fields — declare the FK field in `schema:` explicitly

---

## 7. Indexes

```yaml
indexes:
  - fields: [org_id, role]           # composite index
  - fields: [email], unique: true    # unique constraint index
  - fields: [created_at], order: desc # descending order index
```

- `fields`: list of field names from `schema:` (min 1)
- `unique`: bool (optional, default false)
- `order`: `asc` or `desc` (optional, default asc)

---

## 8. Do's and Don'ts

| Rule                        | Correct                                              | Wrong                                    |
|-----------------------------|------------------------------------------------------|------------------------------------------|
| Top-level key               | `resource: users`                                    | `name: users`                            |
| Enum field                  | `{ type: enum, values: [admin, member] }`            | `{ type: enum }`                         |
| Array field                 | `{ type: array, items: string }`                     | `{ type: array }`                        |
| Soft delete schema          | `deleted_at: { type: timestamp, nullable: true }` + `soft_delete: true` on delete endpoint | `soft_delete: true` alone |
| Foreign key reference       | `ref: organizations.id`                              | `ref: organizations`                     |
| FK field type               | `{ type: uuid, ref: organizations.id }`              | `{ type: string, ref: organizations.id }` |
| Pagination value            | `pagination: cursor` or `pagination: offset`         | `pagination: page`                       |
| Input format                | `input: [email, name, role]`                         | `input: { email: ..., name: ... }`       |
| Tenant key field            | `tenant_key: org_id` + `org_id: { type: uuid, required: true }` in schema | `tenant_key: org_id` without schema field |
| Controller reference        | `controller: { before: validate_org }`               | `controller: { before: validate_org.rs }` |
| Relation FK on belongs_to   | `{ type: belongs_to, key: org_id }`                  | `{ type: belongs_to, foreign_key: org_id }` |
| Relation FK on has_many     | `{ type: has_many, foreign_key: user_id }`           | `{ type: has_many, key: user_id }`       |

---

## 9. Common Patterns

### Basic CRUD Resource
```yaml
resource: products
version: 1
schema:
  id:          { type: uuid, primary: true, generated: true }
  name:        { type: string, min: 1, max: 200, required: true }
  price:       { type: float, min: 0, required: true }
  active:      { type: boolean, default: true }
  created_at:  { type: timestamp, generated: true }
  updated_at:  { type: timestamp, generated: true }
endpoints:
  list:   { auth: [member, admin] }
  get:    { auth: [member, admin] }
  create: { auth: [admin], input: [name, price, active] }
  update: { auth: [admin], input: [name, price, active] }
  delete: { auth: [admin] }
```

### User Resource with Auth + Roles
```yaml
resource: users
version: 1
schema:
  id:         { type: uuid, primary: true, generated: true }
  email:      { type: string, format: email, unique: true, required: true }
  name:       { type: string, min: 1, max: 200, required: true }
  role:       { type: enum, values: [admin, member], default: member }
  org_id:     { type: uuid, ref: organizations.id, required: true }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }
endpoints:
  list:   { auth: [admin], filters: [role, org_id], search: [name, email] }
  get:    { auth: [admin, owner] }
  create: { auth: [admin], input: [email, name, role, org_id] }
  update: { auth: [admin, owner], input: [name, role] }
  delete: { auth: [admin] }
relations:
  organization: { resource: organizations, type: belongs_to, key: org_id }
```

### Soft Delete
```yaml
schema:
  ...
  deleted_at: { type: timestamp, nullable: true }
endpoints:
  delete: { auth: [admin], soft_delete: true }
```

### Multi-Tenant Resource
```yaml
resource: projects
version: 1
tenant_key: org_id
schema:
  id:     { type: uuid, primary: true, generated: true }
  org_id: { type: uuid, ref: organizations.id, required: true }
  name:   { type: string, required: true }
endpoints:
  list:   { auth: [member, admin] }
  create: { auth: [admin], input: [name] }
```

### List with Caching + Filtering + Cursor Pagination
```yaml
endpoints:
  list:
    auth: [member, admin]
    filters: [status, category_id]
    search: [title, description]
    sort: [created_at, title]
    pagination: cursor
    cache: { ttl: 30 }
```

### Create with Controller + Events + Jobs
```yaml
endpoints:
  create:
    auth: [admin]
    input: [email, name, role]
    controller: { before: validate_email, after: send_notifications }
    events: [user.created]
    jobs: [send_welcome_email]
```

---

## 10. Error Code Quick Reference

Run `shaperail check --json` to get structured errors with fix suggestions.

| Code  | Trigger                                | Fix                                                             |
|-------|----------------------------------------|-----------------------------------------------------------------|
| SR001 | resource name empty                    | Add `resource: <name>` (snake_case plural)                     |
| SR002 | version is 0 or missing                | Set `version: 1`                                               |
| SR003 | schema has no fields                   | Add at least one field                                          |
| SR004 | no primary key                         | Add `primary: true` to one field (typically `id`)              |
| SR005 | multiple primary keys                  | Remove `primary: true` from all but one field                  |
| SR010 | enum field missing values              | Add `values: [a, b, c]` to the field                           |
| SR011 | non-enum field has values              | Change `type: enum` or remove `values:`                        |
| SR012 | ref on non-uuid field                  | Change field type to `uuid`                                    |
| SR013 | ref not in resource.field format       | Use `ref: resource_name.field_name` (e.g., `organizations.id`) |
| SR014 | array field missing items              | Add `items: string` (or other type)                            |
| SR015 | format on non-string field             | Remove `format:` or change type to `string`                    |
| SR016 | primary key not generated              | Add `generated: true` and `required: true` to the pk field     |
| SR020 | tenant_key field not in schema         | Add the field to `schema:`                                      |
| SR021 | tenant_key field not uuid+required     | Set field to `{ type: uuid, required: true }`                  |
| SR030 | controller path not found              | Path is relative to resources/, no `.rs` extension             |
| SR031 | controller before function not found   | Check function name matches in `.controller.rs`                |
| SR032 | controller after function not found    | Check function name matches in `.controller.rs`                |
| SR033 | WASM controller path invalid           | Use `wasm:path/to/plugin.wasm` prefix                          |
| SR035 | events on unsupported endpoint type    | Remove `events:` — only valid on create/update/delete          |
| SR036 | jobs on unsupported endpoint type      | Remove `jobs:` — only valid on create/update/delete            |
| SR040 | input/filter/search/sort field missing | Add field to `schema:` or fix the field name                   |
| SR041 | soft_delete without deleted_at         | Add `deleted_at: { type: timestamp, nullable: true }` to schema |
| SR050 | upload on non-create endpoint          | Move `upload:` to a create endpoint                            |
| SR051 | upload missing field name              | Add `field: <name>` to upload config                           |
| SR052 | upload field not in schema             | Add the upload field to `schema:`                              |
| SR053 | upload field wrong type                | Change field type to `string`                                  |
| SR054 | upload missing max_size_mb             | Add `max_size_mb: 10` to upload config                         |
| SR060 | relation missing resource name         | Add `resource: <name>` to relation                             |
| SR061 | belongs_to missing key                 | Add `key: <field_name>` (FK field on this resource)            |
| SR062 | has_many/has_one missing foreign_key   | Add `foreign_key: <field_name>` (FK on the related resource)   |
| SR070 | index has no fields                    | Add at least one field name to `fields:`                       |
| SR071 | index field not in schema              | Fix field name to match a `schema:` field                      |
| SR072 | index order invalid                    | Use `order: asc` or `order: desc`                              |

---

## 11. CLI Reference

```bash
shaperail init <name>                   # scaffold new project
shaperail serve                         # start dev server (hot reload)
shaperail generate                      # run codegen for all resources
shaperail check [path] [--json]         # validate with structured fix suggestions
shaperail explain <file>                # dry-run: show routes, table, relations
shaperail diff                          # show what codegen would change
shaperail llm-context [--resource <n>] [--json]  # dump project context for LLM
shaperail migrate                       # apply pending SQL migrations
shaperail routes                        # list all routes with auth requirements
shaperail export openapi                # output OpenAPI 3.1 spec
shaperail export json-schema            # output JSON Schema for resource YAML
shaperail resource create <name> [--archetype basic|user|content|tenant|lookup]
```
```

- [ ] **Step 2: Verify the file was created**

Run: `wc -l docs/llm-guide.md`  
Expected: ~250–320 lines

- [ ] **Step 3: Commit**

```bash
git add docs/llm-guide.md
git commit -m "docs: add llm-guide.md — single-file LLM context for Shaperail"
```

---

## Task 2: Create `docs/llm-reference.md`

**Files:**
- Create: `docs/llm-reference.md`

- [ ] **Step 1: Write `docs/llm-reference.md`**

Create `/Users/Mahin/Desktop/shaperail/docs/llm-reference.md` with this exact content:

```markdown
# Shaperail Quick Reference

Terse lookup tables. For patterns and examples, see [llm-guide.md](llm-guide.md).

---

## Field Types

| Type      | Required sub-keys | Notes                                      |
|-----------|------------------|--------------------------------------------|
| uuid      | —                | Use for PKs and FKs                        |
| string    | —                | Supports format, min, max                  |
| integer   | —                | Supports min, max, default                 |
| float     | —                | Supports min, max, default                 |
| boolean   | —                | Supports default                           |
| timestamp | —                | Use generated:true for auto-timestamps     |
| enum      | values           | values is required                         |
| json      | —                | Unstructured JSON blob                     |
| array     | items            | items type is required                     |

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

| Type       | Required key | Description                                   |
|------------|-------------|-----------------------------------------------|
| belongs_to | key         | FK is on **this** resource                     |
| has_many   | foreign_key | FK is on the **other** resource, returns list  |
| has_one    | foreign_key | FK is on the **other** resource, returns one   |

## Config Keys (`shaperail.config.yaml`)

| Key        | Required | Description                                   |
|------------|----------|-----------------------------------------------|
| project    | ✓        | Project name string                           |
| port       |          | HTTP port (default 3000)                      |
| workers    |          | `auto` or integer                             |
| database   |          | Single DB: `type`, `host`, `port`, `name`     |
| databases  |          | Multi-DB map: `engine` (postgres/mysql/sqlite/mongodb), `url` |
| cache      |          | Redis: `url`                                  |
| auth       |          | `provider: jwt`, `secret_env: JWT_SECRET`     |
| storage    |          | `provider: s3/gcs/azure/local`, `bucket`      |
| logging    |          | `level`, `format: json/text`                  |
| events     |          | `backend: redis`                              |
| protocols  |          | List: `[rest, graphql, grpc]`                 |

## CLI Commands

| Command                               | Description                                          |
|---------------------------------------|------------------------------------------------------|
| `shaperail init <name>`               | Scaffold new project                                 |
| `shaperail serve [--port N]`          | Start dev server with hot reload                     |
| `shaperail generate`                  | Run codegen for all resources                        |
| `shaperail check [path] [--json]`     | Validate with structured fix suggestions             |
| `shaperail explain <file>`            | Show routes, table schema, relations                 |
| `shaperail diff`                      | Show codegen changes (dry run)                       |
| `shaperail llm-context [--resource N] [--json]` | Dump project context for LLM          |
| `shaperail migrate [--rollback]`      | Apply or rollback SQL migrations                     |
| `shaperail seed [path]`               | Load fixture YAML into database                      |
| `shaperail routes`                    | List routes with auth requirements                   |
| `shaperail export openapi`            | Output OpenAPI 3.1 spec                              |
| `shaperail export sdk --lang ts`      | Generate TypeScript SDK                              |
| `shaperail export json-schema`        | Output JSON Schema for resource YAML                 |
| `shaperail resource create <name> [--archetype basic\|user\|content\|tenant\|lookup]` | Scaffold resource |
| `shaperail doctor`                    | Check system dependencies                            |

## Archetypes

| Archetype | Fields included                                                |
|-----------|----------------------------------------------------------------|
| basic     | id, created_at, updated_at                                    |
| user      | id, email, name, role, password_hash, created_at, updated_at  |
| content   | id, title, body, status, author_id, created_at, updated_at    |
| tenant    | id, name, plan, created_at, updated_at (+ tenant isolation)   |
| lookup    | id, code, label, active, sort_order                           |

## Error Codes

| Code  | Trigger                              | Fix                                           |
|-------|--------------------------------------|-----------------------------------------------|
| SR001 | Empty resource name                  | Add `resource: <name>`                        |
| SR002 | Version < 1                          | Set `version: 1`                              |
| SR003 | Empty schema                         | Add at least one field                        |
| SR004 | No primary key                       | Add `primary: true` to one field              |
| SR005 | Multiple primary keys                | Remove `primary: true` from extras            |
| SR010 | Enum missing values                  | Add `values: [a, b]`                          |
| SR011 | Values on non-enum                   | Change type to `enum` or remove `values:`     |
| SR012 | ref on non-uuid field                | Change type to `uuid`                         |
| SR013 | ref wrong format                     | Use `ref: resource.field`                     |
| SR014 | Array missing items                  | Add `items: string`                           |
| SR015 | format on non-string                 | Remove or change type to `string`             |
| SR016 | PK not generated                     | Add `generated: true, required: true`         |
| SR020 | tenant_key field absent              | Add field to `schema:`                        |
| SR021 | tenant_key field wrong type          | Set `{ type: uuid, required: true }`          |
| SR040 | input/filter/search/sort field absent | Add to `schema:` or fix name                 |
| SR041 | soft_delete without deleted_at       | Add `deleted_at: { type: timestamp, nullable: true }` |
| SR060 | Relation missing resource            | Add `resource: <name>`                        |
| SR061 | belongs_to missing key               | Add `key: <field>`                            |
| SR062 | has_many/has_one missing foreign_key | Add `foreign_key: <field>`                    |
| SR070 | Index fields empty                   | Add at least one field                        |
| SR071 | Index field not in schema            | Fix field name                                |
| SR072 | Index order invalid                  | Use `asc` or `desc`                           |
```

- [ ] **Step 2: Commit**

```bash
git add docs/llm-reference.md
git commit -m "docs: add llm-reference.md — machine-optimized quick lookup tables"
```

---

## Task 3: Implement `shaperail llm-context` CLI Command

**Files:**
- Create: `shaperail-cli/src/commands/llm_context.rs`
- Modify: `shaperail-cli/src/commands/mod.rs` (add `pub mod llm_context;`)
- Modify: `shaperail-cli/src/main.rs` (add `LlmContext` command variant + dispatch)

- [ ] **Step 1: Write the failing test first**

Add this test to the bottom of `shaperail-cli/src/commands/llm_context.rs` (create the file with just the test module first):

```rust
// shaperail-cli/src/commands/llm_context.rs
#[cfg(test)]
mod tests {
    use super::*;
    use shaperail_core::{DatabaseConfig, ProjectConfig, WorkerCount};

    fn make_config(project: &str) -> ProjectConfig {
        ProjectConfig {
            project: project.to_string(),
            port: 3000,
            workers: WorkerCount::Auto,
            database: Some(DatabaseConfig {
                db_type: "postgresql".to_string(),
                host: "localhost".to_string(),
                port: 5432,
                name: "test_db".to_string(),
                pool_size: 5,
            }),
            databases: None,
            cache: None,
            auth: None,
            storage: None,
            logging: None,
            events: None,
            protocols: vec!["rest".to_string()],
            graphql: None,
            grpc: None,
        }
    }

    #[test]
    fn db_summary_single_db() {
        let config = make_config("my-app");
        assert_eq!(db_summary(&config), "postgresql");
    }

    #[test]
    fn auth_summary_no_auth() {
        let config = make_config("my-app");
        assert_eq!(auth_summary(&config), "none");
    }

    #[test]
    fn auth_summary_with_jwt() {
        let mut config = make_config("my-app");
        config.auth = Some(shaperail_core::AuthConfig {
            provider: "jwt".to_string(),
            secret_env: "JWT_SECRET".to_string(),
        });
        assert_eq!(auth_summary(&config), "jwt");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails (module doesn't exist yet)**

```bash
cargo test -p shaperail-cli 2>&1 | head -20
```

Expected: compile error — `llm_context` module not declared.

- [ ] **Step 3: Write the full implementation**

Replace the file with the complete implementation:

```rust
// shaperail-cli/src/commands/llm_context.rs
use shaperail_core::{ProjectConfig, RelationType};

pub fn run(resource_filter: Option<&str>, json_output: bool) -> i32 {
    let config = match super::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    let resources = match super::load_all_resources() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    let mut filtered: Vec<_> = resources.iter().collect();
    if let Some(name) = resource_filter {
        filtered.retain(|r| r.resource == name);
        if filtered.is_empty() {
            eprintln!("No resource named '{name}' found in resources/");
            return 1;
        }
    }
    filtered.sort_by(|a, b| a.resource.cmp(&b.resource));

    let all_diags: Vec<_> = filtered
        .iter()
        .flat_map(|r| {
            shaperail_codegen::diagnostics::diagnose_resource(r)
                .into_iter()
                .map(|d| (r.resource.clone(), d))
        })
        .collect();

    if json_output {
        print_json(&config, &filtered, &all_diags);
    } else {
        print_markdown(&config, &filtered, &all_diags);
    }
    0
}

fn db_summary(config: &ProjectConfig) -> String {
    if let Some(ref dbs) = config.databases {
        let mut engines: Vec<String> = dbs
            .values()
            .map(|d| format!("{:?}", d.engine).to_lowercase())
            .collect();
        engines.sort();
        engines.dedup();
        engines.join(", ")
    } else if let Some(ref db) = config.database {
        db.db_type.clone()
    } else {
        "unknown".into()
    }
}

fn auth_summary(config: &ProjectConfig) -> String {
    config
        .auth
        .as_ref()
        .map(|a| a.provider.clone())
        .unwrap_or_else(|| "none".into())
}

fn print_markdown(
    config: &ProjectConfig,
    resources: &[&shaperail_core::ResourceDefinition],
    diags: &[(String, shaperail_codegen::diagnostics::Diagnostic)],
) {
    println!("# Project: {}", config.project);
    println!(
        "Database: {} | Auth: {} | Port: {}",
        db_summary(config),
        auth_summary(config),
        config.port
    );
    println!();
    println!("## Resources ({})", resources.len());
    println!();

    for rd in resources {
        println!("### {} (v{})", rd.resource, rd.version);

        // Fields — one compact line
        let field_strs: Vec<String> = rd
            .schema
            .iter()
            .map(|(name, field)| {
                let mut parts = vec![field.field_type.to_string()];
                if field.primary {
                    parts.push("pk".into());
                }
                if field.generated {
                    parts.push("generated".into());
                }
                if field.required {
                    parts.push("required".into());
                }
                if field.unique {
                    parts.push("unique".into());
                }
                if let Some(ref r) = field.reference {
                    parts.push(format!("fk→{r}"));
                }
                if let Some(ref vals) = field.values {
                    parts.push(format!("[{}]", vals.join(",")));
                }
                if let Some(ref def) = field.default {
                    parts.push(format!("default:{def}"));
                }
                format!("{name}({})", parts.join(","))
            })
            .collect();
        println!("Fields: {}", field_strs.join(", "));

        // Endpoints
        if let Some(ref eps) = rd.endpoints {
            let mut ep_list: Vec<(String, String)> = eps
                .iter()
                .map(|(action, ep)| {
                    let auth_str = match &ep.auth {
                        Some(a) => format!("{a}"),
                        None => "none".into(),
                    };
                    (action.clone(), format!("{action}[{auth_str}]"))
                })
                .collect();
            ep_list.sort_by(|a, b| a.0.cmp(&b.0));
            println!(
                "Endpoints: {}",
                ep_list
                    .iter()
                    .map(|(_, s)| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        // Relations
        if let Some(ref rels) = rd.relations {
            let mut rel_list: Vec<(String, String)> = rels
                .iter()
                .map(|(name, rel)| {
                    let kind = match rel.relation_type {
                        RelationType::BelongsTo => "belongs_to",
                        RelationType::HasMany => "has_many",
                        RelationType::HasOne => "has_one",
                    };
                    (name.clone(), format!("{name}({kind}→{})", rel.resource))
                })
                .collect();
            rel_list.sort_by(|a, b| a.0.cmp(&b.0));
            println!(
                "Relations: {}",
                rel_list
                    .iter()
                    .map(|(_, s)| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        // Cache
        if let Some(ref eps) = rd.endpoints {
            let mut cached: Vec<String> = eps
                .iter()
                .filter_map(|(action, ep)| {
                    ep.cache.as_ref().map(|c| format!("{action}({}s)", c.ttl))
                })
                .collect();
            if !cached.is_empty() {
                cached.sort();
                println!("Cache: {}", cached.join(", "));
            }
        }

        // Tenant key
        if let Some(ref tk) = rd.tenant_key {
            println!("Tenant key: {tk}");
        }

        // Soft delete
        let has_soft_delete = rd
            .endpoints
            .as_ref()
            .map(|eps| eps.values().any(|ep| ep.soft_delete))
            .unwrap_or(false);
        if has_soft_delete {
            println!("Soft delete: enabled");
        }

        // Per-resource validation errors
        let res_diags: Vec<_> = diags
            .iter()
            .filter(|(r, _)| r == &rd.resource)
            .collect();
        if !res_diags.is_empty() {
            println!(
                "Errors: {}",
                res_diags
                    .iter()
                    .map(|(_, d)| format!("[{}] {}", d.code, d.error))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }

        println!();
    }

    // Validation summary
    if diags.is_empty() {
        println!("## Validation\n✓ No errors found");
    } else {
        println!("## Validation\n⚠ {} issue(s):", diags.len());
        for (resource, d) in diags {
            println!("  {resource} [{}] {} → {}", d.code, d.error, d.fix);
        }
    }
}

fn print_json(
    config: &ProjectConfig,
    resources: &[&shaperail_core::ResourceDefinition],
    diags: &[(String, shaperail_codegen::diagnostics::Diagnostic)],
) {
    let resource_list: Vec<serde_json::Value> = resources
        .iter()
        .map(|rd| {
            let fields: Vec<serde_json::Value> = rd
                .schema
                .iter()
                .map(|(name, field)| {
                    serde_json::json!({
                        "name": name,
                        "type": field.field_type.to_string(),
                        "primary": field.primary,
                        "generated": field.generated,
                        "required": field.required,
                        "unique": field.unique,
                        "ref": field.reference,
                        "values": field.values,
                        "default": field.default,
                    })
                })
                .collect();

            let endpoints: Vec<serde_json::Value> = rd
                .endpoints
                .as_ref()
                .map(|eps| {
                    eps.iter()
                        .map(|(action, ep)| {
                            serde_json::json!({
                                "action": action,
                                "method": ep.method(),
                                "path": format!("/v{}{}", rd.version, ep.path()),
                                "auth": ep.auth.as_ref().map(|a| format!("{a}")),
                                "cache_ttl": ep.cache.as_ref().map(|c| c.ttl),
                                "soft_delete": ep.soft_delete,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let relations: Vec<serde_json::Value> = rd
                .relations
                .as_ref()
                .map(|rels| {
                    rels.iter()
                        .map(|(name, rel)| {
                            let kind = match rel.relation_type {
                                RelationType::BelongsTo => "belongs_to",
                                RelationType::HasMany => "has_many",
                                RelationType::HasOne => "has_one",
                            };
                            serde_json::json!({
                                "name": name,
                                "type": kind,
                                "resource": rel.resource,
                                "key": rel.key,
                                "foreign_key": rel.foreign_key,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let errors: Vec<serde_json::Value> = diags
                .iter()
                .filter(|(r, _)| r == &rd.resource)
                .map(|(_, d)| {
                    serde_json::json!({
                        "code": d.code,
                        "error": d.error,
                        "fix": d.fix,
                    })
                })
                .collect();

            serde_json::json!({
                "name": rd.resource,
                "version": rd.version,
                "tenant_key": rd.tenant_key,
                "fields": fields,
                "endpoints": endpoints,
                "relations": relations,
                "errors": errors,
            })
        })
        .collect();

    let output = serde_json::json!({
        "project": {
            "name": config.project,
            "database": db_summary(config),
            "auth": auth_summary(config),
            "port": config.port,
        },
        "resources": resource_list,
        "validation": {
            "total_errors": diags.len(),
            "clean": diags.is_empty(),
        },
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use shaperail_core::{DatabaseConfig, ProjectConfig, WorkerCount};

    fn make_config(project: &str) -> ProjectConfig {
        ProjectConfig {
            project: project.to_string(),
            port: 3000,
            workers: WorkerCount::Auto,
            database: Some(DatabaseConfig {
                db_type: "postgresql".to_string(),
                host: "localhost".to_string(),
                port: 5432,
                name: "test_db".to_string(),
                pool_size: 5,
            }),
            databases: None,
            cache: None,
            auth: None,
            storage: None,
            logging: None,
            events: None,
            protocols: vec!["rest".to_string()],
            graphql: None,
            grpc: None,
        }
    }

    #[test]
    fn db_summary_single_db() {
        let config = make_config("my-app");
        assert_eq!(db_summary(&config), "postgresql");
    }

    #[test]
    fn auth_summary_no_auth() {
        let config = make_config("my-app");
        assert_eq!(auth_summary(&config), "none");
    }

    #[test]
    fn auth_summary_with_jwt() {
        let mut config = make_config("my-app");
        config.auth = Some(shaperail_core::AuthConfig {
            provider: "jwt".to_string(),
            secret_env: "JWT_SECRET".to_string(),
        });
        assert_eq!(auth_summary(&config), "jwt");
    }
}
```

- [ ] **Step 4: Add `pub mod llm_context;` to `shaperail-cli/src/commands/mod.rs`**

Add this line in alphabetical order in the pub mod list (after `jobs_status`, before `migrate`):

```rust
pub mod llm_context;
```

- [ ] **Step 5: Add `LlmContext` variant to `Commands` enum in `shaperail-cli/src/main.rs`**

Add after the `JobsStatus` variant (around line 99), before `Resource`:

```rust
    /// Dump a project-aware context summary for LLM consumption
    #[command(name = "llm-context")]
    LlmContext {
        /// Filter to a single resource by name
        #[arg(short, long)]
        resource: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
```

- [ ] **Step 6: Add dispatch arm in `main()` in `shaperail-cli/src/main.rs`**

Add after the `Commands::JobsStatus` arm (around line 183), before `Commands::Resource`:

```rust
        Commands::LlmContext { resource, json } => {
            commands::llm_context::run(resource.as_deref(), json)
        }
```

- [ ] **Step 7: Check it compiles**

```bash
cargo build -p shaperail-cli 2>&1
```

Expected: compiles with no errors. Fix any type mismatches before continuing.

- [ ] **Step 8: Run the tests**

```bash
cargo test -p shaperail-cli -- llm_context 2>&1
```

Expected:
```
test commands::llm_context::tests::db_summary_single_db ... ok
test commands::llm_context::tests::auth_summary_no_auth ... ok
test commands::llm_context::tests::auth_summary_with_jwt ... ok
```

- [ ] **Step 9: Run clippy**

```bash
cargo clippy -p shaperail-cli -- -D warnings 2>&1
```

Expected: no warnings. Fix any lint issues.

- [ ] **Step 10: Commit**

```bash
git add shaperail-cli/src/commands/llm_context.rs \
        shaperail-cli/src/commands/mod.rs \
        shaperail-cli/src/main.rs
git commit -m "feat(shaperail-cli): add llm-context command for project-aware LLM context dumps"
```

---

## Task 4: Doc Audit — Fix Existing Docs

**Files:**
- Modify: files in `docs/` that contain anti-patterns (identified during scan)

This task is a systematic scan of the 38 docs for LLM anti-patterns. Fix in-place; do not create new files.

**What to scan for and fix:**

1. **Multiple equivalent syntaxes** — any "you can also...", "alternatively...", or "another way to..." phrasing. Remove the alternative; keep only the canonical form.

2. **Implicit behavior without YAML** — any sentence like "the framework automatically..." or "by default, Shaperail will..." without showing the YAML that enables it. Add the YAML example.

3. **Contradictions with `agent_docs/resource-format.md`** — `agent_docs/resource-format.md` is the source of truth. Any doc that shows different field names, key names, or syntax than what the spec shows must be corrected.

4. **`name:` key instead of `resource:`** — any YAML example showing `name: users` instead of `resource: users`. Fix to `resource:`.

5. **Soft delete without deleted_at in schema** — any example showing `soft_delete: true` without `deleted_at: { type: timestamp, nullable: true }` in the schema.

6. **Relation key confusion** — any `belongs_to` example using `foreign_key:` or any `has_many` example using `key:`. Fix to match the spec.

- [ ] **Step 1: Scan docs for anti-patterns**

Run these greps to find files needing fixes:

```bash
grep -rl "alternatively\|you can also\|another way\|also works" docs/ --include="*.md"
grep -rl "name: " docs/ --include="*.md" | xargs grep -l "^name:" 2>/dev/null || true
grep -rn "soft_delete: true" docs/ --include="*.md"
```

- [ ] **Step 2: Fix each flagged file**

For each file flagged above:
- Remove "alternatively" / "you can also" phrasing, keeping only the canonical example
- Fix any YAML showing `name:` at the resource level to `resource:`
- Fix any `soft_delete: true` example to include `deleted_at: { type: timestamp, nullable: true }` in the schema section

- [ ] **Step 3: Cross-check key docs against the spec**

Read these docs and verify their YAML examples match `agent_docs/resource-format.md` exactly:
- `docs/resource-guide.md`
- `docs/controllers.md`
- `docs/auth-and-ownership.md`
- `docs/getting-started.md`
- `docs/guides.md`

Fix any discrepancies found.

- [ ] **Step 4: Commit**

```bash
git add docs/
git commit -m "docs: audit LLM anti-patterns — remove alternatives, fix syntax examples"
```

---

## Task 5: JSON Schema Audit and Publish

**Files:**
- Create: `docs/schema/resource.schema.json`
- Possibly modify: `shaperail-codegen/src/json_schema.rs` (if gaps found)

- [ ] **Step 1: Generate current JSON Schema and inspect it**

```bash
cargo run -p shaperail-cli -- export json-schema 2>&1 | head -100
```

- [ ] **Step 2: Check coverage against `agent_docs/resource-format.md`**

Verify the generated schema includes definitions for all of these top-level keys:
- `resource`, `version`, `db`, `tenant_key`, `schema`, `endpoints`, `relations`, `indexes`

And for field definitions, all options:
- `type`, `primary`, `generated`, `required`, `unique`, `nullable`, `min`, `max`, `format`, `values`, `default`, `ref`, `items`, `sensitive`

If any are missing, they need to be added to `shaperail-codegen/src/json_schema.rs`. The function to modify is `generate_resource_json_schema()` starting at line 8.

- [ ] **Step 3: Publish the schema to `docs/schema/resource.schema.json`**

```bash
mkdir -p docs/schema
cargo run -p shaperail-cli -- export json-schema --output docs/schema/resource.schema.json
```

- [ ] **Step 4: Verify the output file**

```bash
wc -c docs/schema/resource.schema.json
```

Expected: > 5000 bytes (the schema is comprehensive).

- [ ] **Step 5: Add schema reference to `docs/llm-guide.md`**

Add this line at the top of `docs/llm-guide.md`, after the first paragraph:

```markdown
**IDE validation:** Add `# yaml-language-server: $schema=https://shaperail.dev/schema/resource.schema.json` to any resource YAML file for inline validation.
```

- [ ] **Step 6: Commit**

```bash
git add docs/schema/resource.schema.json docs/llm-guide.md
git commit -m "docs: publish resource JSON Schema for IDE and LLM YAML validation"
```

---

## Self-Review Notes

**Spec coverage check:**
- Pillar 1 (llm-guide + doc audit): Tasks 1 + 4 ✓
- Pillar 2 (machine-readable artifacts — REFERENCE.md): Task 2 ✓
- Pillar 2 (check --json fix fields): Already implemented in `Diagnostic` struct — no task needed ✓
- Pillar 2 (JSON Schema audit + publish): Task 5 ✓
- Pillar 3 (llm-context command): Task 3 ✓

**Type consistency:**
- `db_summary(config: &ProjectConfig) -> String` — used in both `print_markdown` and `print_json` ✓
- `auth_summary(config: &ProjectConfig) -> String` — same ✓
- `RelationType::BelongsTo/HasMany/HasOne` — imported from `shaperail_core` ✓
- `shaperail_codegen::diagnostics::Diagnostic` — the `fix` field is `String`, `code` is `&'static str` ✓
- `ep.auth.as_ref().map(|a| format!("{a}"))` — matches `explain.rs` pattern for `AuthRule` Display ✓

**Placeholder check:** None found — all steps include exact code or exact commands.
