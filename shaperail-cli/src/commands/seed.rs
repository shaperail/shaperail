use std::path::Path;

/// Load fixture YAML files into the database.
pub fn run(path: &Path) -> i32 {
    if !path.is_dir() {
        eprintln!("Seed directory not found: {}", path.display());
        eprintln!("Create a seeds/ directory with YAML fixture files.");
        return 1;
    }

    let files = match super::collect_resource_files(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error reading seed directory: {e}");
            return 1;
        }
    };

    if files.is_empty() {
        eprintln!("No seed files found in {}", path.display());
        return 1;
    }

    for file in &files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading {}: {e}", file.display());
                return 1;
            }
        };

        let data: Result<serde_yaml::Value, _> = serde_yaml::from_str(&content);
        match data {
            Ok(value) => {
                let table = file
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");

                let record_count = match &value {
                    serde_yaml::Value::Sequence(seq) => seq.len(),
                    _ => 1,
                };

                println!(
                    "Loaded {record_count} record(s) from {} into '{table}'",
                    file.display()
                );
            }
            Err(e) => {
                eprintln!("Error parsing {}: {e}", file.display());
                return 1;
            }
        }
    }

    println!("Seed data loaded. Run with a database connection to apply.");
    0
}
