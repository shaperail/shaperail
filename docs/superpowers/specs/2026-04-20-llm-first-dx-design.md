# LLM-First Developer Experience Initiative
**Date:** 2026-04-20  
**Status:** Approved  
**Scope:** Full initiative — doc audit, machine-readable artifacts, CLI command

---

## Goal

An AI model (Claude, ChatGPT, etc.) working in Shaperail should:
1. Start from a single ~300-line context file and generate valid resource YAML + controllers on the first try
2. On an existing project, run one command to understand the full project state in ~50 lines
3. Self-correct from CLI errors without needing additional documentation

---

## Current State

- Framework: v0.7.0, M01–M19 complete, production-ready
- 38 user docs exist — comprehensive but optimized for humans, not LLMs
- `shaperail check --json` emits structured errors but no `fix:` guidance
- No single canonical reference file for LLM consumption
- No project-aware context dump command

---

## Pillar 1: LLM Guide + Doc Audit

### New File: `docs/llm-guide.md`

A single file (~300 lines) that is the *only* context an LLM needs to start building. Structure:

1. **Resource YAML reference** — every field type (`uuid`, `string`, `integer`, `float`, `boolean`, `timestamp`, `enum`, `json`), every valid option per type (`primary`, `generated`, `required`, `unique`, `min`, `max`, `format`, `values`, `default`, `ref`). Dense, no prose. One canonical form shown — no alternatives.

2. **Endpoint reference** — all endpoint types (`list`, `create`, `update`, `delete`, and custom) with every valid key per type:
   - `list`: `auth`, `filters`, `search`, `pagination`, `cache`, `sort`
   - `create`: `auth`, `input`, `controller`, `events`, `jobs`
   - `update`: `auth`, `input`, `controller`
   - `delete`: `auth`, `soft_delete`, `controller`
   - custom: `method`, `path`, `auth`, `input`, `controller`

3. **Controller API** — `ControllerContext` struct fields, `before`/`after` return types, a complete working template for a `create` controller with before-validation and after-event patterns.

4. **Relations + indexes** — exact syntax for `belongs_to`, `has_many`, `has_one`; composite index syntax.

5. **Do's and Don'ts** — 10 hard rules:
   - Use `resource:` not `name:`
   - `enum` requires `values:` array
   - `soft_delete: true` requires a `deleted_at: { type: timestamp, generated: true }` field
   - `ref:` must be in `resource.field` format
   - `pagination:` must be `cursor` or `offset`
   - Version drives route prefix: `version: 1` → `/v1/...`
   - `input:` lists field names, not field definitions
   - `auth:` takes role names from your auth config, not arbitrary strings
   - `controller:` path is relative to the resource file, no `.rs` extension needed
   - Relations do not auto-create foreign key constraints — declare the field explicitly in `schema:`

6. **Error code table** — SR001–SR072, one line each: trigger condition + fix. This is the LLM's repair manual.

### Doc Audit

Scan all 38 docs in `docs/` for:
- Multiple equivalent syntaxes for the same concept → remove all but the canonical one
- Implicit behavior described without showing the YAML that enables it → add explicit YAML
- Field names that differ from `resource-format.md` (source of truth) → fix to match
- Docs that contradict each other → fix and add cross-reference
- Any "you can also..." or "alternatively..." phrasing → remove the alternative

Fix in-place. No new doc files for audit fixes.

---

## Pillar 2: Machine-Readable Artifacts

### Enhanced `shaperail check --json`

Add a `fix` field to every error object:

```json
{
  "file": "resources/users.yaml",
  "errors": [
    {
      "code": "SR024",
      "field": "schema.role",
      "message": "field 'role' is type enum but has no values",
      "fix": "Add values array: { type: enum, values: [admin, member, viewer] }",
      "doc": "https://shaperail.dev/docs/fields#enum"
    }
  ]
}
```

Every error code in the validator must have a corresponding `fix` string. No error exits without a fix suggestion.

### New File: `docs/REFERENCE.md`

A machine-optimized reference card (~100 lines, no prose):
- All field types and their valid keys (table format)
- All endpoint types and their valid keys (table format)
- All relation types and required keys
- Config file keys (`shaperail.config.yaml`)
- All CLI commands with flags
- Complete error code table (SR001–SR072): code, trigger, fix

This is the quick-lookup companion to `llm-guide.md`. The guide teaches patterns; the reference answers "what keys are valid here?"

### JSON Schema (Existing — Enhance)

`shaperail export json-schema` already works. Ensure:
- Schema covers 100% of resource YAML fields (audit for gaps)
- Schema is published to `docs/schema/resource.schema.json` and committed
- README and `llm-guide.md` reference it so IDEs auto-validate resource files

---

## Pillar 3: `shaperail llm-context` CLI Command

### Purpose

An LLM joining an existing Shaperail project currently needs to read 10–20 files to understand the project state. This command dumps a minimal, accurate summary in ~50–100 lines.

### Usage

```bash
shaperail llm-context                    # full project summary
shaperail llm-context --resource users   # single resource deep-dive
shaperail llm-context --format json      # machine-readable JSON
```

### Markdown Output (default)

```
# Project: my-app (v1.0.0)
Database: postgres | Auth: jwt | Tenancy: disabled

## Resources (3)

### users (v1)
Fields: id(uuid,pk), email(string,unique), name(string), role(enum:[admin,member]), org_id(uuid,fk→organizations.id), created_at, updated_at
Endpoints: list[member,admin], create[admin], update[admin,owner], delete[admin]
Relations: organization(belongs_to), orders(has_many)
Cache: list(60s)
Jobs: send_welcome_email on create

### organizations (v1)
Fields: id(uuid,pk), name(string), plan(enum:[free,pro,enterprise]), created_at
Endpoints: list[admin], create[admin], update[admin], delete[admin]
Relations: members(has_many→users)

### orders (v1)
Fields: id(uuid,pk), user_id(uuid,fk→users.id), total(float), status(enum:[pending,paid,cancelled]), created_at
Endpoints: list[member,admin], create[member], update[admin]
Relations: user(belongs_to)

## Validation
✓ No errors found
```

### JSON Output (`--format json`)

Structured version of the same data — used when an LLM needs to parse the output programmatically or insert it into a system prompt.

### Implementation Notes

- Lives in `shaperail-cli/src/commands/llm_context.rs`
- Reads resource YAML files via existing parser — no new parsing logic
- Runs `shaperail check` logic inline to include validation status
- `--resource` flag filters to one resource and adds controller file path + snippet preview
- Output must be deterministic (sorted by resource name)

---

## Implementation Order

1. **`docs/llm-guide.md`** — highest leverage, can be tested immediately by prompting an LLM
2. **`docs/REFERENCE.md`** — quick win, pure writing
3. **`shaperail check --json` fix fields** — requires touching validator + CLI (~50 lines of Rust)
4. **`shaperail llm-context` command** — new CLI command, uses existing parser
5. **Doc audit** — systematic pass over 38 docs, lowest urgency but important for consistency
6. **JSON Schema audit** — verify coverage, publish to docs/

---

## Success Criteria

- An LLM given only `llm-guide.md` generates a valid, compilable resource + controller on the first attempt for ≥90% of common patterns
- `shaperail check --json` returns a `fix` field for every error code (SR001–SR072)
- `shaperail llm-context` output fits in ≤150 lines for a 5-resource project
- Doc audit eliminates all "alternatively" / "you can also" phrasing from user docs
- JSON Schema validates 100% of fields in `resource-format.md`

---

## Out of Scope

- M20 (Embedded AI + Admin Panel) — separate milestone
- gRPC Update RPC — separate bugfix
- New framework features — this initiative is purely DX/documentation/tooling
