use std::fs;
use std::path::Path;

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
        let sql = generate_create_table_sql(resource);

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

fn generate_create_table_sql(resource: &shaperail_core::ResourceDefinition) -> String {
    let table = &resource.resource;
    let mut columns = Vec::new();
    let mut constraints = Vec::new();

    for (name, schema) in &resource.schema {
        let sql_type = match &schema.field_type {
            shaperail_core::FieldType::Uuid => "UUID",
            shaperail_core::FieldType::String => "TEXT",
            shaperail_core::FieldType::Integer => "INTEGER",
            shaperail_core::FieldType::Bigint => "BIGINT",
            shaperail_core::FieldType::Number => "DOUBLE PRECISION",
            shaperail_core::FieldType::Boolean => "BOOLEAN",
            shaperail_core::FieldType::Timestamp => "TIMESTAMPTZ",
            shaperail_core::FieldType::Date => "DATE",
            shaperail_core::FieldType::Enum => "TEXT",
            shaperail_core::FieldType::Json => "JSONB",
            shaperail_core::FieldType::Array => "JSONB",
            shaperail_core::FieldType::File => "TEXT",
        };

        let mut col = format!("    {name} {sql_type}");

        if schema.primary {
            col.push_str(" PRIMARY KEY");
        }
        if schema.generated && schema.field_type == shaperail_core::FieldType::Uuid {
            col.push_str(" DEFAULT gen_random_uuid()");
        }
        if schema.generated
            && (schema.field_type == shaperail_core::FieldType::Timestamp
                || schema.field_type == shaperail_core::FieldType::Date)
        {
            col.push_str(" DEFAULT now()");
        }
        if !schema.nullable && !schema.primary && schema.required {
            col.push_str(" NOT NULL");
        }
        if schema.unique {
            col.push_str(" UNIQUE");
        }
        if let Some(default) = &schema.default {
            let default_val = match default {
                serde_json::Value::String(s) => format!("'{s}'"),
                serde_json::Value::Bool(b) => b.to_string(),
                other => other.to_string(),
            };
            col.push_str(&format!(" DEFAULT {default_val}"));
        }

        columns.push(col);
    }

    // Add deleted_at column if any endpoint uses soft_delete
    let has_soft_delete = resource
        .endpoints
        .as_ref()
        .is_some_and(|eps| eps.values().any(|ep| ep.soft_delete));
    if has_soft_delete {
        columns.push("    deleted_at TIMESTAMPTZ".to_string());
    }

    // Indexes
    if let Some(indexes) = &resource.indexes {
        for idx in indexes {
            let fields_str = idx.fields.join(", ");
            let unique = if idx.unique { "UNIQUE " } else { "" };
            let idx_name = format!("idx_{table}_{}", idx.fields.join("_"));
            constraints.push(format!(
                "CREATE {unique}INDEX {idx_name} ON {table} ({fields_str});"
            ));
        }
    }

    let columns_sql = columns.join(",\n");
    let mut sql = format!("CREATE TABLE IF NOT EXISTS {table} (\n{columns_sql}\n);\n");

    for constraint in &constraints {
        sql.push('\n');
        sql.push_str(constraint);
        sql.push('\n');
    }

    sql
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
