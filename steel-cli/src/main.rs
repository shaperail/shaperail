use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(name = "steel", about = "SteelAPI — AI-Native Rust Backend Framework")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate resource YAML files
    Validate {
        /// Path to a resource file or directory of resource files
        path: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { path } => {
            let exit_code = cmd_validate(&path);
            process::exit(exit_code);
        }
    }
}

fn cmd_validate(path: &std::path::Path) -> i32 {
    let files = if path.is_dir() {
        match collect_resource_files(path) {
            Ok(files) => files,
            Err(e) => {
                eprintln!("Error reading directory: {e}");
                return 1;
            }
        }
    } else {
        vec![path.to_path_buf()]
    };

    if files.is_empty() {
        eprintln!("No resource files found in {}", path.display());
        return 1;
    }

    let mut has_errors = false;

    for file in &files {
        match steel_codegen::parser::parse_resource_file(file) {
            Ok(rd) => {
                let errors = steel_codegen::validator::validate_resource(&rd);
                if errors.is_empty() {
                    println!("\u{2713} {} valid", file.display());
                } else {
                    has_errors = true;
                    eprintln!("\u{2717} {}", file.display());
                    for err in &errors {
                        eprintln!("  - {err}");
                    }
                }
            }
            Err(e) => {
                has_errors = true;
                eprintln!("\u{2717} {}: {e}", file.display());
            }
        }
    }

    if has_errors {
        1
    } else {
        0
    }
}

fn collect_resource_files(dir: &std::path::Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "yaml" || ext == "yml" {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    Ok(files)
}
