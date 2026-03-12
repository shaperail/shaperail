use std::path::Path;

use super::collect_resource_files;

/// Validate all resource files, reporting errors.
pub fn run(path: &Path) -> i32 {
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
        match shaperail_codegen::parser::parse_resource_file(file) {
            Ok(rd) => {
                let errors = shaperail_codegen::validator::validate_resource(&rd);
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
