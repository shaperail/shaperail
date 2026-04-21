# Wiring Gaps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto-connect four manual-wiring primitives — controller map, job registry, WebSocket channel routes, and inbound webhook routes — so a generated Shaperail app works without any `main.rs` bootstrapping.

**Architecture:** Four independent changes that share one pattern: codegen or runtime reads declarations → generates/loads registration code → scaffold template calls it. Each change is self-contained and can be implemented, tested, and committed independently. Three changes touch `shaperail-cli/src/commands/init.rs` to update the scaffold template; that file is a raw-string Rust source template, so edits must be made to the string literal inside.

**Tech Stack:** Rust, `serde_yaml`, `insta` snapshot testing, `tempfile` for stub-writing tests.

---

## Files Modified

| File | Change |
|------|--------|
| `shaperail-codegen/src/rust.rs` | Update `generate_registry_module()` to use `#[path]` and populate `build_controller_map()`; add `generate_job_registry()` |
| `shaperail-codegen/tests/snapshot_tests.rs` | Add codegen tests for controller map and job registry output |
| `shaperail-cli/src/commands/generate.rs` | Add `write_controller_stubs()` and `write_job_stubs()` called from `run()` |
| `shaperail-runtime/src/jobs/worker.rs` | Add `JobRegistry::is_empty()` |
| `shaperail-runtime/src/ws/session.rs` | Add `load_channels()` |
| `shaperail-runtime/src/ws/mod.rs` | Export `load_channels` |
| `shaperail-core/src/config.rs` | Add `signature_header` field to `InboundWebhookConfig` |
| `shaperail-runtime/src/events/inbound.rs` | Thread `signature_header` through `InboundWebhookState` |
| `shaperail-cli/src/commands/init.rs` | Update scaffold template: job worker startup, WS channel loading, inbound route registration |

---

## Task 1: Populate build_controller_map() in codegen + write controller stubs

**Context:** `generate_registry_module()` in `shaperail-codegen/src/rust.rs` (line 170) currently outputs an empty `build_controller_map()`. The function already has a reference implementation of how to iterate controller declarations in `generate_controller_traits()` (line 223) — use that same iteration logic to build `#[path]` module declarations and `map.register()` calls. Controller stubs are written by a new function in `shaperail-cli/src/commands/generate.rs`. The fixture `shaperail-codegen/tests/fixtures/valid/users.yaml` has `controller: { before: validate_org }` on the `create` endpoint — use it for testing.

**Files:**
- Modify: `shaperail-codegen/src/rust.rs:170-213` — update `generate_registry_module()`; add helper `collect_controller_hooks()`
- Modify: `shaperail-codegen/tests/snapshot_tests.rs` — add two tests
- Modify: `shaperail-cli/src/commands/generate.rs` — add `write_controller_stubs()`; call from `run()`

---

- [ ] **Step 1: Write the failing codegen test**

Add to `shaperail-codegen/tests/snapshot_tests.rs`:

```rust
#[test]
fn controller_map_populated_when_resource_has_controller() {
    let yaml = include_str!("fixtures/valid/users.yaml");
    let rd = shaperail_codegen::parser::parse_resource(yaml).unwrap();
    let project = shaperail_codegen::rust::generate_project(&[rd]).unwrap();
    assert!(
        project.mod_rs.contains("users_controller"),
        "expected #[path] module for users_controller in mod_rs:\n{}",
        project.mod_rs
    );
    assert!(
        project.mod_rs.contains(r#"map.register("users", "validate_org""#),
        "expected map.register call in mod_rs:\n{}",
        project.mod_rs
    );
}

#[test]
fn controller_map_empty_when_no_controllers() {
    let yaml = include_str!("fixtures/valid/minimal.yaml");
    let rd = shaperail_codegen::parser::parse_resource(yaml).unwrap();
    let project = shaperail_codegen::rust::generate_project(&[rd]).unwrap();
    assert!(
        !project.mod_rs.contains("_controller"),
        "expected no controller modules in mod_rs:\n{}",
        project.mod_rs
    );
    assert!(
        project.mod_rs.contains("ControllerMap::new()"),
        "expected empty ControllerMap::new() in mod_rs:\n{}",
        project.mod_rs
    );
}
```

- [ ] **Step 2: Run the tests to confirm they fail**

```bash
cargo test -p shaperail-codegen controller_map 2>&1 | tail -20
```

Expected: FAILED — `users_controller` not found in mod_rs output.

- [ ] **Step 3: Add helper function `collect_controller_hooks()` to rust.rs**

Add this function just before `generate_registry_module()` in `shaperail-codegen/src/rust.rs`:

```rust
/// Returns (resource_name, [hook_fn_names]) for each resource with native (non-WASM) controller hooks.
fn collect_controller_hooks(resources: &[ResourceDefinition]) -> Vec<(&str, Vec<&str>)> {
    resources
        .iter()
        .filter_map(|resource| {
            let hooks: Vec<&str> = resource
                .endpoints
                .as_ref()
                .map(|eps| {
                    eps.iter()
                        .filter_map(|(_, ep)| ep.controller.as_ref())
                        .flat_map(|c| {
                            let before = c
                                .before
                                .as_deref()
                                .filter(|s| !s.starts_with(shaperail_core::WASM_HOOK_PREFIX));
                            let after = c
                                .after
                                .as_deref()
                                .filter(|s| !s.starts_with(shaperail_core::WASM_HOOK_PREFIX));
                            [before, after].into_iter().flatten()
                        })
                        .collect()
                })
                .unwrap_or_default();
            if hooks.is_empty() {
                None
            } else {
                Some((resource.resource.as_str(), hooks))
            }
        })
        .collect()
}
```

- [ ] **Step 4: Update `generate_registry_module()` to populate `build_controller_map()`**

Replace the body of `generate_registry_module()` in `shaperail-codegen/src/rust.rs` (lines 170–214) with:

```rust
fn generate_registry_module(resources: &[ResourceDefinition]) -> String {
    let module_lines = resources
        .iter()
        .map(|resource| format!("pub mod {};", resource.resource))
        .collect::<Vec<_>>()
        .join("\n");

    let registry_lines = resources
        .iter()
        .map(|resource| {
            let store_name = format!("{}Store", to_pascal_case(&resource.resource));
            format!(
                "    stores.insert({name:?}.to_string(), std::sync::Arc::new({module}::{store_name}::new(pool.clone())));",
                name = resource.resource,
                module = resource.resource
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let ctrl_hooks = collect_controller_hooks(resources);

    // #[path] module declarations for controller files
    let ctrl_path_decls: Vec<String> = ctrl_hooks
        .iter()
        .map(|(name, _)| {
            format!(
                "#[path = \"../resources/{name}.controller.rs\"]\nmod {name}_controller;"
            )
        })
        .collect();

    // map.register(...) calls
    let mut seen_ctrl = std::collections::HashSet::new();
    let ctrl_register_lines: Vec<String> = ctrl_hooks
        .iter()
        .flat_map(|(res_name, hooks)| {
            hooks.iter().filter_map(|fn_name| {
                let key = (*res_name, *fn_name);
                if seen_ctrl.insert(key) {
                    Some(format!(
                        "    map.register({res_name:?}, {fn_name:?}, {res_name}_controller::{fn_name});"
                    ))
                } else {
                    None
                }
            }).collect::<Vec<_>>()
        })
        .collect();

    let ctrl_map_body = if ctrl_register_lines.is_empty() {
        "    shaperail_runtime::handlers::controller::ControllerMap::new()".to_string()
    } else {
        let mut lines = vec![
            "    let mut map = shaperail_runtime::handlers::controller::ControllerMap::new();".to_string(),
        ];
        lines.extend(ctrl_register_lines);
        lines.push("    map".to_string());
        lines.join("\n")
    };

    let preamble = {
        let mut parts = vec![module_lines.clone()];
        if !ctrl_path_decls.is_empty() {
            parts.push(ctrl_path_decls.join("\n"));
        }
        let job_path_decls = collect_job_path_decls(resources);
        if !job_path_decls.is_empty() {
            parts.push(job_path_decls.join("\n"));
        }
        parts.join("\n\n")
    };

    format!(
        r#"#![allow(dead_code)]

{preamble}

pub fn build_store_registry(pool: sqlx::PgPool) -> shaperail_runtime::db::StoreRegistry {{
    let mut stores: std::collections::HashMap<
        String,
        std::sync::Arc<dyn shaperail_runtime::db::ResourceStore>,
    > = std::collections::HashMap::new();
{registry_lines}
    std::sync::Arc::new(stores)
}}

pub fn build_controller_map() -> shaperail_runtime::handlers::controller::ControllerMap {{
{ctrl_map_body}
}}

{job_registry_fn}

{controller_traits}
"#,
        job_registry_fn = generate_job_registry(resources),
        controller_traits = generate_controller_traits(resources),
    )
}
```

Also add this helper (used above, can be empty until Task 2):

```rust
/// Returns `#[path]` declarations for job modules. Called by generate_registry_module().
fn collect_job_path_decls(_resources: &[ResourceDefinition]) -> Vec<String> {
    Vec::new() // filled in Task 2
}
```

- [ ] **Step 5: Run the failing tests — they should now pass**

```bash
cargo test -p shaperail-codegen controller_map 2>&1 | tail -20
```

Expected: 2 tests PASSED.

- [ ] **Step 6: Write a failing test for controller stub generation in shaperail-cli**

Add a test module to `shaperail-cli/src/commands/generate.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use shaperail_codegen::parser::parse_resource;

    #[test]
    fn controller_stub_written_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let resources_dir = dir.path().join("resources");
        std::fs::create_dir_all(&resources_dir).unwrap();

        let yaml = r#"
resource: orders
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    auth: [admin]
    input: [id]
    controller:
      before: check_inventory
"#;
        let rd = parse_resource(yaml).unwrap();
        write_controller_stubs(&[rd], &resources_dir).unwrap();

        let stub_path = resources_dir.join("orders.controller.rs");
        assert!(stub_path.exists(), "stub file should be created");
        let contents = std::fs::read_to_string(&stub_path).unwrap();
        assert!(contents.contains("check_inventory"), "stub should contain function name");
        assert!(contents.contains("todo!"), "stub should have todo! placeholder");
    }

    #[test]
    fn controller_stub_not_overwritten_when_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let resources_dir = dir.path().join("resources");
        std::fs::create_dir_all(&resources_dir).unwrap();

        let existing = resources_dir.join("orders.controller.rs");
        std::fs::write(&existing, "// existing content").unwrap();

        let yaml = r#"
resource: orders
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    auth: [admin]
    input: [id]
    controller:
      before: check_inventory
"#;
        let rd = parse_resource(yaml).unwrap();
        write_controller_stubs(&[rd], &resources_dir).unwrap();

        let contents = std::fs::read_to_string(&existing).unwrap();
        assert_eq!(contents, "// existing content", "existing file must not be overwritten");
    }
}
```

Add `tempfile` to `shaperail-cli/Cargo.toml` under `[dev-dependencies]`:
```toml
tempfile = "3"
```

- [ ] **Step 7: Run the stub tests — confirm they fail**

```bash
cargo test -p shaperail-cli controller_stub 2>&1 | tail -10
```

Expected: error — `write_controller_stubs` not found.

- [ ] **Step 8: Implement `write_controller_stubs()` in generate.rs**

Add this function (and make it `pub(crate)` for tests) after `clear_generated_rust_files` in `shaperail-cli/src/commands/generate.rs`:

```rust
/// Writes a stub controller file for each resource that declares native controller hooks,
/// if the file does not already exist. Never overwrites existing files.
pub(crate) fn write_controller_stubs(
    resources: &[ResourceDefinition],
    resources_dir: &Path,
) -> Result<(), String> {
    for resource in resources {
        let Some(endpoints) = &resource.endpoints else {
            continue;
        };

        // Collect non-WASM hook function names for this resource
        let hook_names: Vec<&str> = endpoints
            .iter()
            .filter_map(|(_, ep)| ep.controller.as_ref())
            .flat_map(|c| {
                let before = c
                    .before
                    .as_deref()
                    .filter(|s| !s.starts_with(shaperail_core::WASM_HOOK_PREFIX));
                let after = c
                    .after
                    .as_deref()
                    .filter(|s| !s.starts_with(shaperail_core::WASM_HOOK_PREFIX));
                [before, after].into_iter().flatten()
            })
            .collect();

        if hook_names.is_empty() {
            continue;
        }

        let stub_path = resources_dir.join(format!("{}.controller.rs", resource.resource));
        if stub_path.exists() {
            continue;
        }

        let mut lines = Vec::new();
        for fn_name in &hook_names {
            lines.push(format!(
                r#"pub async fn {fn_name}(
    ctx: &mut shaperail_runtime::handlers::ControllerContext,
) -> Result<(), shaperail_core::ShaperailError> {{
    todo!("implement {fn_name}")
}}
"#
            ));
        }

        fs::write(&stub_path, lines.join("\n")).map_err(|e| {
            format!("Failed to write {}: {e}", stub_path.display())
        })?;
    }
    Ok(())
}
```

Also add the `shaperail_core` import at the top of `generate.rs` if not already present:
```rust
use shaperail_core::WASM_HOOK_PREFIX;
```

Wait — `WASM_HOOK_PREFIX` is used via `shaperail_core::WASM_HOOK_PREFIX` in the filter closure, but the import at the top of the file is `use shaperail_core::ResourceDefinition;`. Just qualify inline as `shaperail_core::WASM_HOOK_PREFIX` or add: `use shaperail_core::{ResourceDefinition, WASM_HOOK_PREFIX};`.

Update the existing import in `generate.rs`:
```rust
use shaperail_core::{ResourceDefinition, WASM_HOOK_PREFIX};
```

Then update the filter in `write_controller_stubs` to use just `WASM_HOOK_PREFIX`.

Finally, call `write_controller_stubs` from `run()` in generate.rs, after writing generated modules:

```rust
pub fn run() -> i32 {
    // ... existing resource loading ...

    match write_generated_modules(&resources, Path::new("generated")) {
        Ok(paths) => {
            for path in &paths {
                println!("Generated {}", path.display());
            }
            // Write controller stubs for any newly declared controllers
            if let Err(e) = write_controller_stubs(&resources, Path::new("resources")) {
                eprintln!("Warning: {e}");
            }
            println!(
                "Generated {} resource module(s) in generated/",
                resources.len()
            );
            0
        }
        Err(e) => {
            eprintln!("Error: {e}");
            1
        }
    }
}
```

- [ ] **Step 9: Run the stub tests — they should now pass**

```bash
cargo test -p shaperail-cli controller_stub 2>&1 | tail -10
```

Expected: 2 tests PASSED.

- [ ] **Step 10: Run the full workspace check**

```bash
cargo test -p shaperail-codegen -p shaperail-cli 2>&1 | tail -20
cargo clippy -p shaperail-codegen -p shaperail-cli -- -D warnings 2>&1 | tail -20
cargo fmt --check 2>&1
```

Expected: all pass.

- [ ] **Step 11: Commit**

```bash
git add shaperail-codegen/src/rust.rs shaperail-codegen/tests/snapshot_tests.rs \
        shaperail-cli/src/commands/generate.rs shaperail-cli/Cargo.toml
git commit -m "feat(codegen): populate build_controller_map() from resource YAML; write controller stubs"
```

---

## Task 2: generate_job_registry() + JobRegistry::is_empty() + job stubs + scaffold

**Context:** Job names come from `jobs:` lists in resource endpoints. The fixture `users.yaml` has `jobs: [send_welcome_email]` on the `create` endpoint. Job names are deduplicated across all resources. The generated `build_job_registry()` function uses `#[path]` modules from `jobs/<name>.rs`. Each job stub exports `pub async fn handle(payload: serde_json::Value) -> Result<(), ShaperailError>`. The scaffold template in `shaperail-cli/src/commands/init.rs` starts at line 146 as `let main_rs = r###"..."###`. The job worker block should be inserted after `let event_emitter = ...` (current line ~1018) and before the `AppState` construction.

**Files:**
- Modify: `shaperail-codegen/src/rust.rs` — add `generate_job_registry()`, fill in `collect_job_path_decls()` stub
- Modify: `shaperail-runtime/src/jobs/worker.rs:26-45` — add `is_empty()` to `JobRegistry`
- Modify: `shaperail-cli/src/commands/generate.rs` — add `write_job_stubs()`; call from `run()`
- Modify: `shaperail-cli/src/commands/init.rs` — update scaffold template

---

- [ ] **Step 1: Write failing codegen test for job registry**

Add to `shaperail-codegen/tests/snapshot_tests.rs`:

```rust
#[test]
fn job_registry_populated_when_resource_has_jobs() {
    let yaml = include_str!("fixtures/valid/users.yaml");
    let rd = shaperail_codegen::parser::parse_resource(yaml).unwrap();
    let project = shaperail_codegen::rust::generate_project(&[rd]).unwrap();
    assert!(
        project.mod_rs.contains("build_job_registry"),
        "expected build_job_registry fn in mod_rs:\n{}",
        project.mod_rs
    );
    assert!(
        project.mod_rs.contains("send_welcome_email"),
        "expected send_welcome_email in mod_rs:\n{}",
        project.mod_rs
    );
}

#[test]
fn job_registry_empty_when_no_jobs() {
    let yaml = include_str!("fixtures/valid/minimal.yaml");
    let rd = shaperail_codegen::parser::parse_resource(yaml).unwrap();
    let project = shaperail_codegen::rust::generate_project(&[rd]).unwrap();
    assert!(
        project.mod_rs.contains("JobRegistry::new()"),
        "expected empty JobRegistry::new() in mod_rs:\n{}",
        project.mod_rs
    );
}
```

- [ ] **Step 2: Run the tests — confirm they fail**

```bash
cargo test -p shaperail-codegen job_registry 2>&1 | tail -10
```

Expected: FAILED — `build_job_registry` not found in mod_rs.

- [ ] **Step 3: Implement `generate_job_registry()` and fill in `collect_job_path_decls()` in rust.rs**

Add `collect_job_names()` and `collect_job_path_decls()` helpers (replace the placeholder `collect_job_path_decls` stub from Task 1):

```rust
/// Returns all unique job names declared across all resources, sorted for determinism.
fn collect_job_names(resources: &[ResourceDefinition]) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for resource in resources {
        if let Some(endpoints) = &resource.endpoints {
            for (_, ep) in endpoints {
                for job in ep.jobs.as_deref().unwrap_or_default() {
                    names.insert(job.clone());
                }
            }
        }
    }
    names.into_iter().collect()
}

/// Returns `#[path]` declarations for job handler modules.
fn collect_job_path_decls(resources: &[ResourceDefinition]) -> Vec<String> {
    collect_job_names(resources)
        .iter()
        .map(|name| {
            format!(
                "#[path = \"../jobs/{name}.rs\"]\nmod job_{name};"
            )
        })
        .collect()
}

/// Generates the `build_job_registry()` function body.
fn generate_job_registry(resources: &[ResourceDefinition]) -> String {
    let job_names = collect_job_names(resources);

    if job_names.is_empty() {
        return r#"pub fn build_job_registry() -> shaperail_runtime::jobs::JobRegistry {
    shaperail_runtime::jobs::JobRegistry::new()
}"#
        .to_string();
    }

    let inserts: Vec<String> = job_names
        .iter()
        .map(|name| {
            format!(
                r#"    handlers.insert(
        "{name}".to_string(),
        std::sync::Arc::new(|payload: serde_json::Value| {{
            Box::pin(job_{name}::handle(payload))
                as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), shaperail_core::ShaperailError>> + Send>>
        }}) as shaperail_runtime::jobs::JobHandler,
    );"#
            )
        })
        .collect();

    format!(
        r#"pub fn build_job_registry() -> shaperail_runtime::jobs::JobRegistry {{
    let mut handlers: std::collections::HashMap<String, shaperail_runtime::jobs::JobHandler> =
        std::collections::HashMap::new();
{inserts}
    shaperail_runtime::jobs::JobRegistry::from_handlers(handlers)
}}"#,
        inserts = inserts.join("\n")
    )
}
```

- [ ] **Step 4: Run the codegen tests — they should now pass**

```bash
cargo test -p shaperail-codegen job_registry 2>&1 | tail -10
```

Expected: 2 tests PASSED.

- [ ] **Step 5: Write failing test for `JobRegistry::is_empty()` in worker.rs**

Add to the `#[cfg(test)]` block in `shaperail-runtime/src/jobs/worker.rs`:

```rust
#[test]
fn is_empty_returns_true_for_new_registry() {
    let registry = JobRegistry::new();
    assert!(registry.is_empty());
}

#[test]
fn is_empty_returns_false_when_handler_registered() {
    let mut handlers = HashMap::new();
    handlers.insert(
        "a_job".to_string(),
        Arc::new(|_payload: serde_json::Value| {
            Box::pin(async { Ok(()) })
                as Pin<Box<dyn Future<Output = Result<(), ShaperailError>> + Send>>
        }) as JobHandler,
    );
    let registry = JobRegistry::from_handlers(handlers);
    assert!(!registry.is_empty());
}
```

- [ ] **Step 6: Run the tests — confirm they fail**

```bash
cargo test -p shaperail-runtime is_empty 2>&1 | tail -10
```

Expected: FAILED — `is_empty` method not found.

- [ ] **Step 7: Add `is_empty()` to `JobRegistry` in worker.rs**

In `shaperail-runtime/src/jobs/worker.rs`, add the method to the `impl JobRegistry` block after `get()`:

```rust
    /// Returns true if no handlers are registered.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
```

- [ ] **Step 8: Run the is_empty tests — they should pass**

```bash
cargo test -p shaperail-runtime is_empty 2>&1 | tail -10
```

Expected: 2 tests PASSED.

- [ ] **Step 9: Write failing tests for job stub generation**

Add to `shaperail-cli/src/commands/generate.rs` tests module (in the same `#[cfg(test)]` block added in Task 1):

```rust
    #[test]
    fn job_stub_written_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let jobs_dir = dir.path().join("jobs");
        std::fs::create_dir_all(&jobs_dir).unwrap();

        let yaml = r#"
resource: users
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    auth: [admin]
    input: [id]
    jobs: [send_welcome_email]
"#;
        let rd = parse_resource(yaml).unwrap();
        write_job_stubs(&[rd], &jobs_dir).unwrap();

        let stub_path = jobs_dir.join("send_welcome_email.rs");
        assert!(stub_path.exists(), "job stub should be created");
        let contents = std::fs::read_to_string(&stub_path).unwrap();
        assert!(contents.contains("pub async fn handle"), "stub should have handle fn");
        assert!(contents.contains("todo!"), "stub should have todo! placeholder");
    }

    #[test]
    fn job_stub_not_overwritten_when_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let jobs_dir = dir.path().join("jobs");
        std::fs::create_dir_all(&jobs_dir).unwrap();

        let existing = jobs_dir.join("send_welcome_email.rs");
        std::fs::write(&existing, "// existing job handler").unwrap();

        let yaml = r#"
resource: users
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
endpoints:
  create:
    auth: [admin]
    input: [id]
    jobs: [send_welcome_email]
"#;
        let rd = parse_resource(yaml).unwrap();
        write_job_stubs(&[rd], &jobs_dir).unwrap();

        let contents = std::fs::read_to_string(&existing).unwrap();
        assert_eq!(contents, "// existing job handler");
    }
```

- [ ] **Step 10: Run the tests — confirm they fail**

```bash
cargo test -p shaperail-cli job_stub 2>&1 | tail -10
```

Expected: FAILED — `write_job_stubs` not found.

- [ ] **Step 11: Implement `write_job_stubs()` and call it from `run()` in generate.rs**

Add to `shaperail-cli/src/commands/generate.rs` (after `write_controller_stubs`):

```rust
/// Writes a stub job handler file for each unique job name declared across resources,
/// if the file does not already exist. Never overwrites existing files.
pub(crate) fn write_job_stubs(
    resources: &[ResourceDefinition],
    jobs_dir: &Path,
) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for resource in resources {
        if let Some(endpoints) = &resource.endpoints {
            for (_, ep) in endpoints {
                for job_name in ep.jobs.as_deref().unwrap_or_default() {
                    if !seen.insert(job_name.clone()) {
                        continue;
                    }
                    let stub_path = jobs_dir.join(format!("{job_name}.rs"));
                    if stub_path.exists() {
                        continue;
                    }
                    // Ensure jobs/ directory exists before writing
                    fs::create_dir_all(jobs_dir).map_err(|e| {
                        format!("Failed to create {}: {e}", jobs_dir.display())
                    })?;
                    let stub = format!(
                        r#"pub async fn handle(
    _payload: serde_json::Value,
) -> Result<(), shaperail_core::ShaperailError> {{
    todo!("implement {job_name}")
}}
"#
                    );
                    fs::write(&stub_path, stub).map_err(|e| {
                        format!("Failed to write {}: {e}", stub_path.display())
                    })?;
                }
            }
        }
    }
    Ok(())
}
```

Then add `write_job_stubs` call to `run()` in generate.rs, alongside the controller stubs call:

```rust
            // Write controller stubs for any newly declared controllers
            if let Err(e) = write_controller_stubs(&resources, Path::new("resources")) {
                eprintln!("Warning: {e}");
            }
            // Write job stubs for any newly declared jobs
            if let Err(e) = write_job_stubs(&resources, Path::new("jobs")) {
                eprintln!("Warning: {e}");
            }
```

- [ ] **Step 12: Run the job stub tests — they should pass**

```bash
cargo test -p shaperail-cli job_stub 2>&1 | tail -10
```

Expected: 2 tests PASSED.

- [ ] **Step 13: Update the scaffold template in init.rs**

The scaffold template `main_rs` is the raw string starting at line 146 in `shaperail-cli/src/commands/init.rs`. Find the block after `let event_emitter = ...` and before `let jwt_config = ...` — this is the insertion point for job worker startup.

In the template, locate the pattern (around template line ~1018):
```rust
    let event_emitter = job_queue
        .clone()
        .map(|queue| EventEmitter::new(queue, config.events.as_ref()));
    let jwt_config = JwtConfig::from_env().map(Arc::new);
```

Replace with:
```rust
    let event_emitter = job_queue
        .clone()
        .map(|queue| EventEmitter::new(queue, config.events.as_ref()));

    let job_registry = generated::build_job_registry();
    if !job_registry.is_empty() {
        if let Some(ref jq) = job_queue {
            let (_tx, shutdown_rx) = tokio::sync::watch::channel(false);
            let worker = shaperail_runtime::jobs::Worker::new(
                jq.clone(),
                job_registry,
                std::time::Duration::from_secs(1),
            );
            tokio::spawn(async move { worker.spawn(shutdown_rx).await });
        }
    }

    let jwt_config = JwtConfig::from_env().map(Arc::new);
```

Also add `shaperail_runtime::jobs::Worker` to the template's use block. The template currently imports:
```rust
use shaperail_runtime::jobs::JobQueue;
```
Add `Worker` and `JobRegistry` to that line:
```rust
use shaperail_runtime::jobs::{JobQueue, JobRegistry, Worker};
```

Wait — the generated `build_job_registry()` returns `shaperail_runtime::jobs::JobRegistry`, and `is_empty()` is called on it. `Worker::new()` takes a `JobRegistry`. These types are already imported via the fully qualified paths in `generated/mod.rs`. But the template's `main.rs` is a separate compilation unit and its `use` imports only affect main.rs. Since `job_registry` and `Worker` are used in main.rs, we need either fully-qualified paths or the import. Add to the template's imports:

```rust
use shaperail_runtime::jobs::{JobQueue, JobRegistry, Worker};
```

- [ ] **Step 14: Run workspace tests to verify the template compiles**

The scaffold template is a string literal, not compiled code, so we can't directly compile-test it here. Run the CLI tests to verify the generate.rs changes compile:

```bash
cargo test -p shaperail-cli 2>&1 | tail -20
cargo clippy -p shaperail-codegen -p shaperail-runtime -p shaperail-cli -- -D warnings 2>&1 | tail -20
```

Expected: all pass (no regressions).

- [ ] **Step 15: Commit**

```bash
git add shaperail-codegen/src/rust.rs shaperail-codegen/tests/snapshot_tests.rs \
        shaperail-runtime/src/jobs/worker.rs \
        shaperail-cli/src/commands/generate.rs \
        shaperail-cli/src/commands/init.rs
git commit -m "feat(codegen,runtime,cli): generate job registry, add JobRegistry::is_empty, write job stubs, auto-start worker"
```

---

## Task 3: load_channels() + WebSocket scaffold wiring

**Context:** `load_channels()` reads `*.channel.yaml` files from a directory, parses each as `shaperail_core::ChannelDefinition` using `serde_yaml`, and returns the collection. Errors in individual files are logged and skipped (server starts normally). The scaffold template needs to call `load_channels()` before `HttpServer::new`, then loop over the results inside the app factory to call `configure_ws_routes()`. WebSocket routes require Redis (for `RedisPubSub`) and JWT. `RoomManager::new()` is always free to construct.

**Files:**
- Modify: `shaperail-runtime/src/ws/session.rs` — add `load_channels()` after `configure_ws_routes()`
- Modify: `shaperail-runtime/src/ws/mod.rs` — export `load_channels`
- Modify: `shaperail-cli/src/commands/init.rs` — update scaffold template

---

- [ ] **Step 1: Write failing tests for `load_channels()`**

Add to the `#[cfg(test)]` module in `shaperail-runtime/src/ws/session.rs`:

```rust
    #[test]
    fn load_channels_returns_empty_for_missing_dir() {
        let result = load_channels(std::path::Path::new("/nonexistent/path/to/channels"));
        assert!(result.is_empty());
    }

    #[test]
    fn load_channels_reads_valid_yaml_files() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"channel: notifications
auth: [member, admin]
rooms: true
"#;
        std::fs::write(dir.path().join("notifications.channel.yaml"), yaml).unwrap();

        let channels = load_channels(dir.path());
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].channel, "notifications");
        assert!(channels[0].rooms);
    }

    #[test]
    fn load_channels_skips_invalid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.channel.yaml"), "not: valid: yaml: [[[").unwrap();
        let channels = load_channels(dir.path());
        assert!(channels.is_empty(), "invalid yaml file should be skipped");
    }

    #[test]
    fn load_channels_ignores_non_channel_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("users.yaml"), "resource: users\nversion: 1\nschema: {}\n").unwrap();
        let channels = load_channels(dir.path());
        assert!(channels.is_empty(), "non-channel files should be ignored");
    }
```

Add `tempfile` to `shaperail-runtime/Cargo.toml` under `[dev-dependencies]` if not already present:
```toml
tempfile = "3"
```

- [ ] **Step 2: Run the tests — confirm they fail**

```bash
cargo test -p shaperail-runtime load_channels 2>&1 | tail -10
```

Expected: FAILED — `load_channels` not found.

- [ ] **Step 3: Implement `load_channels()` in session.rs**

Add this function at the end of `shaperail-runtime/src/ws/session.rs`, before the `#[cfg(test)]` block:

```rust
/// Reads all `*.channel.yaml` files from `dir` and returns parsed `ChannelDefinition` values.
/// Returns an empty vec if the directory does not exist or contains no channel files.
/// Files that fail to parse are skipped with a warning log.
pub fn load_channels(dir: &std::path::Path) -> Vec<shaperail_core::ChannelDefinition> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut channels = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".channel.yaml") {
            continue;
        }
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to read channel file");
                continue;
            }
        };
        match serde_yaml::from_str::<shaperail_core::ChannelDefinition>(&contents) {
            Ok(def) => channels.push(def),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to parse channel file");
            }
        }
    }
    channels
}
```

- [ ] **Step 4: Run the load_channels tests — they should pass**

```bash
cargo test -p shaperail-runtime load_channels 2>&1 | tail -10
```

Expected: 4 tests PASSED.

- [ ] **Step 5: Export `load_channels` from ws/mod.rs**

In `shaperail-runtime/src/ws/mod.rs`, update the exports:

```rust
pub use session::{configure_ws_routes, load_channels, WsChannelState};
```

- [ ] **Step 6: Update the scaffold template in init.rs — channels loading and WS route wiring**

In the scaffold template `main_rs` in `shaperail-cli/src/commands/init.rs`, make three changes:

**Change A** — Add to the template's use imports (alongside other `shaperail_runtime` imports):
```rust
use shaperail_runtime::ws::{load_channels, RedisPubSub, RoomManager};
```

**Change B** — After the `let redis_pool = ...` block and before `let cache = ...` (around template line ~1016), insert channel loading and WS state:
```rust
    let channels = load_channels(std::path::Path::new("channels/"));
    let ws_pubsub = redis_pool
        .as_ref()
        .map(|pool| RedisPubSub::new(pool.clone()));
    let room_manager = if channels.is_empty() {
        None
    } else {
        Some(RoomManager::new())
    };
```

**Change C** — Before the `HttpServer::new(move || {` block, add clones for the closure (alongside other `_clone` variables):
```rust
    let channels_clone = channels.clone();
    let ws_pubsub_clone = ws_pubsub.clone();
    let room_manager_clone = room_manager.clone();
```

**Change D** — Inside `HttpServer::new(move || {`, before the final `app.configure(...)` line, add WS route wiring. Currently the closure ends with:
```rust
        app.configure(|cfg| register_all_resources(cfg, &res, st))
```

Replace with:
```rust
        let ch = channels_clone.clone();
        let pubsub = ws_pubsub_clone.clone();
        let rm = room_manager_clone.clone();
        let jwt_ws = jwt_config_clone.clone();
        app.configure(move |cfg| {
            register_all_resources(cfg, &res, st);
            if let (Some(ref p), Some(ref r), Some(ref j)) = (&pubsub, &rm, &jwt_ws) {
                for channel in &ch {
                    shaperail_runtime::ws::configure_ws_routes(
                        cfg,
                        channel.clone(),
                        r.clone(),
                        p.clone(),
                        j.clone(),
                    );
                }
            }
        })
```

- [ ] **Step 7: Run full workspace tests**

```bash
cargo test -p shaperail-runtime -p shaperail-cli 2>&1 | tail -20
cargo clippy -p shaperail-runtime -p shaperail-cli -- -D warnings 2>&1 | tail -20
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add shaperail-runtime/src/ws/session.rs shaperail-runtime/src/ws/mod.rs \
        shaperail-runtime/Cargo.toml \
        shaperail-cli/src/commands/init.rs
git commit -m "feat(runtime,cli): add load_channels(), auto-register WebSocket routes from channel YAML"
```

---

## Task 4: InboundWebhookConfig.signature_header + wire configure_inbound_routes to scaffold

**Context:** `configure_inbound_routes()` already exists in `shaperail-runtime/src/events/inbound.rs` and is re-exported from `shaperail-runtime/src/events/mod.rs`. The only gap is that the scaffold template doesn't call it. This task also adds a `signature_header` field to `InboundWebhookConfig` in `shaperail-core/src/config.rs` (the struct has `#[serde(deny_unknown_fields)]` — add the field with a default). The field is threaded through `InboundWebhookState` so callers can inspect it.

**Files:**
- Modify: `shaperail-core/src/config.rs:406-419` — add `signature_header` to `InboundWebhookConfig`
- Modify: `shaperail-runtime/src/events/inbound.rs` — add `signature_header` to `InboundWebhookState`; update `configure_inbound_routes` to pass it
- Modify: `shaperail-cli/src/commands/init.rs` — update scaffold template

---

- [ ] **Step 1: Write failing tests for `signature_header` field**

Add to the `#[cfg(test)]` block in `shaperail-core/src/config.rs`:

```rust
    #[test]
    fn inbound_config_default_signature_header() {
        let yaml = r#"path: /webhooks/github
secret_env: GITHUB_SECRET
"#;
        let cfg: InboundWebhookConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.signature_header, "X-Webhook-Signature");
    }

    #[test]
    fn inbound_config_custom_signature_header() {
        let yaml = r#"path: /webhooks/github
secret_env: GITHUB_SECRET
signature_header: X-Hub-Signature-256
"#;
        let cfg: InboundWebhookConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.signature_header, "X-Hub-Signature-256");
    }
```

- [ ] **Step 2: Run the tests — confirm they fail**

```bash
cargo test -p shaperail-core inbound_config 2>&1 | tail -10
```

Expected: FAILED — field `signature_header` not found on `InboundWebhookConfig`.

- [ ] **Step 3: Add `signature_header` field to `InboundWebhookConfig` in config.rs**

In `shaperail-core/src/config.rs`, update `InboundWebhookConfig` (lines 407–419):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboundWebhookConfig {
    /// URL path for the inbound webhook (e.g., "/webhooks/stripe").
    pub path: String,

    /// Environment variable holding the verification secret.
    pub secret_env: String,

    /// Event names this endpoint accepts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,

    /// HTTP header carrying the HMAC-SHA256 signature.
    /// Defaults to `X-Webhook-Signature`. Use `X-Hub-Signature-256` for GitHub, `Stripe-Signature` for Stripe.
    #[serde(default = "default_signature_header")]
    pub signature_header: String,
}

fn default_signature_header() -> String {
    "X-Webhook-Signature".to_string()
}
```

- [ ] **Step 4: Run the tests — they should pass**

```bash
cargo test -p shaperail-core inbound_config 2>&1 | tail -10
```

Expected: 2 tests PASSED.

- [ ] **Step 5: Thread `signature_header` through `InboundWebhookState` in inbound.rs**

In `shaperail-runtime/src/events/inbound.rs`, update `InboundWebhookState` to carry the field, and update `configure_inbound_routes` to populate it:

```rust
pub struct InboundWebhookState {
    pub secret: String,
    pub accepted_events: Vec<String>,
    pub emitter: EventEmitter,
    /// Which HTTP header carries the HMAC signature for this endpoint.
    pub signature_header: String,
}

pub fn configure_inbound_routes(
    cfg: &mut web::ServiceConfig,
    configs: &[InboundWebhookConfig],
    emitter: &EventEmitter,
) {
    for config in configs {
        let secret = std::env::var(&config.secret_env).unwrap_or_default();
        let state = web::Data::new(InboundWebhookState {
            secret,
            accepted_events: config.events.clone(),
            emitter: emitter.clone(),
            signature_header: config.signature_header.clone(),
        });

        let path = config.path.clone();
        cfg.service(
            web::resource(&path)
                .app_data(state)
                .route(web::post().to(handle_inbound_webhook)),
        );
    }
}
```

- [ ] **Step 6: Run runtime tests to confirm no regressions**

```bash
cargo test -p shaperail-runtime 2>&1 | tail -20
```

Expected: all pass (the existing inbound tests in inbound.rs should still pass).

- [ ] **Step 7: Update the scaffold template in init.rs — wire configure_inbound_routes**

In the scaffold template `main_rs` in `shaperail-cli/src/commands/init.rs`, make two changes:

**Change A** — Add to the template's use imports:
```rust
use shaperail_runtime::events::configure_inbound_routes;
```

**Change B** — Inside `HttpServer::new(move || {`, inside the `app.configure(move |cfg| { ... })` closure added in Task 3, add inbound route registration alongside the WebSocket wiring. The full updated `app.configure` block becomes:

```rust
        let ch = channels_clone.clone();
        let pubsub = ws_pubsub_clone.clone();
        let rm = room_manager_clone.clone();
        let jwt_ws = jwt_config_clone.clone();
        app.configure(move |cfg| {
            register_all_resources(cfg, &res, st);
            if let (Some(ref p), Some(ref r), Some(ref j)) = (&pubsub, &rm, &jwt_ws) {
                for channel in &ch {
                    shaperail_runtime::ws::configure_ws_routes(
                        cfg,
                        channel.clone(),
                        r.clone(),
                        p.clone(),
                        j.clone(),
                    );
                }
            }
            if let Some(ref events_cfg) = config_events_clone {
                if !events_cfg.inbound.is_empty() {
                    if let Some(ref emitter) = emitter_clone {
                        configure_inbound_routes(cfg, &events_cfg.inbound, emitter);
                    }
                }
            }
        })
```

This requires two new clone variables added before `HttpServer::new`. Add alongside the other `_clone` variables:
```rust
    let config_events_clone = config.events.clone();
    let emitter_clone = event_emitter.clone();
```

(`event_emitter` was moved into `AppState` but we cloned it before that — or we create the clone here before the move. Check the template order: `event_emitter` is created at line ~1018 and moved into `AppState` at line ~1040. Add the clone *before* the `AppState` construction, e.g. immediately after `let event_emitter = ...`:

```rust
    let event_emitter = job_queue
        .clone()
        .map(|queue| EventEmitter::new(queue, config.events.as_ref()));
    let emitter_for_inbound = event_emitter.clone(); // clone before move into AppState
```

Then use `emitter_for_inbound` instead of `emitter_clone` in the scaffold — rename the variable above accordingly.

- [ ] **Step 8: Run full workspace check**

```bash
cargo test --workspace 2>&1 | tail -30
cargo clippy --workspace -- -D warnings 2>&1 | tail -20
cargo fmt --check 2>&1
```

Expected: all pass.

- [ ] **Step 9: Commit**

```bash
git add shaperail-core/src/config.rs \
        shaperail-runtime/src/events/inbound.rs \
        shaperail-cli/src/commands/init.rs
git commit -m "feat(core,runtime,cli): add signature_header to InboundWebhookConfig, wire configure_inbound_routes to scaffold"
```

---

## Self-Review

**Spec coverage check:**
- ✅ Change 1 (controllers) → Task 1: `generate_registry_module()` populated, `#[path]` modules, stubs
- ✅ Change 2 (jobs) → Task 2: `generate_job_registry()`, `is_empty()`, job stubs, scaffold worker startup
- ✅ Change 3 (WebSockets) → Task 3: `load_channels()`, exported, scaffold wiring
- ✅ Change 4 (events inbound) → Task 4: `signature_header` field, `InboundWebhookState` updated, scaffold wiring

**No placeholders found.**

**Type consistency check:**
- `ControllerMap::register(resource: &str, name: &str, f: impl ControllerHandler)` — controller stubs use `async fn name(ctx: &mut ControllerContext)` which satisfies the `ControllerHandler` blanket impl ✅
- `JobHandler = Arc<dyn Fn(Value) -> Pin<Box<dyn Future<...> + Send>> + Send + Sync>` — generated code uses explicit cast ✅
- `Worker::new(queue: JobQueue, registry: JobRegistry, poll_interval: Duration)` — scaffold passes correct types ✅
- `configure_ws_routes(cfg, definition: ChannelDefinition, room_manager: RoomManager, pubsub: RedisPubSub, jwt_config: Arc<JwtConfig>)` — scaffold passes clones of correct types ✅
- `configure_inbound_routes(cfg, configs: &[InboundWebhookConfig], emitter: &EventEmitter)` — scaffold borrows correctly ✅
