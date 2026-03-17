use std::fs;
use std::path::Path;

/// Show what codegen would change without writing files.
/// Compares the current generated/ directory against what would be generated.
pub fn run() -> i32 {
    let resources = match super::load_all_resources() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    let generated = match shaperail_codegen::rust::generate_project(&resources) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Error generating code: {e}");
            return 1;
        }
    };

    let generated_dir = Path::new("generated");
    let mut has_changes = false;

    // Check mod.rs
    let mod_path = generated_dir.join("mod.rs");
    if let Some(diff) = diff_file(&mod_path, &generated.mod_rs) {
        has_changes = true;
        println!("{diff}");
    }

    // Check each module
    for module in &generated.modules {
        let file_path = generated_dir.join(&module.file_name);
        if let Some(diff) = diff_file(&file_path, &module.contents) {
            has_changes = true;
            println!("{diff}");
        }
    }

    // Check for files that would be removed
    if generated_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(generated_dir) {
            let expected_files: std::collections::HashSet<String> = generated
                .modules
                .iter()
                .map(|m| m.file_name.clone())
                .chain(std::iter::once("mod.rs".to_string()))
                .collect();

            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                    let file_name = path.file_name().unwrap().to_string_lossy().to_string();
                    if !expected_files.contains(&file_name) {
                        has_changes = true;
                        println!("--- {}", path.display());
                        println!("+++ /dev/null");
                        println!("(file would be removed)");
                        println!();
                    }
                }
            }
        }
    }

    if !has_changes {
        println!("No changes. Generated code is up to date.");
    }

    0
}

/// Compare a file on disk against expected contents.
/// Returns a diff summary if they differ, or None if identical/file doesn't exist yet.
fn diff_file(path: &Path, expected: &str) -> Option<String> {
    let existing = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => {
            // File doesn't exist yet — it would be created
            let line_count = expected.lines().count();
            return Some(format!(
                "--- /dev/null\n+++ {}\n(new file, {} lines)\n",
                path.display(),
                line_count
            ));
        }
    };

    if existing == expected {
        return None;
    }

    // Simple line-based diff summary
    let old_lines: Vec<&str> = existing.lines().collect();
    let new_lines: Vec<&str> = expected.lines().collect();

    let mut output = format!(
        "--- {}\n+++ {} (regenerated)\n",
        path.display(),
        path.display()
    );

    let mut added = 0;
    let mut removed = 0;

    // Count added/removed lines (simple comparison, not a proper diff algorithm)
    let max_len = old_lines.len().max(new_lines.len());
    for i in 0..max_len {
        match (old_lines.get(i), new_lines.get(i)) {
            (Some(old), Some(new)) if old != new => {
                removed += 1;
                added += 1;
            }
            (Some(_), None) => removed += 1,
            (None, Some(_)) => added += 1,
            _ => {}
        }
    }

    output.push_str(&format!(
        "{} lines changed (+{added} -{removed})\n\n",
        added + removed
    ));

    Some(output)
}
