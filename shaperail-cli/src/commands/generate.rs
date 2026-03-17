use std::fs;
use std::path::{Path, PathBuf};

use shaperail_core::ResourceDefinition;

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
