pub mod build;
pub mod check;
pub mod diff;
pub mod doctor;
pub mod explain;
pub mod explain_format;
pub mod export;
pub mod generate;
pub mod init;
pub mod jobs_status;
pub mod llm_context;
pub mod migrate;
pub mod resource;
pub mod routes;
pub mod seed;
pub mod serve;
pub mod test;
pub mod validate;
pub mod workspace;

use std::path::{Path, PathBuf};

use shaperail_core::ResourceDefinition;

/// Collect all canonical `.yaml` resource files from a directory.
pub fn collect_resource_files(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "yaml" {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Parse all resource files from the resources/ directory.
pub fn load_all_resources() -> Result<Vec<ResourceDefinition>, String> {
    load_all_resources_from(Path::new("resources"))
}

pub fn load_all_resources_from(resources_dir: &Path) -> Result<Vec<ResourceDefinition>, String> {
    if !resources_dir.is_dir() {
        return Err(
            "No resources/ directory found. Run this from a Shaperail project root.".into(),
        );
    }
    let files = collect_resource_files(resources_dir)
        .map_err(|e| format!("Failed to read resources/ directory: {e}"))?;
    if files.is_empty() {
        return Err("No resource files found in resources/".into());
    }

    let mut resources = Vec::new();
    for file in &files {
        let rd = shaperail_codegen::parser::parse_resource_file(file).map_err(|e| e.to_string())?;
        let validation_errors = shaperail_codegen::validator::validate_resource(&rd);
        if !validation_errors.is_empty() {
            let rendered = validation_errors
                .into_iter()
                .map(|err| err.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!("{}: {rendered}", file.display()));
        }
        resources.push(rd);
    }
    Ok(resources)
}

/// Load shaperail.config.yaml from the current directory.
pub fn load_config() -> Result<shaperail_core::ProjectConfig, String> {
    let path = Path::new("shaperail.config.yaml");
    if !path.exists() {
        return Err(
            "No shaperail.config.yaml found. Run this from a Shaperail project root.".into(),
        );
    }
    shaperail_codegen::config_parser::parse_config_file(path)
        .map_err(|e| format!("Failed to parse shaperail.config.yaml: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_resource_files_only_includes_yaml() {
        let temp = tempfile::tempdir().unwrap();
        let resources = temp.path();

        std::fs::write(resources.join("users.yaml"), "resource: users").unwrap();
        std::fs::write(resources.join("users.yml"), "resource: users").unwrap();
        std::fs::write(resources.join("README.md"), "# docs").unwrap();

        let files = collect_resource_files(resources).unwrap();

        assert_eq!(files, vec![resources.join("users.yaml")]);
    }
}
