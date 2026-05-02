use std::fs;
use std::path::Path;

use shaperail_core::{FieldType, ResourceDefinition};

/// Generate and apply SQL migrations from resource files, or rollback.
pub fn run(rollback: bool) -> i32 {
    if rollback {
        return run_rollback();
    }

    let resources = match super::load_all_resources() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    let migrations_dir = Path::new("migrations");
    if let Err(e) = fs::create_dir_all(migrations_dir) {
        eprintln!("Error creating migrations/ directory: {e}");
        return 1;
    }

    // Compute the next free version once; increment locally per emitted file
    // so multi-resource invocations get distinct numbers.
    let mut next_version = compute_next_version(&list_migration_files(migrations_dir));

    // Generate migration SQL from resource definitions
    for resource in &resources {
        let migration_name = format!("create_{}", resource.resource);
        let sql = render_migration_sql(resource);

        if migration_exists(migrations_dir, &migration_name) {
            println!(
                "Migration for '{}' already exists, skipping",
                resource.resource
            );
            continue;
        }

        let filename = format!("{next_version:04}_{migration_name}.sql");
        let path = migrations_dir.join(&filename);
        next_version += 1;

        if let Err(e) = fs::write(&path, &sql) {
            eprintln!("Error writing migration {}: {e}", path.display());
            return 1;
        }
        println!("Generated {}", path.display());
    }

    // Apply migrations via sqlx
    println!("Applying migrations...");
    let status = std::process::Command::new("sqlx")
        .args(["migrate", "run", "--source", "migrations"])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("Migrations applied successfully");
            0
        }
        Ok(_) => {
            eprintln!("Migration apply failed. Is sqlx-cli installed? (cargo install sqlx-cli)");
            1
        }
        Err(e) => {
            eprintln!("Failed to run sqlx migrate: {e}");
            eprintln!("Install sqlx-cli: cargo install sqlx-cli");
            1
        }
    }
}

fn run_rollback() -> i32 {
    println!("Rolling back last migration...");
    let status = std::process::Command::new("sqlx")
        .args(["migrate", "revert", "--source", "migrations"])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("Rollback completed");
            0
        }
        Ok(_) => {
            eprintln!("Rollback failed");
            1
        }
        Err(e) => {
            eprintln!("Failed to run sqlx migrate revert: {e}");
            1
        }
    }
}

pub(crate) fn render_migration_sql(resource: &ResourceDefinition) -> String {
    let mut sql = String::new();
    if resource_requires_pgcrypto(resource) {
        sql.push_str("CREATE EXTENSION IF NOT EXISTS \"pgcrypto\";\n\n");
    }
    sql.push_str(&shaperail_runtime::db::build_create_table_sql(resource));
    sql.push_str(";\n");
    sql
}

fn resource_requires_pgcrypto(resource: &ResourceDefinition) -> bool {
    resource
        .schema
        .values()
        .any(|field| field.field_type == FieldType::Uuid && field.generated)
}

/// Returns the next free integer prefix above the highest existing one.
/// Robust to gaps and non-`_create_*` migrations (e.g. hand-written invariants files).
fn compute_next_version(filenames: &[String]) -> u32 {
    filenames
        .iter()
        .filter_map(|name| {
            name.split_once('_')
                .and_then(|(prefix, _)| prefix.parse::<u32>().ok())
        })
        .max()
        .map(|m| m + 1)
        .unwrap_or(1)
}

fn list_migration_files(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".sql") {
                    files.push(name.to_string());
                }
            }
        }
    }
    files.sort();
    files
}

fn migration_exists(dir: &Path, migration_name: &str) -> bool {
    list_migration_files(dir)
        .iter()
        .any(|f| f.contains(migration_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_version_empty_dir() {
        let files: Vec<String> = vec![];
        assert_eq!(compute_next_version(&files), 1);
    }

    #[test]
    fn next_version_picks_max_plus_one_with_gap() {
        // Mirrors the bug-report repro: 0001-0006, gap at 0007, 0008-0010 with
        // 0010 hand-written (non-_create_*). New file must be 0011, not 0010.
        let files = vec![
            "0001_create_organizations.sql".to_string(),
            "0002_create_users.sql".to_string(),
            "0006_create_accounts.sql".to_string(),
            "0008_create_journal_entries.sql".to_string(),
            "0009_create_journal_lines.sql".to_string(),
            "0010_m02_ledger_invariants.sql".to_string(),
        ];
        assert_eq!(compute_next_version(&files), 11);
    }

    #[test]
    fn next_version_ignores_non_numeric_prefix() {
        let files = vec!["readme.sql".to_string(), "0003_create_x.sql".to_string()];
        assert_eq!(compute_next_version(&files), 4);
    }
}
