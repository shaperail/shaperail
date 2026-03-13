use std::path::Path;
use std::process::Command;

/// Start dev server with hot reload via cargo-watch.
pub fn run(port: Option<u16>, check: bool) -> i32 {
    let config = match super::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    let resources = match super::load_all_resources() {
        Ok(resources) => resources,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    if let Err(e) = validate_project_layout() {
        eprintln!("Error: {e}");
        return 1;
    }

    if let Err(e) = super::generate::write_generated_modules(&resources, Path::new("generated")) {
        eprintln!("Error generating typed query modules: {e}");
        return 1;
    }

    let port = port.unwrap_or(config.port);

    // Try cargo-watch first for hot reload
    let has_cargo_watch = Command::new("cargo")
        .args(["watch", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let command = serve_command(has_cargo_watch);

    if check {
        println!("Serve check passed.");
        println!("Project: {}", config.project);
        println!("Resources: {}", resources.len());
        println!("Port: {port}");
        println!(
            "Hot reload: {}",
            if has_cargo_watch {
                "enabled"
            } else {
                "disabled"
            }
        );
        println!("Command: cargo {}", command.join(" "));
        return 0;
    }

    if has_cargo_watch {
        println!("Starting dev server on port {port} with hot reload...");
        let status = Command::new("cargo")
            .args(&command)
            .env("SHAPERAIL_PORT", port.to_string())
            .status();

        match status {
            Ok(s) => s.code().unwrap_or(1),
            Err(e) => {
                eprintln!("Failed to start cargo-watch: {e}");
                1
            }
        }
    } else {
        println!("cargo-watch not found, starting without hot reload...");
        println!("Install cargo-watch for hot reload: cargo install cargo-watch");
        println!("Starting dev server on port {port}...");

        let status = Command::new("cargo")
            .args(&command)
            .env("SHAPERAIL_PORT", port.to_string())
            .status();

        match status {
            Ok(s) => s.code().unwrap_or(1),
            Err(e) => {
                eprintln!("Failed to start server: {e}");
                1
            }
        }
    }
}

fn validate_project_layout() -> Result<(), String> {
    if !Path::new("Cargo.toml").is_file() {
        return Err("No Cargo.toml found. Run this from a Shaperail project root.".into());
    }

    if !Path::new("src/main.rs").is_file() {
        return Err(
            "No src/main.rs found. Run `shaperail init <name>` to scaffold a project.".into(),
        );
    }

    Ok(())
}

fn serve_command(has_cargo_watch: bool) -> Vec<&'static str> {
    if has_cargo_watch {
        vec![
            "watch",
            "-s",
            "shaperail generate",
            "-x",
            "run",
            "-w",
            "src",
            "-w",
            "resources",
            "-w",
            "generated",
        ]
    } else {
        vec!["run"]
    }
}
