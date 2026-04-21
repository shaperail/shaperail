# LLM Context Files in Scaffold Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 7 LLM context files to `shaperail init` so every coding agent (Claude Code, Cursor, Copilot, Windsurf, Gemini, Codex) has Shaperail syntax knowledge from day one.

**Architecture:** All content is Rust `const &str` literals embedded in the CLI binary — zero network dependency, version-locked, offline-capable. `llm-context.md` holds the full syntax reference once; six thin agent adapter files point to it. Changes are confined to `shaperail-cli/src/commands/init.rs` and `shaperail-cli/tests/cli_tests.rs`.

**Tech Stack:** Rust, `std::fs`, existing `write_file` helper in `init.rs`.

---

## Files Modified

| File | Change |
|------|--------|
| `shaperail-cli/src/commands/init.rs` | Add 2 string constants; add 9 `write_file`/`create_dir_all` calls in `scaffold()` |
| `shaperail-cli/tests/cli_tests.rs` | Add `scaffold_writes_llm_context_files` test |

---

## Task 1: Add string constants and write LLM context files

**Files:**
- Modify: `shaperail-cli/src/commands/init.rs`
- Test: `shaperail-cli/tests/cli_tests.rs`

The `scaffold()` function in `init.rs` already uses a `write_file` helper (line 1436) and `fs::create_dir_all` (line 64). We add two `const &str` values near the top of the file and call `write_file` seven times at the end of `scaffold()`, just before the final `generate` call.

---

- [ ] **Step 1: Write the failing test**

Open `shaperail-cli/tests/cli_tests.rs`. After the last `#[test]` function, add:

```rust
#[test]
fn scaffold_writes_llm_context_files() {
    let tmp = TempDir::new().unwrap();
    let project_name = "llm-test";

    shaperail()
        .args(["init", project_name])
        .current_dir(tmp.path())
        .assert()
        .success();

    let root = tmp.path().join(project_name);

    assert!(root.join("llm-context.md").exists(), "llm-context.md missing");
    assert!(root.join("CLAUDE.md").exists(), "CLAUDE.md missing");
    assert!(root.join("AGENTS.md").exists(), "AGENTS.md missing");
    assert!(root.join("GEMINI.md").exists(), "GEMINI.md missing");
    assert!(
        root.join(".cursor/rules/shaperail.md").exists(),
        ".cursor/rules/shaperail.md missing"
    );
    assert!(
        root.join(".github/copilot-instructions.md").exists(),
        ".github/copilot-instructions.md missing"
    );
    assert!(root.join(".windsurfrules").exists(), ".windsurfrules missing");

    let claude = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap();
    assert!(
        claude.contains("llm-context.md"),
        "CLAUDE.md should reference llm-context.md"
    );

    let ctx = std::fs::read_to_string(root.join("llm-context.md")).unwrap();
    assert!(
        ctx.contains("shaperail context"),
        "llm-context.md should mention shaperail context command"
    );
    assert!(
        ctx.contains("resource:"),
        "llm-context.md should contain resource syntax"
    );
}
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cargo test -p shaperail-cli --test cli_tests scaffold_writes_llm_context_files 2>&1 | tail -20
```

Expected: `FAILED` — the files don't exist yet.

- [ ] **Step 3: Add the `AGENT_ADAPTER_MD` constant to `init.rs`**

Open `shaperail-cli/src/commands/init.rs`. Find the line:

```rust
fn scaffold(project_name: &str, root: &Path) -> Result<(), String> {
```

Insert this constant **above** that function (after the existing `use` statements and any other constants/functions):

```rust
const AGENT_ADAPTER_MD: &str = r#"This is a Shaperail project — a deterministic Rust backend framework driven by YAML resource files.

**Full syntax reference:** See `./llm-context.md`
**Live project state:** Run `shaperail context` to see current resources, schema, and endpoints.

## Key Rules

- One canonical syntax per concept — no aliases, no alternative forms
- `resource:` is the top-level key (not `name:`)
- Resource YAML is the source of truth; never reverse-generate it from code
- Unknown fields in resource YAML cause a loud compile error
- `shaperail check --json` gives structured diagnostics with fix suggestions
"#;
```

- [ ] **Step 4: Add the `LLM_CONTEXT_MD` constant to `init.rs`**

Directly after `AGENT_ADAPTER_MD`, insert:

```rust
const LLM_CONTEXT_MD: &str = r#"# Shaperail LLM Context

This project uses the Shaperail framework (deterministic Rust backend from YAML resources).

**Live project state:** Run `shaperail context` to see the current resources, schema, and endpoints for this specific project.

**IDE validation:** Add `# yaml-language-server: $schema=./resources/.schema.json` as the first line of any resource YAML file for inline validation.

---

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

`format` valid values: `email`, `url`, `uuid` (string fields only).
`ref` format: `resource_name.field_name` — the field must be `type: uuid`.

---

## 3. Field Options Reference

| Option    | Type    | Applies to           | Effect                                                          |
|-----------|---------|----------------------|-----------------------------------------------------------------|
| primary   | bool    | uuid                 | Marks as primary key                                            |
| generated | bool    | uuid, timestamp      | Auto-generated (UUID v7 / NOW()) — do not include in input     |
| required  | bool    | any                  | NOT NULL in DB, required in create/update input                 |
| unique    | bool    | any                  | UNIQUE constraint                                               |
| nullable  | bool    | timestamp, json      | Allows null — overrides `required`                              |
| min       | number  | string, int, float   | Min length (string) or minimum value (numbers)                  |
| max       | number  | string, int, float   | Max length (string) or maximum value (numbers)                  |
| format    | string  | string only          | Validation format: email / url / uuid                           |
| values    | list    | enum only            | Allowed enum values — required when `type: enum`                |
| default   | any     | enum, bool, int      | Default value. For enum must be one of `values`                 |
| ref       | string  | uuid only            | Foreign key reference in `resource.field` format                |
| items     | string  | array only           | Element type — required when `type: array`                      |
| sensitive | bool    | uuid, string         | Redacted in logs, omitted from list responses                   |

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
| rate_limit  | ✓    | ✓      | ✓   | ✓      | ✓      | ✓      |
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

The function lives in `resources/<resource_name>.controller.rs`:

```rust
use shaperail_runtime::ControllerContext;

pub async fn validate_org(ctx: &mut ControllerContext) -> Result<(), String> {
    let org_id = ctx.input["org_id"].as_str().ok_or("org_id required")?;
    if org_id.is_empty() {
        return Err("org_id must not be empty".into());
    }
    Ok(())
}
```

`ControllerContext` fields:
| Field       | Type                  | Available in   | Description                                   |
|-------------|-----------------------|----------------|-----------------------------------------------|
| input       | serde_json::Value     | before + after | Request body (before) / saved record (after)  |
| output      | serde_json::Value     | after only     | The record returned by the operation          |
| user_id     | Option<uuid::Uuid>    | before + after | Authenticated user, None if no auth           |
| tenant_id   | Option<uuid::Uuid>    | before + after | Current tenant, None if no multi-tenancy      |
| resource    | &str                  | before + after | Resource name (e.g., "users")                 |

---

## 6. Relations

```yaml
relations:
  org:     { resource: organizations, type: belongs_to, key: org_id }
  posts:   { resource: posts, type: has_many, foreign_key: author_id }
  profile: { resource: profiles, type: has_one, foreign_key: user_id }
```

Rules:
- `belongs_to` requires `key:` — the FK field name on this resource's schema
- `has_many` / `has_one` require `foreign_key:` — the FK field name on the related resource
- `resource:` must exactly match the `resource:` name in the related YAML file
- Relations do NOT auto-create schema fields — declare the FK field in `schema:` explicitly

---

## 7. Indexes

```yaml
indexes:
  - fields: [org_id, role]
  - fields: [email], unique: true
  - fields: [created_at], order: desc
```

- `fields`: list of field names from `schema:` (min 1)
- `unique`: bool (optional, default false)
- `order`: `asc` or `desc` (optional, default asc)

---

## 8. Do's and Don'ts

| Rule                        | Correct                                              | Wrong                                     |
|-----------------------------|------------------------------------------------------|-------------------------------------------|
| Top-level key               | `resource: users`                                    | `name: users`                             |
| Enum field                  | `{ type: enum, values: [admin, member] }`            | `{ type: enum }`                          |
| Array field                 | `{ type: array, items: string }`                     | `{ type: array }`                         |
| Soft delete schema          | `deleted_at: { type: timestamp, nullable: true }` + `soft_delete: true` on delete endpoint | `soft_delete: true` alone |
| Foreign key reference       | `ref: organizations.id`                              | `ref: organizations`                      |
| FK field type               | `{ type: uuid, ref: organizations.id }`              | `{ type: string, ref: organizations.id }` |
| Pagination value            | `pagination: cursor` or `pagination: offset`         | `pagination: page`                        |
| Input format                | `input: [email, name, role]`                         | `input: { email: ..., name: ... }`        |
| Tenant key field            | `tenant_key: org_id` + `org_id: { type: uuid, required: true }` in schema | `tenant_key: org_id` without schema field |
| Controller reference        | `controller: { before: validate_org }`               | `controller: { before: validate_org.rs }` |
| Relation FK on belongs_to   | `{ type: belongs_to, key: org_id }`                  | `{ type: belongs_to, foreign_key: org_id }` |
| Relation FK on has_many     | `{ type: has_many, foreign_key: user_id }`           | `{ type: has_many, key: user_id }`        |

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
| SR001 | resource name empty                    | Add `resource: <name>` (snake_case plural)                      |
| SR002 | version is 0 or missing                | Set `version: 1`                                                |
| SR003 | schema has no fields                   | Add at least one field                                          |
| SR004 | no primary key                         | Add `primary: true` to one field (typically `id`)               |
| SR005 | multiple primary keys                  | Remove `primary: true` from all but one field                   |
| SR010 | enum field missing values              | Add `values: [a, b, c]` to the field                            |
| SR011 | non-enum field has values              | Change `type: enum` or remove `values:`                         |
| SR012 | ref on non-uuid field                  | Change field type to `uuid`                                     |
| SR013 | ref not in resource.field format       | Use `ref: resource_name.field_name` (e.g., `organizations.id`)  |
| SR014 | array field missing items              | Add `items: string` (or other type)                             |
| SR015 | format on non-string field             | Remove `format:` or change type to `string`                     |
| SR016 | primary key not generated              | Add `generated: true` and `required: true` to the pk field      |
| SR020 | tenant_key field not in schema         | Add the field to `schema:`                                       |
| SR021 | tenant_key field not uuid+required     | Set field to `{ type: uuid, required: true }`                   |
| SR030 | controller path not found              | Path is relative to resources/, no `.rs` extension              |
| SR031 | controller before function not found   | Check function name matches in `.controller.rs`                 |
| SR032 | controller after function not found    | Check function name matches in `.controller.rs`                 |
| SR033 | WASM controller path invalid           | Use `wasm:path/to/plugin.wasm` prefix                           |
| SR035 | events on unsupported endpoint type    | Remove `events:` — only valid on create/update/delete           |
| SR036 | jobs on unsupported endpoint type      | Remove `jobs:` — only valid on create/update/delete             |
| SR040 | input/filter/search/sort field missing | Add field to `schema:` or fix the field name                    |
| SR041 | soft_delete without deleted_at         | Add `deleted_at: { type: timestamp, nullable: true }` to schema |
| SR050 | upload on non-create endpoint          | Move `upload:` to a create endpoint                             |
| SR051 | upload missing field name              | Add `field: <name>` to upload config                            |
| SR052 | upload field not in schema             | Add the upload field to `schema:`                               |
| SR053 | upload field wrong type                | Change field type to `string`                                   |
| SR054 | upload missing max_size_mb             | Add `max_size_mb: 10` to upload config                          |
| SR060 | relation missing resource name         | Add `resource: <name>` to relation                              |
| SR061 | belongs_to missing key                 | Add `key: <field_name>` (FK field on this resource)             |
| SR062 | has_many/has_one missing foreign_key   | Add `foreign_key: <field_name>` (FK on the related resource)    |
| SR070 | index has no fields                    | Add at least one field name to `fields:`                        |
| SR071 | index field not in schema              | Fix field name to match a `schema:` field                       |
| SR072 | index order invalid                    | Use `order: asc` or `order: desc`                               |

---

## 11. CLI Reference

```bash
shaperail init <name>                   # scaffold new project
shaperail serve                         # start dev server (hot reload)
shaperail generate                      # run codegen for all resources
shaperail check [path] [--json]         # validate with structured fix suggestions
shaperail explain <file>                # dry-run: show routes, table, relations
shaperail diff                          # show what codegen would change
shaperail context [--resource <n>] [--json]  # dump project context for LLM
shaperail migrate                       # apply pending SQL migrations
shaperail routes                        # list all routes with auth requirements
shaperail export openapi                # output OpenAPI 3.1 spec
shaperail export json-schema            # output JSON Schema for resource YAML
shaperail resource create <name> [--archetype basic|user|content|tenant|lookup]
```

---

# Shaperail Quick Reference

Terse lookup tables. For patterns and examples, see the guide sections above.

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
| databases  |          | Multi-DB map: `engine`, `url`                  |
| cache      |          | Redis: `url`                                   |
| auth       |          | `provider: jwt`, `secret_env: JWT_SECRET`      |
| storage    |          | `provider: s3/gcs/azure/local`, `bucket`       |
| logging    |          | `level`, `format: json/text`                   |
| events     |          | `backend: redis`                               |
| protocols  |          | List: `[rest, graphql, grpc]`                  |

## Archetypes

| Archetype | Fields included                                                 |
|-----------|-----------------------------------------------------------------|
| basic     | id, created_at, updated_at                                      |
| user      | id, email, name, role, password_hash, created_at, updated_at   |
| content   | id, title, body, status, author_id, created_at, updated_at     |
| tenant    | id, name, plan, created_at, updated_at (+ tenant isolation)    |
| lookup    | id, code, label, active, sort_order                            |
"#;
```

- [ ] **Step 5: Add `write_file` calls for all 7 files in `scaffold()`**

In `shaperail-cli/src/commands/init.rs`, find this block near the end of `scaffold()`:

```rust
    let resources = super::load_all_resources_from(&root.join("resources"))?;
    super::generate::write_generated_modules(&resources, &root.join("generated"))?;

    Ok(())
```

Insert before that block:

```rust
    // LLM context files for coding agents (Claude Code, Cursor, Copilot, Windsurf, Gemini, Codex)
    write_file(&root.join("llm-context.md"), LLM_CONTEXT_MD)?;
    write_file(&root.join("CLAUDE.md"), AGENT_ADAPTER_MD)?;
    write_file(&root.join("AGENTS.md"), AGENT_ADAPTER_MD)?;
    write_file(&root.join("GEMINI.md"), AGENT_ADAPTER_MD)?;
    fs::create_dir_all(root.join(".cursor/rules"))
        .map_err(|e| format!("Failed to create .cursor/rules: {e}"))?;
    write_file(&root.join(".cursor/rules/shaperail.md"), AGENT_ADAPTER_MD)?;
    fs::create_dir_all(root.join(".github"))
        .map_err(|e| format!("Failed to create .github: {e}"))?;
    write_file(&root.join(".github/copilot-instructions.md"), AGENT_ADAPTER_MD)?;
    write_file(&root.join(".windsurfrules"), AGENT_ADAPTER_MD)?;
```

- [ ] **Step 6: Run the failing test to confirm it now passes**

```bash
cargo test -p shaperail-cli --test cli_tests scaffold_writes_llm_context_files 2>&1 | tail -10
```

Expected: `test scaffold_writes_llm_context_files ... ok`

- [ ] **Step 7: Run the full CLI test suite to check for regressions**

```bash
cargo test -p shaperail-cli --test cli_tests 2>&1 | tail -20
```

Expected: all previously passing tests still pass. (Skip `init_scaffold_compiles_with_local_workspace_deps` if it's slow — it's a compile smoke test.)

- [ ] **Step 8: Commit**

```bash
git add shaperail-cli/src/commands/init.rs shaperail-cli/tests/cli_tests.rs
git commit -m "feat(cli): add LLM context files to scaffold for all coding agents"
```

---

## Self-Review

**Spec coverage:**
- ✅ `llm-context.md` written with full guide content → Task 1 Steps 3–4
- ✅ `CLAUDE.md` written → Task 1 Step 5
- ✅ `AGENTS.md` written → Task 1 Step 5
- ✅ `GEMINI.md` written → Task 1 Step 5
- ✅ `.cursor/rules/shaperail.md` written (with `create_dir_all`) → Task 1 Step 5
- ✅ `.github/copilot-instructions.md` written (with `create_dir_all`) → Task 1 Step 5
- ✅ `.windsurfrules` written → Task 1 Step 5
- ✅ Test asserts all 7 files exist and spot-checks content → Task 1 Step 1
- ✅ No network dependency — all content is `const &str` → Task 1 Steps 3–4

**No placeholders found.**

**Content consistency:** `AGENT_ADAPTER_MD` references `llm-context.md` and `shaperail context` — both of which the test asserts are present in the scaffold and the `llm-context.md` content respectively.
