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

    // Generate migration SQL from resource definitions
    for resource in &resources {
        let migration_name = format!("create_{}", resource.resource);
        let sql = render_migration_sql(resource);

        // Find next migration number
        let existing = list_migration_files(migrations_dir);
        let next_num = existing.len() + 1;
        let filename = format!("{next_num:04}_{migration_name}.sql");
        let path = migrations_dir.join(&filename);

        if migration_exists(migrations_dir, &migration_name) {
            println!(
                "Migration for '{}' already exists, skipping",
                resource.resource
            );
            continue;
        }

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
