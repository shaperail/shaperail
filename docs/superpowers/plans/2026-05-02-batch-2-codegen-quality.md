# Batch 2 — Codegen Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `shaperail generate` output pass `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` immediately, and have `shaperail export openapi` carry per-endpoint role metadata.

**Architecture:** Three small, mechanical changes inside `shaperail-codegen` — (1) shell out to `rustfmt` after writing each `.rs` file, with a graceful warn-and-continue fallback if `rustfmt` is missing; (2) prepend `let _ = filters;` to generated `find_all` bodies when the resource declares no filters; (3) emit an `x-shaperail-auth` vendor extension on each OpenAPI operation that has non-public `auth:` rules. No runtime semantics change; OpenAPI output is additive.

**Tech Stack:** Rust 2021, `serde_json`, `std::process::Command` (for `rustfmt` invocation), insta snapshots.

**Spec:** `docs/superpowers/specs/2026-05-02-batch-2-codegen-quality-design.md`

**Branch:** `fix/init-template-cleanup` (continuing — to be renamed before push)

---

## File Structure

| File | Responsibility | Action |
|------|----------------|--------|
| `shaperail-codegen/src/rust.rs` | Generates `generated/<resource>.rs` | Modify — add `let _ = filters;` prepend; identify the file-write callsites for `rustfmt` |
| `shaperail-codegen/src/lib.rs` (or wherever the codegen entry-point writes files) | Top-level codegen orchestration | Modify — add post-write `rustfmt` invocation |
| `shaperail-codegen/src/openapi.rs` | OpenAPI document builder | Modify — emit `x-shaperail-auth` after the `security` block |
| `shaperail-codegen/Cargo.toml` | Crate manifest | Modify — add `which` dev-dep if needed for rustfmt-availability tests |
| `shaperail-codegen/tests/` | Snapshot/integration tests | Modify — add a new test asserting generated files are rustfmt-clean and an OpenAPI test asserting `x-shaperail-auth` is emitted |
| `CHANGELOG.md` | Changelog | Modify |

---

## Task 1: Emit `x-shaperail-auth` per OpenAPI operation (#9)

This is the cheapest, lowest-risk change in this batch. Do it first.

**Files:**
- Modify: `shaperail-codegen/src/openapi.rs`

- [ ] **Step 1.1: Locate the security block**

In `shaperail-codegen/src/openapi.rs`, find the block that emits `security` for non-public endpoints (currently around line 577–588). It looks like:

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

- [ ] **Step 1.2: Identify the auth-roles accessor**

The `auth` field on `EndpointSpec` is of some `Auth` type (enum or struct). It already has an `is_public()` method. We need a way to obtain the list of roles (e.g., `["super_admin", "admin"]`).

Inspect the `Auth` type definition (search `pub enum Auth\|pub struct Auth` across `shaperail-core` and `shaperail-codegen`). It likely has either:

- A method `roles()` that returns `&[String]` for the role-list variant.
- A direct field like `Auth::Roles(Vec<String>)` to pattern-match on.

If neither exists, add a `pub fn roles(&self) -> &[String]` method on `Auth` that returns the slice for the role-list variant and an empty slice for `Public`. Place it next to `is_public()`.

- [ ] **Step 1.3: Emit the extension**

Immediately AFTER the `security` insertion above, add:

```rust
    if let Some(auth) = &ep.auth {
        if !auth.is_public() {
            let roles = auth.roles();
            if !roles.is_empty() {
                operation.insert(
                    "x-shaperail-auth".to_string(),
                    serde_json::json!(roles),
                );
            }
        }
    }
```

(If you also added the `roles()` accessor in Step 1.2, the imports might need adjusting — verify with `cargo build`.)

- [ ] **Step 1.4: Add a test**

In whichever test file currently exercises OpenAPI output (likely `shaperail-codegen/tests/openapi_export.rs` or an inline `#[test]` in `openapi.rs`), add:

```rust
#[test]
fn openapi_emits_x_shaperail_auth_for_role_endpoints() {
    // Build a minimal ResourceDefinition with an endpoint declaring
    // auth: [super_admin, admin] (use the same shape your other
    // OpenAPI tests use).
    let spec = build_openapi_spec_for_resource_with_role_endpoint();
    let delete_op = &spec["paths"]["/v1/agents/{id}"]["delete"];
    assert_eq!(
        delete_op["x-shaperail-auth"],
        serde_json::json!(["super_admin", "admin"])
    );
}

#[test]
fn openapi_omits_x_shaperail_auth_for_public_endpoints() {
    // auth: public OR no auth declared.
    let spec = build_openapi_spec_for_resource_with_public_list();
    let list_op = &spec["paths"]["/v1/agents"]["get"];
    assert!(list_op.get("x-shaperail-auth").is_none());
}
```

Adapt `build_openapi_spec_for_resource_*` helper names to whatever conventions exist in the test file. If no such helpers exist, inline a small `ResourceDefinition` construction in each test (look at how `x-shaperail-controller` is currently tested at lines 1093–1098 of `openapi.rs` — copy that style).

- [ ] **Step 1.5: Verify**

```
cargo test -p shaperail-codegen openapi
cargo build -p shaperail-codegen
```

Expected: tests pass, build clean.

- [ ] **Step 1.6: Commit**

```
git add shaperail-codegen/src/openapi.rs shaperail-codegen/tests
git commit -m "$(cat <<'EOF'
feat(codegen): emit x-shaperail-auth per OpenAPI operation

Generated OpenAPI specs now carry per-operation role metadata as a
vendor extension matching the existing x-shaperail-controller
pattern. SDK generators that respect extensions can use this to
emit role-aware client code; tools that don't are unaffected.

Roles are NOT injected into the standard `security: [{bearerAuth: [...]}]`
scopes array — those are reserved by OpenAPI for OAuth flow scopes,
and SDK generators that follow the spec apply OAuth-specific code
paths to non-empty scopes, producing nonsense client code for
non-OAuth schemes.

Closes #9.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Eliminate the unused `filters` warning (#6)

**Files:**
- Modify: `shaperail-codegen/src/rust.rs`

- [ ] **Step 2.1: Locate the find_all body assembly**

In `shaperail-codegen/src/rust.rs`, the find_all-style helper template lives around lines 653–683. The variable `filter_decls` (built around line 522 from `endpoint.spec.filters`) is interpolated near the top of the body:

```rust
{filter_decls}
{search_decl}
{sort_decls}
```

When the resource declares no filters, `filter_decls` is empty and `filters: &FilterSet` is bound but never read in the body — `cargo clippy --all-targets -- -D warnings` fails on the generated file with an `unused_variables` warning.

- [ ] **Step 2.2: Apply the fix**

Find the line that builds `filter_decls` (around line 522 in `build_list_endpoint_helper` or whatever the function is named). Inspect the actual variable; expect something like:

```rust
let filters = endpoint.spec.filters.clone().unwrap_or_default();
let filter_decls = filters
    .iter()
    .map(|f| /* ... */)
    .collect::<Vec<_>>()
    .join("\n");
```

When the local `filters` Vec is empty, we want `filter_decls` to be `        let _ = filters;` instead of empty string. Replace the assignment with:

```rust
let filter_decls = if filters.is_empty() {
    "        let _ = filters;".to_string()
} else {
    filters
        .iter()
        .map(|f| /* ... unchanged ... */)
        .collect::<Vec<_>>()
        .join("\n")
};
```

(Indentation: 8 spaces matches the surrounding generated code. Verify by looking at how `parse_filter` lines are indented at line 1177.)

- [ ] **Step 2.3: Audit other unused trait params**

The trait method also takes `search`, `sort`, `page`. Quickly verify these are always used in the body (the `match page { ... }` block at line 665 always reads `page`; sort and search usage depends on YAML). If any of those become unused for a resource with no `pagination`/`search` declared, apply the same `let _ = X;` pattern. Most likely only `filters` is the issue, but check.

- [ ] **Step 2.4: Test by generating and clippying a minimal resource**

There is likely an existing snapshot test in `shaperail-codegen/tests/` that exercises a resource with no filters. Run:

```
cargo test -p shaperail-codegen
```

If a snapshot test now diverges (an `let _ = filters;` line was added that wasn't in the snapshot), accept the new snapshot:

```
cargo insta accept
```

- [ ] **Step 2.5: New regression test**

Add (or extend) a test that:

1. Builds a `ResourceDefinition` with `endpoints.list:` declared but no `filters:` field.
2. Runs the rust codegen for it.
3. Asserts the generated source string contains either `let _ = filters;` OR an actual `parse_filter(filters, ...)` invocation. (One of those two means the parameter is read.)

Place in `shaperail-codegen/tests/clippy_clean.rs` (new) or an existing rust-codegen test file.

- [ ] **Step 2.6: Verify a fresh scaffold passes clippy**

If you have time and a `DATABASE_URL` set, do a `shaperail init`-then-`cargo clippy` smoke:

```
cd /tmp && rm -rf shaperail-batch2-clippy
cargo run -q -p shaperail-cli -- init shaperail-batch2-clippy
# add a resource with list endpoint and no filters
echo 'resource: things
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  list:
    auth: public' > shaperail-batch2-clippy/resources/things.yaml
(cd shaperail-batch2-clippy && cargo run -q -p shaperail-cli -- generate 2>/dev/null)
(cd shaperail-batch2-clippy && cargo clippy --all-targets -- -D warnings 2>&1 | tail -10)
rm -rf shaperail-batch2-clippy
```

Optional but recommended. If `cargo clippy` produces warnings you didn't expect, do not fix them in init.rs — fix at the codegen template root. Skip the smoke if it would require a live workspace setup that's awkward.

- [ ] **Step 2.7: Commit**

```
git add shaperail-codegen/src/rust.rs shaperail-codegen/tests
git commit -m "$(cat <<'EOF'
fix(codegen): drop unused-filters warning in find_all bodies

When a resource declares no filters: in YAML, the generated
find_all impl took filters: &FilterSet but never read it,
tripping cargo clippy --all-targets -- -D warnings. Insert
`let _ = filters;` at the top of the body in that case.

Closes #6.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: rustfmt the generated files (#5)

**Files:**
- Modify: whichever module owns the generated-file write loop (likely `shaperail-codegen/src/lib.rs` or the entry function in `rust.rs`).

- [ ] **Step 3.1: Locate the write callsites**

Search for `write_file\b\|fs::write\b\|File::create\b` in `shaperail-codegen/src` and identify where generated `.rs` files land on disk. There may be one central writer or per-target writers (rust, openapi, typescript, etc.).

- [ ] **Step 3.2: Add a rustfmt helper**

Create a private function next to the writer (or in a new `format.rs` module if cleanly isolated):

```rust
/// Run `rustfmt --edition 2021` against the file at `path`. Logs and continues
/// on any failure — never panics or returns Err — because codegen must keep
/// working in environments without `rustfmt` on PATH.
fn rustfmt_in_place(path: &std::path::Path) {
    let result = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2021")
        .arg(path)
        .status();
    match result {
        Ok(status) if status.success() => {}
        Ok(status) => {
            tracing::warn!(
                path = %path.display(),
                exit = ?status.code(),
                "rustfmt exited non-zero; leaving generated file unformatted"
            );
        }
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "rustfmt not found on PATH; skipping format pass. \
                 Install with `rustup component add rustfmt`."
            );
        }
    }
}
```

If `tracing` isn't already a dep of `shaperail-codegen` (check `Cargo.toml`), use `eprintln!` instead — codegen runs in an interactive `shaperail generate` context where stderr is acceptable.

- [ ] **Step 3.3: Wire it into the writer**

After every `.rs` file is written, call `rustfmt_in_place(&path)`. Do NOT call it on `.json`, `.yaml`, `.proto`, `.ts` etc. — only Rust output.

If the writer is generic over file extension, gate the call:

```rust
if path.extension().and_then(|e| e.to_str()) == Some("rs") {
    rustfmt_in_place(path);
}
```

- [ ] **Step 3.4: Add a rustfmt-clean regression test**

Add `shaperail-codegen/tests/generated_is_rustfmt_clean.rs`:

```rust
//! Asserts that the rust codegen produces files that are idempotent under
//! `rustfmt --check`. Skipped when rustfmt is not on PATH.

use std::process::Command;

#[test]
fn generated_files_are_rustfmt_clean() {
    if Command::new("rustfmt").arg("--version").status().map(|s| !s.success()).unwrap_or(true) {
        eprintln!("Skipping: rustfmt not on PATH");
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    // Build a minimal ResourceDefinition. Reuse helpers from existing tests
    // if available; otherwise inline the smallest ResourceDefinition that
    // exercises a list endpoint with filters and a list endpoint without.
    let out_dir = tmp.path().join("generated");
    std::fs::create_dir_all(&out_dir).unwrap();
    // Run the codegen entry point against `out_dir`. Adjust to whatever
    // function shaperail-codegen exposes (e.g., `generate_resource_rust`).
    // For each generated `.rs` file, run `rustfmt --check`.

    for entry in std::fs::read_dir(&out_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let status = Command::new("rustfmt")
            .arg("--check")
            .arg("--edition")
            .arg("2021")
            .arg(&path)
            .status()
            .expect("rustfmt --check");
        assert!(
            status.success(),
            "{} is not rustfmt-clean after generation",
            path.display()
        );
    }
}
```

If `tempfile` isn't a `[dev-dependencies]` entry, add it. (Likely already there — check `shaperail-codegen/Cargo.toml`.)

- [ ] **Step 3.5: Build + test**

```
cargo build -p shaperail-codegen
cargo test -p shaperail-codegen
```

Expected: clean build, all tests pass. The new `generated_files_are_rustfmt_clean` test passes (or skips if rustfmt is missing).

If the new test fails because rustfmt's pass over the generated output produced a non-empty diff, the codegen template is the bug — examine the diff (re-run `rustfmt` non-`--check` to see the rewrite, then update the template). Most common diffs are: trailing whitespace, mismatched indentation, single-line method chains rustfmt wants to split.

- [ ] **Step 3.6: Commit**

```
git add shaperail-codegen/src shaperail-codegen/tests shaperail-codegen/Cargo.toml
git commit -m "$(cat <<'EOF'
fix(codegen): rustfmt every generated .rs file post-write

After writing each generated/<resource>.rs file, shell out to
rustfmt --edition 2021 to format it in place. If rustfmt is
missing or fails, log a warn line and continue — codegen
remains functional in environments without rustfmt installed.

Adds a regression test (skipped when rustfmt is unavailable)
that asserts every generated .rs file passes
`rustfmt --check`.

Closes #5.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: CHANGELOG and final quality gate

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 4.1: Append to `[Unreleased]`**

In `CHANGELOG.md`, under the existing `## [Unreleased]` section, add to `### Added` and `### Fixed` (the sections already exist from prior batches):

```markdown
- **OpenAPI export** now emits `x-shaperail-auth: [<roles>]` as a vendor extension on each operation that declares non-public `auth:`. Matches the existing `x-shaperail-controller` / `x-shaperail-events` extension pattern. Standard `security:` is unchanged — roles are deliberately not stuffed into the OAuth-scopes array (#9).
```

(under `### Added`)

```markdown
- **`shaperail generate` output passes `cargo fmt --check` immediately.** Each generated `.rs` file is run through `rustfmt --edition 2021` post-write; missing rustfmt on `PATH` is degraded to a warning rather than failing codegen (#5).
- **Generated list handlers no longer trip `cargo clippy -- -D warnings`** with `unused_variables: filters`. When a resource declares no `filters:` in YAML, the codegen now emits `let _ = filters;` at the top of the find_all body (#6).
```

(under `### Fixed`)

- [ ] **Step 4.2: Final gate**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --no-fail-fast --features test-support
```

Expected: clean fmt, clean clippy, no NEW test failures (the 44 pre-existing `PoolTimedOut` failures still appear).

- [ ] **Step 4.3: Commit**

```
git add CHANGELOG.md
git commit -m "$(cat <<'EOF'
docs(changelog): note batch-2 (codegen quality)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Acceptance Checklist

- [ ] `shaperail init demo && shaperail generate && cargo fmt --check` passes immediately after generation.
- [ ] `cargo clippy --all-targets -- -D warnings` passes on a generated tree where some resources declare no `filters:`.
- [ ] `shaperail export openapi` produces a spec where role-bearing operations carry `x-shaperail-auth: [<roles>]`, and public operations do not.
- [ ] Removing `rustfmt` from `PATH` and running `shaperail generate` emits a single warning line and continues without error.
- [ ] `cargo test --workspace --features test-support` passes (modulo pre-existing DB-required failures); `cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean.
