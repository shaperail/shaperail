use std::path::Path;
use std::process::{Child, Command};

/// Start all services declared in shaperail.workspace.yaml.
///
/// Services are started in dependency order (topological sort).
/// Each service runs in its own process with its own port.
pub fn run_serve() -> i32 {
    let config = match load_workspace_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    // Load sagas if present
    match load_all_sagas() {
        Ok(sagas) if !sagas.is_empty() => {
            println!(
                "Loaded {} saga(s): {}",
                sagas.len(),
                sagas
                    .iter()
                    .map(|s| s.saga.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Err(e) => {
            eprintln!("Warning: Failed to load sagas: {e}");
        }
        _ => {}
    }

    println!(
        "Starting workspace '{}' with {} service(s)...",
        config.workspace,
        config.services.len()
    );

    // Compute startup order respecting depends_on
    let order = shaperail_codegen::workspace_parser::topological_order(&config);

    // Validate all service directories exist and have required files
    for name in &order {
        let svc = match config.services.get(name) {
            Some(s) => s,
            None => continue,
        };
        let svc_path = Path::new(&svc.path);
        if !svc_path.is_dir() {
            eprintln!(
                "Error: Service '{name}' directory '{}' does not exist.",
                svc.path
            );
            return 1;
        }
        if !svc_path.join("shaperail.config.yaml").exists() {
            eprintln!(
                "Error: Service '{name}' is missing shaperail.config.yaml in '{}'.",
                svc.path
            );
            return 1;
        }
    }

    // Load and validate resources for each service
    for name in &order {
        let svc = match config.services.get(name) {
            Some(s) => s,
            None => continue,
        };
        let resources_dir = Path::new(&svc.path).join("resources");
        if resources_dir.is_dir() {
            match super::load_all_resources_from(&resources_dir) {
                Ok(resources) => {
                    println!(
                        "  {name}: {} resource(s) on port {}",
                        resources.len(),
                        svc.port
                    );
                }
                Err(e) => {
                    eprintln!("Error: Service '{name}' resource validation failed: {e}");
                    return 1;
                }
            }
        } else {
            println!("  {name}: no resources/ directory (port {})", svc.port);
        }
    }

    // Start each service as a child process
    let mut children: Vec<(String, Child)> = Vec::new();

    for name in &order {
        let svc = match config.services.get(name) {
            Some(s) => s,
            None => continue,
        };

        println!("Starting service '{name}' on port {}...", svc.port);

        let child = Command::new("cargo")
            .args(["run"])
            .current_dir(&svc.path)
            .env("SHAPERAIL_PORT", svc.port.to_string())
            .spawn();

        match child {
            Ok(child) => {
                children.push((name.clone(), child));
            }
            Err(e) => {
                eprintln!("Error: Failed to start service '{name}': {e}");
                // Kill already-started services
                for (child_name, mut child) in children {
                    eprintln!("Stopping service '{child_name}'...");
                    let _ = child.kill();
                }
                return 1;
            }
        }
    }

    println!("\nAll {} service(s) started.", children.len());
    println!("Press Ctrl+C to stop all services.\n");

    // Wait for any child to exit (indicates a crash)
    loop {
        let mut exited = None;
        for (idx, (name, child)) in children.iter_mut().enumerate() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    eprintln!(
                        "\nService '{name}' exited with status: {}",
                        status.code().unwrap_or(-1)
                    );
                    exited = Some((idx, status.code().unwrap_or(1)));
                    break;
                }
                Ok(None) => {} // still running
                Err(e) => {
                    eprintln!("Error checking service '{name}': {e}");
                }
            }
        }
        if let Some((exited_idx, code)) = exited {
            // Stop all remaining services
            for (idx, (name, child)) in children.iter_mut().enumerate() {
                if idx != exited_idx {
                    eprintln!("Stopping service '{name}'...");
                    let _ = child.kill();
                }
            }
            return code;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Load workspace config from shaperail.workspace.yaml.
pub fn load_workspace_config() -> Result<shaperail_core::WorkspaceConfig, String> {
    let path = Path::new("shaperail.workspace.yaml");
    if !path.exists() {
        return Err(
            "No shaperail.workspace.yaml found. Run this from a Shaperail workspace root.".into(),
        );
    }
    shaperail_codegen::workspace_parser::parse_workspace_file(path)
        .map_err(|e| format!("Failed to parse shaperail.workspace.yaml: {e}"))
}

/// Load all saga definitions from the sagas/ directory.
pub fn load_all_sagas() -> Result<Vec<shaperail_core::SagaDefinition>, String> {
    let sagas_dir = Path::new("sagas");
    if !sagas_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut sagas = Vec::new();
    let entries = std::fs::read_dir(sagas_dir)
        .map_err(|e| format!("Failed to read sagas/ directory: {e}"))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read saga entry: {e}"))?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "yaml") {
            let saga = shaperail_codegen::workspace_parser::parse_saga_file(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            sagas.push(saga);
        }
    }

    Ok(sagas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_workspace_missing_file_returns_error() {
        // Run from a temp dir that has no workspace file
        let _temp = tempfile::tempdir().unwrap();
        // load_workspace_config looks in cwd; this test just verifies the function exists
        // and has the correct return type
        let result = load_workspace_config();
        assert!(result.is_err());
    }
}
