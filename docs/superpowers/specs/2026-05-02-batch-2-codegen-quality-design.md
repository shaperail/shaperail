# Batch 2 — Codegen Quality

**Date:** 2026-05-02
**Issues:** #5 (generated code not rustfmt-clean), #6 (unused `filters` parameter), #9 (OpenAPI omits per-role auth)
**Risk:** Low. All three are local changes inside `shaperail-codegen` plus minor template tweaks. No runtime semantics change.

## Goal

Make `shaperail generate` produce output that consumers can pipe straight into `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` in CI without further babysitting, and have `shaperail export openapi` carry enough auth metadata that SDK generators can act on it.

## Non-Goals

- Wiring the YAML-declared `filters:` list through to the actual SQL query builder. That is a feature gap, not a quality fix, and it gets its own milestone.
- Replacing `serde_json::json!` macros in `openapi.rs` with a typed builder.
- Adopting a real OpenAPI library. Keeping the deterministic hand-rolled generator per the PRD.

## Change 1 — `cargo fmt --check` clean output (#5)

### 1a — Post-write rustfmt pass

**File:** `shaperail-codegen/src/rust.rs` (or wherever the file-write happens; likely the generator entry point in `lib.rs` or a `writer.rs`).

After every generated `.rs` file is written to disk, invoke `rustfmt` on it via `std::process::Command`:

```rust
fn rustfmt_in_place(path: &Path) -> Result<(), CodegenError> {
    let status = Command::new("rustfmt")
        .arg("--edition")
        .arg("2021")
        .arg(path)
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            tracing::warn!(path = %path.display(), exit = ?s.code(),
                "rustfmt exited non-zero; leaving file unformatted");
            Ok(())
        }
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e,
                "rustfmt not found on PATH; skipping format pass. \
                 Install rustfmt with `rustup component add rustfmt`.");
            Ok(())
        }
    }
}
```

Behavior contract:

- If `rustfmt` is on `PATH` and succeeds → file is formatted.
- If it is missing or fails → emit a `tracing::warn!` once per `shaperail generate` run and continue. Codegen never fails because of an absent rustfmt.
- Use `--edition 2021` to match the workspace edition; do not pick up the user's `rustfmt.toml` opinionated settings (default rustfmt = stable, deterministic).

### 1b — Template cleanup

Audit `shaperail-codegen/src/rust.rs` for the most common template-induced diffs:

- Trailing whitespace inside `r###"…"###` blocks.
- Indentation drift (mix of tabs/spaces or 5-space prefixes).
- Method-chain continuations that rustfmt collapses to a single line — match its choice in the template so the post-pass becomes a no-op for the common case.
- Brace placement around `match` arms where formatted output drops a newline.

Goal: minimize the diff that the post-pass produces. The post-pass is the safety net; clean templates are the first line of defense.

### 1c — Test infrastructure

Add `shaperail-codegen/tests/generated_is_rustfmt_clean.rs`:

- Builds a small in-memory `ResourceDefinition` fixture covering each archetype (basic, content, lookup, tenant).
- Runs the generator end-to-end into a tempdir.
- Shells out to `rustfmt --check` on every emitted `.rs` file.
- Fails if any file is not idempotent under formatting.

This test is gated on `which::which("rustfmt").is_ok()`; absent rustfmt → `eprintln!` and skip. CI runs always have rustfmt available so this surfaces regressions.

## Change 2 — Eliminate unused `filters` warning (#6)

### 2a — Defensive `let _ = filters;` in empty-filter bodies

**File:** `shaperail-codegen/src/rust.rs`

`find_all` is declared on a trait (`async fn find_all(&self, endpoint: ..., filters: &FilterSet, ...)`), so all impls share the parameter name. We cannot rename to `_filters` per-resource.

Inspect the codegen path that emits the body of `find_all`. Today (lines ~518–583) `filter_decls`, `filter_args`, `filter_positions` are produced by iterating over `endpoint.spec.filters`. When that list is empty, the body never references `filters` and clippy warns.

Fix: when the resource has no declared filters for the list endpoint, prepend a single line to the body:

```rust
let _ = filters;
```

Implementation: in the function that assembles the `find_all` body (or the variable now named `list_dispatch` at `rust.rs:137`), branch on `endpoint.spec.filters.as_ref().map(|f| f.is_empty()).unwrap_or(true)`. If true, prepend the discard line.

Cross-check the same pattern for any other unused parameter in the trait body — `search`, `sort`, `page` could have the same issue when `pagination` or `search` is omitted from the YAML. Audit and apply the same prepend.

### 2b — Test

Extend `shaperail-codegen/tests/golden_<archetype>.rs` (or add `clippy_clean.rs`) to:

- Generate output for a resource with `endpoints.list:` and no `filters:` field.
- Assert the emitted file passes `cargo clippy --message-format=json -- -D warnings` when compiled.

If a full clippy invocation is too heavy for this test layer, at minimum textually assert the body contains either a `filters` reference or the `let _ = filters;` discard.

## Change 3 — OpenAPI per-role auth metadata (#9)

### 3a — Emit `x-shaperail-auth` extension

**File:** `shaperail-codegen/src/openapi.rs` (~line 577)

Today:

```rust
if let Some(auth) = &ep.auth {
    if !auth.is_public() {
        operation.insert(
            "security".to_string(),
            serde_json::json!([
                { "bearerAuth": [] },
                { "apiKeyAuth": [] }
            ]),
        );
    }
}
```

Add immediately after, while the existing `security` block stays untouched:

```rust
if let Some(auth) = &ep.auth {
    if !auth.is_public() {
        let roles = auth.roles();  // returns &[String]
        if !roles.is_empty() {
            operation.insert(
                "x-shaperail-auth".to_string(),
                serde_json::json!(roles),
            );
        }
    }
}
```

The accessor `auth.roles()` already exists or is trivial to add on the `Auth` enum/struct; verify and adjust naming.

### 3b — Why a vendor extension and not OAuth scopes

Stuffing roles into the OAuth scopes array (`{"bearerAuth": ["super_admin", "admin"]}`) is technically valid YAML but semantically wrong — OpenAPI 3.1 reserves the scopes array for OAuth flow scopes. Generators like `openapi-typescript-codegen` and `openapi-generator-cli` may apply OAuth-specific code paths to non-empty scopes (e.g., generating scope-acquisition helpers), producing nonsense client code.

Vendor extensions (`x-shaperail-*`) are the OpenAPI-blessed way to carry framework-specific metadata. The generator already uses `x-shaperail-controller` and `x-shaperail-events` for the same reason. SDK consumers who care about per-endpoint roles can read the extension; consumers who do not are unaffected.

### 3c — Tests

Add to `shaperail-codegen/tests/openapi_export.rs` (or wherever existing OpenAPI snapshot tests live):

- Resource with `endpoints.delete: { auth: [super_admin, admin] }` → generated spec contains `paths./v1/<r>/{id}.delete.x-shaperail-auth == ["super_admin", "admin"]`.
- Resource with `endpoints.list: { auth: public }` → `x-shaperail-auth` is **absent** on the list operation.
- Resource with no `auth:` declared → `x-shaperail-auth` absent and `security` falls back to whatever current behavior produces.

## Documentation

- `docs/openapi.md` (or wherever the export command is documented): add a paragraph describing `x-shaperail-auth` and link to OpenAPI vendor-extension semantics.
- `agent_docs/codegen-patterns.md`: note that all generated `.rs` files are run through `rustfmt` post-write; document the warn-and-continue contract.
- `CHANGELOG.md` under `[Unreleased]`:
  - **Fixed:** `shaperail generate` output now passes `cargo fmt --check` (#5).
  - **Fixed:** Generated list handlers no longer trigger `unused_variables` on `filters` when a resource declares no filters (#6).
  - **Added:** OpenAPI export now emits `x-shaperail-auth: [<roles>]` per operation (#9).

## Acceptance Criteria

1. `shaperail init demo && cd demo && shaperail generate && cargo fmt --check` passes immediately after generation.
2. `cargo clippy --all-targets -- -D warnings` passes on the generated tree of a fresh `shaperail init demo`.
3. `shaperail export openapi --output spec.json` on a project with `auth: [super_admin, admin]` on an endpoint produces a spec where the operation has `x-shaperail-auth == ["super_admin", "admin"]`.
4. Removing `rustfmt` from `PATH` and running `shaperail generate` emits one warning line and continues without error.
5. `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` pass.

## Rollout

Single PR. Non-breaking — the only externally visible change in the OpenAPI spec is an additive vendor extension, and the `rustfmt` pass is silent for users who already had clean output. Existing generated files are reformatted on the next `shaperail generate` run.
