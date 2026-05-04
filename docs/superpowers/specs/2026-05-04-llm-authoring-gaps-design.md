# Closing Five LLM-Authoring Gaps (Design)

**Date:** 2026-05-04
**Status:** Draft
**Target releases:** `v0.14.x` (additive), `v0.15.0` (breaking), `v0.16.0` (additive)

## Background

Shaperail's value proposition is that an LLM can produce a correct backend from a YAML resource with minimal tokens and minimal failure modes. As of v0.14.0, that thesis holds for pure CRUD inside the resource model and breaks down at five recurring boundaries:

1. **Escape-hatch cliff at controllers.** The moment business logic moves from YAML into a Rust controller file, the LLM is writing weakly-typed Rust against a `serde_json::Value` boundary, with no priors from training data to fall back on.
2. **LLM Guide drift.** The guide is the only priming context; nothing today guarantees it matches the installed CLI.
3. **Shallow verification loop.** rustc + sqlx + OpenAPI catch type/schema/route errors but emit no test stubs and no expanded explanation of what a YAML resource compiles to.
4. **Determinism blocks LLM priors.** The LLM has no Shaperail code in training data and must work entirely from the guide and shipped examples.
5. **Priming token cost.** ~4,768 tokens of guide loaded per session, with overlap between `llm-guide.md` and `llm-reference.md`.

This spec proposes a coordinated set of changes across three releases.

## Decisions

| ID | Decision |
|---|---|
| D1 | WASM TS/Python controllers stay supported but are not promoted as a first-class authoring path. The LLM Guide recommends YAML primitives → typed Rust controllers; WASM gets one paragraph in `docs/controllers.md`. |
| D2 | Diagnostic positions come from a position-tracking YAML parser (`saphyr`), not a best-effort second pass. Drift between parser truth and reported positions is unacceptable. |
| D3 | `Context<I, O>` is generic in the runtime; codegen emits concrete per-endpoint type aliases (`UsersCreateContext`) for user-facing controller signatures. |
| D4 | `computed:` grammar is exactly `(<plain text> \| "{" <sibling_field_name> "}")*`. No arithmetic, no conditionals, no function calls, no expression language. Anything more complex stays in a controller. |
| D5 | Recipes (full reference YAMLs) and archetypes (`--archetype` skeletons) are distinct artifacts. Both are kept; recipes ship under `examples/recipes/`. |
| D6 | The LLM Guide is restructured as a small mandatory core (~2k tokens) plus on-demand sections. Content is embedded in the CLI binary via `include_str!` so it cannot drift from installed behavior. |
| D7 | Test scaffolds emit to `tests/` (one-time, user-owned), not `generated/` (regenerated each codegen run). |
| D8 | LLM-Guide claims are guarded by snapshot tests in CI (`tests/llm_guide_claims/`). New claims must add a fixture or fail review. |

---

## Release sequencing

| Train | Cuts | Sections delivered |
|---|---|---|
| `v0.14.x` (patch) | additive | §2.2 Diagnostic spans, §3.2 `explain` extension, §4 recipes that need no new schema |
| `v0.15.0` (breaking minor) | breaking | §1.2 Typed controller boundary, §2.1 `shaperail llm-guide` CLI, §5 Sectioned guide |
| `v0.16.0` (minor) | additive | §1.1 Declarative primitives, §3.1 Test scaffold generator, §4 remaining recipes, §2.3 Guide-claim CI |

Out of scope for all three trains: WASM TS/Python controllers as a first-class path (existing functionality preserved unchanged).

---

## Section 1 — Escape-hatch cliff at controllers

### 1.1 Declarative primitives (v0.16.0, additive)

Audit of controller patterns observable in `examples/incident-platform/resources/incidents.yaml` and the user/content/tenant archetypes. Each row identifies a Rust controller fn that becomes YAML in v0.16.0:

| Controller pattern today | Replacement | Why declarative wins |
|---|---|---|
| `validate_org` (FK existence pre-check) | none — `ref:` already implies it; codegen emits FK + 422 mapping | The check is implicit in the schema; the controller was redundant. |
| `enforce_incident_update` (restrict mutable fields per-state) | field options `immutable: true`, `immutable_after: { <field>: <value> }` | One-line declaration replaces ~10 lines of match logic. |
| `write_incident_audit` (append to audit table after every write) | top-level `audit_log: { table: <name>, fields: [<columns>], actor: user }` where `fields` is the set of source-resource columns whose values are copied into each audit row | Auto-creates audit table + after-hooks on `create`/`update`/`delete`. Every write triggers an audit row; `fields` chooses what data the row carries, not when audits fire. |
| `open_incident` (state machine) | top-level `state: { field: <name>, transitions: { <from>: [<to>...] } }` | Bad transitions rejected with new diagnostic code; LLM cannot accidentally allow `closed → open`. |
| `full_name = first + last` | field option `computed: "{first_name} {last_name}"` | Literal-interpolation grammar only: `{<field_name>}` substitutes the named sibling field; surrounding plain text is preserved verbatim. No arithmetic, no conditionals, no function calls. See D4. |
| Conditional read/write per role | field options `read: [<roles>]`, `write: [<roles>]` | Layered on top of endpoint-level `auth`. Endpoint auth gates the route; field auth gates the column. |
| Input lowercasing/trimming | field option `transform: { input: [<closed enum>] }` | Closed enum: `lowercase`, `uppercase`, `trim`, `normalize_whitespace`. |

**Schema additions** (all additive — older YAMLs continue to parse):

```yaml
schema:
  email:
    type: string
    format: email
    transform: { input: [lowercase, trim] }
    read: [admin, owner]
    write: [admin, owner]
  slug:
    type: string
    immutable: true
  status:
    type: enum
    values: [draft, in_review, approved, archived]
  full_name:
    type: string
    computed: "{first_name} {last_name}"
  first_name: { type: string, required: true }
  last_name: { type: string, required: true }

audit_log:
  table: invoices_audit
  fields: [status, amount_cents]
  actor: user

state:
  field: status
  transitions:
    draft: [in_review, archived]
    in_review: [approved, draft]
    approved: [archived]
```

**New diagnostics** (numbered after existing range):

| Code | Trigger |
|---|---|
| `SR110` | `state.field` is not declared on the resource. |
| `SR111` | `state.transitions` references a value not in the field's `values:` enum. |
| `SR112` | `audit_log.table` collides with a known resource name. |
| `SR113` | `computed:` value contains non-`{field}` syntax (arithmetic, conditionals, etc.). |
| `SR114` | `transform.input` value is not in the closed enum. |
| `SR115` | `immutable_after` references a non-existent field. |

**Acceptance test for §1.1.** Every controller in `examples/` is audited; either it moves to YAML or its README documents why it stays in Rust. The audit table above is updated with the per-example outcome before v0.16.0 ships.

### 1.2 Typed controller boundary (v0.15.0, breaking)

For each `<resource>` × `<action>`, codegen emits a typed input struct, a typed output struct, and a per-endpoint `Context` type alias:

```rust
// generated/users/create.rs
pub struct UsersCreateInput {
    pub email: String,
    pub name: String,
    pub role: Role,
    pub org_id: Uuid,
}

pub struct UsersCreateOutput {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: Role,
    pub org_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub type UsersCreateContext = Context<UsersCreateInput, UsersCreateOutput>;
```

```rust
// resources/users.controller.rs (user code)
use crate::generated::resources::users::{UsersCreateContext, UsersUpdateContext};

pub async fn before_create(ctx: &mut UsersCreateContext) -> Result<(), ShaperailError> {
    ctx.input.email = ctx.input.email.to_lowercase();
    Ok(())
}

pub async fn after_create(ctx: &mut UsersCreateContext) -> Result<(), ShaperailError> {
    if let Some(out) = ctx.output.as_ref() {
        ctx.emit_event("user.created", &out.id);
    }
    Ok(())
}
```

**Runtime type** (in `shaperail-runtime`):

```rust
pub struct Context<I, O> {
    pub input: I,
    pub resource: Option<I>,    // existing row for update/delete; same shape as input
    pub output: Option<O>,      // set in after-hooks
    pub user: Option<AuthUser>,
    pub session: serde_json::Value,
    pub response_extras: serde_json::Value,
    pub path_params: HashMap<String, String>,
    pub headers: HeaderMap,
    pub request_id: String,
    pub db: Arc<PgPool>,
    pub cache: Arc<RedisClient>,
    pub jobs: Arc<JobQueue>,
    pub events: Arc<EventEmitter>,
    pub storage: Arc<StorageBackend>,
}

impl<I, O> Context<I, O> {
    pub fn path_param(&self, name: &str) -> Option<&str> { /* ... */ }
    pub fn set_response_extra<T: Serialize>(&mut self, key: &str, value: T) { /* ... */ }
    pub fn emit_event<T: Serialize>(&self, name: &str, payload: T) { /* ... */ }
}
```

The previous `ControllerContext` (with `serde_json::Value` fields) is removed in v0.15.0. There is no compatibility shim — `feat!:` semantics, breaking minor.

**Migration aid.** v0.15.0 ships `cargo shaperail migrate-controllers`, a codemod that:
- Rewrites `&mut ControllerContext` signatures to the appropriate `&mut <Resource><Action>Context`.
- Replaces `ctx.input.get("x").and_then(Value::as_str)` with `ctx.input.x.as_str()` (or `&ctx.input.x` for owned values).
- Flags any access patterns it cannot rewrite with a TODO comment containing the original line.

The codemod is best-effort; manual review is required. Document in `docs/upgrade-0.15.md`, mirrored in `agent_docs/upgrade-0.15.md`.

### 1.3 WASM authoring (no engineering change)

`wasm-plugins` feature flag stays default-on. `examples/wasm-plugins/` stays. No new tooling.

`docs/controllers.md` gets one paragraph:

> **WASM plugins.** The runtime supports WASM controllers (TS/Python compiled to WASM) via the `wasm-plugins` feature for advanced use cases. For new code, prefer YAML primitives or typed Rust controllers — they're better-typed, faster, and produce smaller binaries. See `examples/wasm-plugins/` for the existing surface.

The LLM Guide does **not** mention WASM controllers in any section. They are discoverable from `docs/controllers.md` only.

---

## Section 2 — LLM Guide drift / no training-data baseline

### 2.1 `shaperail llm-guide` CLI subcommand (v0.15.0)

```
shaperail llm-guide                   # prints the mandatory core (~2k tokens)
shaperail llm-guide --section <name>  # prints one section
shaperail llm-guide --list            # lists section names with token counts
shaperail llm-guide --version         # prints CLI version + content hash
shaperail llm-guide --format json     # structured output (sections as objects)
```

Content embedded via `include_str!` from `shaperail-cli/src/llm_guide/`. Binary's guide and binary's behavior cannot drift apart.

`shaperail init` writes a `CLAUDE.md` template (and `AGENTS.md` and `GEMINI.md` symlinks) containing:

> Run `shaperail llm-guide` at the start of any Shaperail task. Run `shaperail llm-guide --section <topic>` when you need a specific topic (recipes, controllers, multi-tenancy, file-storage, etc.). Do not fetch shaperail.io — the website may be out of sync with the installed CLI.

### 2.2 Diagnostic struct upgrade (v0.14.x, additive)

**Current** (`shaperail-codegen/src/diagnostics.rs`):

```rust
pub struct Diagnostic {
    pub code: &'static str,
    pub error: String,
    pub fix: String,
    pub example: String,
}
```

**New** (additive — JSON consumers tolerate unknown fields, so this is safe in a patch release):

```rust
pub struct Diagnostic {
    pub code: &'static str,
    pub error: String,
    pub fix: String,
    pub example: String,
    pub span: Option<Span>,         // NEW
    pub severity: Severity,         // NEW: Error | Warning | Info (defaults to Error)
    pub doc_url: Option<String>,    // NEW: stable URL into shaperail.io/errors/<code>
}

pub struct Span {
    pub file: PathBuf,
    pub line: u32,        // 1-indexed
    pub col: u32,         // 1-indexed
    pub end_line: u32,
    pub end_col: u32,
}

pub enum Severity { Error, Warning, Info }
```

**Parser swap.** `serde_yaml` does not expose marks. Switch to `saphyr` (actively maintained fork of `yaml-rust`; exposes `Marker { line, col }` for every node). Validation passes in `shaperail-codegen` thread the spans through the diagnostic emitter. `yaml-rust2` was considered but rejected: smaller community, less recent activity.

**JSON output.** `shaperail check --json` already exists; it gains the new fields automatically once the struct is updated. Documented under `docs/cli-reference.md` and mirrored in `agent_docs/codegen-patterns.md`.

**Stable doc URLs.** Every `SR*` code gets a permanent page at `docs/errors/<code>.md` (Jekyll, indexed). The `doc_url` field points at the rendered URL: `https://shaperail.io/errors/SR042.html`. CI fails if a code in the registry has no doc page.

### 2.3 Guide-claim snapshot tests (v0.16.0)

Each LLM Guide section (and each recipe) is paired with a fixture under `tests/llm_guide_claims/<claim_id>/`:

```
tests/llm_guide_claims/
  field_options_ref/
    01_string_min_max/
      input.yaml
      expected_check.txt
      expected_explain.txt
    02_enum_values/
      ...
  recipes/
    file_upload/
      input.yaml
      expected_explain.txt
      expected_routes.txt
```

CI step: for each fixture, run the named CLI subcommand against `input.yaml` and assert output matches `expected_*.txt` byte-for-byte. Any change to the LLM Guide that is not backed by a fixture fails review. Any code change that changes a fixture's output forces an explicit guide update in the same PR.

This converts §2c of the original brief from a one-shot audit into a permanent CI guard.

---

## Section 3 — Shallow verification loop

### 3.1 Test scaffold generator (v0.16.0)

New CLI: `shaperail generate --tests` (also accessible as `shaperail test scaffold [resource]`).

**Output.** `tests/<resource>_test.rs`. Created once per resource; never overwritten if the file exists. (This file is owned by the user — codegen never writes here on subsequent runs. The `generated/` directory stays the regenerated home.)

**Per endpoint, three tests:**
- `<resource>_<action>_happy_path` — sends a valid request, asserts 2xx and shape.
- `<resource>_<action>_auth_failure` — sends without/with-wrong auth, asserts 401/403.
- `<resource>_<action>_validation_failure` — sends malformed input, asserts 422 and SR-coded body.

**Test runtime.** `shaperail_runtime::test_support::TestServer` (already exists behind `test-support` feature). Promote `test-support` to default-on for `--tests`-generated code.

**Staleness mitigation.** Stubs introspect the live `ResourceDefinition` at test-run time for routes and input shape; only assertions are static. A test does not bake the string `"/v1/users"` — it calls `ResourceDefinition::list_route()`.

```rust
// tests/users_test.rs (generated once)
use shaperail_runtime::test_support::{TestServer, fixture};

#[tokio::test]
async fn users_list_happy_path() {
    let server = TestServer::start().await;
    let res = server
        .get(fixture::users().list_route())
        .with_role("member")
        .send()
        .await;
    assert_eq!(res.status(), 200);
    // TODO: add assertions about the response body shape
}

#[tokio::test]
async fn users_list_auth_failure() {
    let server = TestServer::start().await;
    let res = server.get(fixture::users().list_route()).send().await;
    assert!(matches!(res.status().as_u16(), 401 | 403));
}
```

### 3.2 `shaperail explain` extension (v0.14.x, additive)

Today `explain` prints routes, table schema, relations. Extend with:

- **Per-field validation rules** (compact one-liner per field):
  ```
  email: required, format=email, unique, max=200
  name: required, min=1, max=200
  role: enum [admin, member, viewer], default=member
  ```
- **Per-endpoint OpenAPI fragment**: request schema, response schema, status codes, auth scheme — printed in compact YAML/JSON, not full OpenAPI.
- **Indexes** (already partially present — confirm and standardize).
- **`--format json`** for machine consumption. The JSON shape is documented in `docs/cli-reference.md` and stable across patch releases.

The compact format is the contract: `explain` is the LLM's pre-flight check, not a full OpenAPI dump (`shaperail export openapi` is for that).

---

## Section 4 — Determinism blocks priors → recipe library

**Reframe.** Archetypes (`shaperail resource create --archetype <name>`) are skeletons; recipes are full reference YAMLs the LLM is told to imitate. Two distinct artifacts; both kept.

**Recipe location.** `examples/recipes/<name>/`:
```
examples/recipes/
  paginated_list_with_filters/
    resource.yaml
    README.md           # "WHEN to use this pattern" + tradeoffs
    tests/integration.rs
  soft_delete_with_audit/
  file_upload/
  multi_tenant_rls/
  parent_child_with_cascade/
  approval_workflow/
  rate_limited_public/
  idempotent_webhook_receiver/
```

**Recipe → train mapping:**

| Recipe | Depends on | Train |
|---|---|---|
| `paginated_list_with_filters` | existing primitives | v0.14.x |
| `file_upload` | existing `upload:` key | v0.14.x |
| `multi_tenant_rls` | existing `tenant_key` | v0.14.x |
| `rate_limited_public` | existing `rate_limit:` key | v0.14.x |
| `soft_delete_with_audit` | new `audit_log:` (§1.1) | v0.16.0 |
| `parent_child_with_cascade` | new `cascade: soft_delete` relation option (§4.1) | v0.16.0 |
| `approval_workflow` | new `state:` primitive (§1.1) | v0.16.0 |
| `idempotent_webhook_receiver` | new `idempotency_key:` field option (§4.2) | v0.16.0 |

**Recipe metadata (every README):**
- One-line "WHEN to use this" summary (rendered in `--section recipes` index).
- Tradeoffs section ("when not to use this").
- Pointer to the integration test that exercises the recipe.

The LLM Guide core lists recipes by name with one-liners. `shaperail llm-guide --section recipes` prints the full index. `shaperail llm-guide --section recipes/file_upload` prints one recipe's YAML and README verbatim.

### 4.1 New `cascade:` relation option (v0.16.0)

```yaml
relations:
  alerts:
    resource: alerts
    type: has_many
    foreign_key: incident_id
    cascade: soft_delete   # closed enum: none | soft_delete | hard_delete
```

`cascade: soft_delete` (default for parents that themselves have `soft_delete: true` on delete) propagates the tombstone down. `cascade: hard_delete` is rejected if the child resource doesn't allow hard deletes — diagnostic `SR120`.

### 4.2 New `idempotency_key:` field option (v0.16.0)

```yaml
schema:
  request_id:
    type: string
    idempotency_key: true
    max: 200
```

When set on a `create` endpoint, codegen emits a uniqueness constraint and middleware that, on duplicate `request_id`, returns the original response with 200 (configurable: `idempotency_window: 24h`).

---

## Section 5 — Sectioned LLM Guide (v0.15.0)

### 5.1 Restructure

`docs/llm-guide.md` (~3,476 tokens) and `docs/llm-reference.md` (~1,292 tokens) collapse into a directory:

```
docs/llm-guide/
  core.md             # mandatory ~2k tokens
  controllers.md
  recipes.md          # index
  recipes/
    file_upload.md
    paginated_list_with_filters.md
    ...
  auth.md
  multi-tenancy.md
  caching.md
  rate-limiting.md
  file-storage.md
  events-and-jobs.md
  websockets.md
  graphql.md
  grpc.md
  errors.md           # full table; per-error pages live under docs/errors/
```

The Jekyll site stitches them back into `/llm-guide.html` for human readers. CLI embeds them via `include_str!` for machine readers.

### 5.2 Mandatory core contents (~2k token target)

The core MUST cover, and only cover:

1. Resource file top-level keys (one line each).
2. Field types table.
3. Field options reference (terse).
4. Endpoint conventions + valid keys per action (table, no prose).
5. Recipe index — name + one-liner per recipe.
6. CLI quick reference — one line per command, no flags.
7. Error code table — code + one-liner. Full descriptions live in `--section errors`.

Every other topic gets an inline pointer at the right place: e.g. the endpoints section says "Need rate limiting? Run `shaperail llm-guide --section rate-limiting`."

### 5.3 Section discoverability

The mandatory core MUST list section names with one-liners so the LLM knows what to fetch without trial-and-error:

```
## Sections (fetch with --section <name>)
controllers          — typed controller signatures, when to use vs. YAML primitives
recipes              — canonical full-resource examples for common patterns
auth                 — roles, ownership, JWT, claims
multi-tenancy        — tenant_key, row-level isolation
caching              — TTL, invalidate_on
rate-limiting        — per-endpoint rate limits
file-storage         — upload endpoint, storage backends
events-and-jobs      — event emission, background job queue
websockets           — WS channels, broadcast patterns
graphql              — GraphQL surface
grpc                 — gRPC surface
errors               — full error code reference
```

### 5.4 Token-budget acceptance

Each section's token count is checked in CI. Budgets:

| Section | Max tokens |
|---|---|
| `core.md` | 2,500 |
| Any single `--section` payload | 2,000 |
| Total guide bundle | 12,000 |

CI fails on overage. This forces deduplication and keeps the LLM-facing surface bounded.

---

## Affected crates and files

| Train | Crate | File(s) |
|---|---|---|
| v0.14.x | `shaperail-codegen` | `src/diagnostics.rs` (struct), `src/parser.rs` (parser swap), `src/validators/*.rs` (thread spans) |
| v0.14.x | `shaperail-cli` | `src/commands/check.rs`, `src/commands/explain.rs` |
| v0.14.x | `examples/recipes/` | new recipe directories (4 of 8) |
| v0.14.x | `docs/errors/` | new per-code pages |
| v0.15.0 | `shaperail-runtime` | `src/handlers/controller.rs` (Context<I, O>), removal of legacy `ControllerContext` |
| v0.15.0 | `shaperail-codegen` | `src/rust.rs` (emit `<Resource><Action>Input/Output/Context`), `src/codemod.rs` (new) |
| v0.15.0 | `shaperail-cli` | `src/commands/llm_guide.rs` (new), `src/llm_guide/*.md` (embedded), `src/commands/migrate_controllers.rs` (new) |
| v0.15.0 | `shaperail-cli/templates/init/CLAUDE.md` | new template |
| v0.16.0 | `shaperail-core` | new field/endpoint options (`computed`, `immutable`, `read`, `write`, `transform`, `audit_log`, `state`, `cascade`, `idempotency_key`) |
| v0.16.0 | `shaperail-codegen` | new validators (`SR110`–`SR120`), state-machine codegen, audit-log codegen, computed-field codegen |
| v0.16.0 | `shaperail-cli` | `src/commands/test_scaffold.rs` (new) |
| v0.16.0 | `examples/recipes/` | remaining 4 recipes |
| v0.16.0 | `tests/llm_guide_claims/` | snapshot fixtures + CI step |

---

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| YAML parser swap regresses existing valid resources. | Land parser swap behind a `--use-saphyr` cargo feature for one patch release; flip default in the next. Run the full snapshot test suite under both parsers in CI during transition. |
| Codemod (`migrate-controllers`) misses access patterns. | Codemod emits a TODO comment at every site it cannot rewrite with the original line preserved; manual review required. v0.15 release notes highlight this. |
| Concrete per-endpoint type aliases bloat compile times. | Aliases over a single generic type — no monomorphization explosion. Snapshot a build-time benchmark before/after; reject if cold build slows >10%. |
| Recipe library drift (recipes break as schema evolves). | Every recipe has an integration test in CI. A schema change that breaks a recipe must update the recipe in the same PR. |
| Token-budget CI rejects useful guide additions. | Budgets are tunable; the rule is to force a conscious decision, not to forbid growth. Raise the budget with justification in the same PR. |
| Sectioned guide fragmentation makes the public site harder to read. | Jekyll stitches sections back into a single `/llm-guide.html` page; humans read one document, the LLM reads N. |
| WASM users feel deprecated. | Explicit "kept and supported" wording in `docs/controllers.md`. No engineering changes — existing tests keep passing. |

---

## Acceptance criteria

**v0.14.x ships when:**
- `shaperail check --json` output includes `span`, `severity`, `doc_url` for every diagnostic that has a known span.
- Every `SR*` code in the registry has a `docs/errors/<code>.md` page; CI fails otherwise.
- `shaperail explain <file>` prints validation rules and OpenAPI fragments; `--format json` produces stable, documented JSON.
- 4 recipes shipped (`paginated_list_with_filters`, `file_upload`, `multi_tenant_rls`, `rate_limited_public`), each with a passing integration test.

**v0.15.0 ships when:**
- All controller signatures in `examples/` use `&mut <Resource><Action>Context`; legacy `&mut ControllerContext` removed from runtime.
- `cargo shaperail migrate-controllers` runs cleanly on `examples/incident-platform` and `examples/multi-tenant`, producing diff-free output (i.e. they were already migrated as part of the release prep).
- `shaperail llm-guide` runs offline, prints the embedded core; `--section <name>` and `--list` work for every section.
- `docs/upgrade-0.15.md` and `agent_docs/upgrade-0.15.md` published; mirror requirement satisfied.
- Token budgets enforced in CI: core ≤ 2,500, any section ≤ 2,000, bundle ≤ 12,000.

**v0.16.0 ships when:**
- Every controller in `examples/` is either migrated to YAML primitives or has a documented justification in its README for staying in Rust.
- All 8 recipes are present with passing integration tests.
- `shaperail generate --tests` produces compiling, passing test stubs for every example resource.
- `tests/llm_guide_claims/` has at least one fixture per LLM Guide section; CI step is wired into `ci.yml`.

---

## Out of scope

- WASM TS/Python first-class authoring (see D1).
- Generated SDK ergonomics for consumer code (frontends/scripts calling the API). Noted as a related but separate boundary; will be addressed in a future spec.
- Editor/LSP integration for YAML resources (uses the existing `export json-schema` output).

---

## Open questions

None remaining. D1–D8 close every choice raised during brainstorming.
