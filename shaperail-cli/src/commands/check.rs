use std::path::Path;

use super::collect_resource_files;

/// Validate resource files with structured fix suggestions.
/// With `--json`, outputs machine-readable diagnostics for LLM consumption.
pub fn run(path: &Path, json_output: bool) -> i32 {
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
    let mut all_diagnostics: Vec<serde_json::Value> = Vec::new();

    for file in &files {
        match shaperail_codegen::parser::parse_resource_file(file) {
            Ok(rd) => {
                let diags = shaperail_codegen::diagnostics::diagnose_resource(&rd);
                if diags.is_empty() {
                    if !json_output {
                        println!("\u{2713} {} valid", file.display());
                    }
                } else {
                    has_errors = true;
                    if json_output {
                        for d in &diags {
                            all_diagnostics.push(serde_json::json!({
                                "file": file.display().to_string(),
                                "code": d.code,
                                "error": d.error,
                                "fix": d.fix,
                                "example": d.example,
                            }));
                        }
                    } else {
                        eprintln!("\u{2717} {}", file.display());
                        for d in &diags {
                            eprintln!("  [{}] {}", d.code, d.error);
                            eprintln!("    Fix: {}", d.fix);
                            eprintln!("    Example: {}", d.example);
                        }
                    }
                }
            }
            Err(e) => {
                has_errors = true;
                if json_output {
                    all_diagnostics.push(serde_json::json!({
                        "file": file.display().to_string(),
                        "code": "SR000",
                        "error": format!("YAML parse error: {e}"),
                        "fix": "fix the YAML syntax error shown above",
                        "example": "resource: <name>\nversion: 1\nschema:\n  id: { type: uuid, primary: true, generated: true }",
                    }));
                } else {
                    eprintln!("\u{2717} {}: {e}", file.display());
                }
            }
        }
    }

    if json_output {
        match serde_json::to_string_pretty(&all_diagnostics) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("JSON serialization failed: {e}");
                return 1;
            }
        }
    }

    if has_errors {
        1
    } else {
        0
    }
}
