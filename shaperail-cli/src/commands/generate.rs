use std::fs;
use std::path::{Path, PathBuf};

use shaperail_core::{ResourceDefinition, WASM_HOOK_PREFIX};

use super::load_all_resources;

/// Run codegen for all resource files, writing typed Rust modules to generated/.
pub fn run() -> i32 {
    let resources = match load_all_resources() {
        Ok(resources) => resources,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    // Feature flag guardrails: warn early if resources use features that may not be enabled
    let required_features = shaperail_codegen::feature_check::check_required_features(&resources);
    if !required_features.is_empty() {
        let warnings =
            shaperail_codegen::feature_check::format_feature_warnings(&required_features);
        eprintln!("{warnings}");
    }

    match write_generated_modules(&resources, Path::new("generated")) {
        Ok(paths) => {
            for path in &paths {
                println!("Generated {}", path.display());
            }
            println!(
                "Generated {} resource module(s) in generated/",
                resources.len()
            );

            // Write controller stubs for any newly declared controllers
            if let Err(e) = write_controller_stubs(&resources, Path::new("resources")) {
                eprintln!("Warning: {e}");
            }

            if let Err(e) = write_job_stubs(&resources, Path::new("jobs")) {
                eprintln!("Warning: {e}");
            }

            0
        }
        Err(e) => {
            eprintln!("Error: {e}");
            1
        }
    }
}

pub(crate) fn write_generated_modules(
    resources: &[ResourceDefinition],
    generated_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(generated_dir)
        .map_err(|e| format!("Failed to create {}: {e}", generated_dir.display()))?;

    clear_generated_rust_files(generated_dir)?;

    let generated = shaperail_codegen::rust::generate_project(resources)?;
    let mut written = Vec::with_capacity(generated.modules.len() + 1);

    for module in generated.modules {
        let path = generated_dir.join(module.file_name);
        fs::write(&path, module.contents)
            .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
        written.push(path);
    }

    let mod_path = generated_dir.join("mod.rs");
    fs::write(&mod_path, generated.mod_rs)
        .map_err(|e| format!("Failed to write {}: {e}", mod_path.display()))?;
    written.push(mod_path);

    Ok(written)
}

fn clear_generated_rust_files(generated_dir: &Path) -> Result<(), String> {
    let entries = fs::read_dir(generated_dir)
        .map_err(|e| format!("Failed to read {}: {e}", generated_dir.display()))?;

    for entry in entries {
        let entry =
            entry.map_err(|e| format!("Failed to inspect {}: {e}", generated_dir.display()))?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            fs::remove_file(&path)
                .map_err(|e| format!("Failed to remove {}: {e}", path.display()))?;
        }
    }

    Ok(())
}

/// Writes a stub controller file for each resource that declares native controller hooks,
/// if the file does not already exist. Never overwrites existing files.
pub(crate) fn write_controller_stubs(
    resources: &[ResourceDefinition],
    resources_dir: &Path,
) -> Result<(), String> {
    fs::create_dir_all(resources_dir)
        .map_err(|e| format!("Failed to create {}: {e}", resources_dir.display()))?;

    for resource in resources {
        let Some(endpoints) = &resource.endpoints else {
            continue;
        };

        // Collect (fn_name, is_after) pairs for non-WASM hooks
        let hook_entries: Vec<(&str, bool)> = endpoints
            .iter()
            .filter_map(|(_, ep)| ep.controller.as_ref())
            .flat_map(|c| {
                let before = c
                    .before
                    .as_deref()
                    .filter(|s| !s.starts_with(WASM_HOOK_PREFIX))
                    .map(|name| (name, false));
                let after = c
                    .after
                    .as_deref()
                    .filter(|s| !s.starts_with(WASM_HOOK_PREFIX))
                    .map(|name| (name, true));
                [before, after].into_iter().flatten()
            })
            .collect();

        if hook_entries.is_empty() {
            continue;
        }

        let stub_path = resources_dir.join(format!("{}.controller.rs", resource.resource));
        if stub_path.exists() {
            continue;
        }

        let mut lines = Vec::new();
        for (fn_name, is_after) in &hook_entries {
            let kind_comment = if *is_after {
                "// After-hook: called after the DB write. Access the result via `ctx.data`.\n"
            } else {
                "// Before-hook: called before the DB write. Modify input via `ctx.input`.\n"
            };
            lines.push(format!(
                r#"{kind_comment}pub async fn {fn_name}(
    ctx: &mut shaperail_runtime::handlers::ControllerContext,
) -> Result<(), shaperail_core::ShaperailError> {{
    todo!("implement {fn_name}")
}}
"#
            ));
        }

        fs::write(&stub_path, lines.join("\n"))
            .map_err(|e| format!("Failed to write {}: {e}", stub_path.display()))?;
    }
    Ok(())
}

/// Writes a stub job handler file for each unique job name declared across resources,
/// if the file does not already exist. Never overwrites existing files.
pub(crate) fn write_job_stubs(
    resources: &[ResourceDefinition],
    jobs_dir: &Path,
) -> Result<(), String> {
    // Collect unique job names across all resources
    let mut all_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for resource in resources {
        if let Some(endpoints) = &resource.endpoints {
            for (_, ep) in endpoints {
                for job_name in ep.jobs.as_deref().unwrap_or_default() {
                    all_names.insert(job_name.clone());
                }
            }
        }
    }

    if all_names.is_empty() {
        return Ok(());
    }

    fs::create_dir_all(jobs_dir)
        .map_err(|e| format!("Failed to create {}: {e}", jobs_dir.display()))?;

    for job_name in &all_names {
        let stub_path = jobs_dir.join(format!("{job_name}.rs"));
        if stub_path.exists() {
            continue;
        }
        let stub = format!(
            r#"pub async fn handle(
    _payload: serde_json::Value,
) -> Result<(), shaperail_core::ShaperailError> {{
    todo!("implement {job_name}")
}}
"#
        );
        fs::write(&stub_path, stub)
            .map_err(|e| format!("Failed to write {}: {e}", stub_path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shaperail_codegen::parser::parse_resource;

    #[test]
    fn controller_stub_written_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        // Intentionally do NOT pre-create resources_dir — write_controller_stubs must create it
        let resources_dir = dir.path().join("resources");

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
      after: audit_order
"#;
        let rd = parse_resource(yaml).unwrap();
        write_controller_stubs(&[rd], &resources_dir).unwrap();

        let stub_path = resources_dir.join("orders.controller.rs");
        assert!(stub_path.exists(), "stub file should be created");
        let contents = std::fs::read_to_string(&stub_path).unwrap();
        assert!(
            contents.contains("check_inventory"),
            "stub should contain before-hook function name"
        );
        assert!(
            contents.contains("audit_order"),
            "stub should contain after-hook function name"
        );
        assert!(
            contents.contains("todo!"),
            "stub should have todo! placeholder"
        );
        // ALL hooks (before and after) must return Result<(), ShaperailError>
        let occurrences = contents
            .matches("Result<(), shaperail_core::ShaperailError>")
            .count();
        assert_eq!(
            occurrences, 2,
            "both before-hook and after-hook stubs must return Result<(), shaperail_core::ShaperailError>"
        );
        assert!(
            !contents.contains("serde_json::Value"),
            "after-hook stub must NOT use serde_json::Value as return type"
        );
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
        assert_eq!(
            contents, "// existing content",
            "existing file must not be overwritten"
        );
    }

    #[test]
    fn job_stub_written_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let jobs_dir = dir.path().join("jobs");
        // Do NOT pre-create jobs_dir — write_job_stubs must create it

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
        assert!(contents.contains("pub async fn handle"));
        assert!(contents.contains("todo!"));
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
}
