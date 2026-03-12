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

    let database_url = match resolve_database_url() {
        Ok(url) => url,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create async runtime: {e}");
            return 1;
        }
    };

    rt.block_on(async {
        let pool = match shaperail_runtime::db::create_pool(&database_url, 5).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to connect to database: {e}");
                return 1;
            }
        };

        match seed_database(&pool, &files).await {
            Ok(summary) => {
                println!("Seed complete:");
                for (table, count) in &summary {
                    println!("  {table}: {count} record(s) inserted");
                }
                0
            }
            Err(e) => {
                eprintln!("Seed failed (transaction rolled back): {e}");
                1
            }
        }
    })
}

/// Resolve DATABASE_URL from environment or .env file.
fn resolve_database_url() -> Result<String, String> {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return Ok(url);
    }

    // Fall back to reading .env file in the current directory.
    let env_path = Path::new(".env");
    if env_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(env_path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some(value) = line.strip_prefix("DATABASE_URL=") {
                    let value = value.trim();
                    if !value.is_empty() {
                        return Ok(value.to_string());
                    }
                }
            }
        }
    }

    Err(
        "DATABASE_URL is not set. Set it in your environment or in a .env file in the project root."
            .to_string(),
    )
}

/// Seed the database with all fixture files inside a single transaction.
async fn seed_database(
    pool: &sqlx::PgPool,
    files: &[std::path::PathBuf],
) -> Result<Vec<(String, usize)>, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    let mut summary = Vec::new();

    for file in files {
        let table = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let content = std::fs::read_to_string(file)
            .map_err(|e| format!("Error reading {}: {e}", file.display()))?;

        let value: serde_yaml::Value = serde_yaml::from_str(&content)
            .map_err(|e| format!("Error parsing {}: {e}", file.display()))?;

        let records = match &value {
            serde_yaml::Value::Sequence(seq) => seq.clone(),
            _ => {
                return Err(format!(
                    "{}: expected a YAML sequence (list of records)",
                    file.display()
                ));
            }
        };

        let mut inserted = 0;
        for (i, record) in records.iter().enumerate() {
            let mapping = match record.as_mapping() {
                Some(m) => m,
                None => {
                    return Err(format!(
                        "{}: record {} is not a mapping",
                        file.display(),
                        i + 1
                    ));
                }
            };

            let columns: Vec<String> = mapping
                .keys()
                .map(yaml_value_to_column_name)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("{}: record {}: {e}", file.display(), i + 1))?;

            if columns.is_empty() {
                return Err(format!(
                    "{}: record {} has no columns",
                    file.display(),
                    i + 1
                ));
            }

            let placeholders: Vec<String> = (1..=columns.len()).map(|n| format!("${n}")).collect();

            let quoted_columns: Vec<String> = columns.iter().map(|c| format!("\"{c}\"")).collect();

            let sql = format!(
                "INSERT INTO \"{}\" ({}) VALUES ({})",
                table,
                quoted_columns.join(", "),
                placeholders.join(", ")
            );

            let mut query = sqlx::query(&sql);

            for (key, val) in mapping.iter() {
                let col_name = yaml_value_to_column_name(key)
                    .map_err(|e| format!("{}: record {}: {e}", file.display(), i + 1))?;
                query = bind_yaml_value(query, val).map_err(|e| {
                    format!(
                        "{}: record {}, column '{}': {e}",
                        file.display(),
                        i + 1,
                        col_name
                    )
                })?;
            }

            query
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("{}: record {}: {e}", file.display(), i + 1))?;

            inserted += 1;
        }

        summary.push((table, inserted));
    }

    tx.commit()
        .await
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(summary)
}

/// Extract a column name from a YAML mapping key.
fn yaml_value_to_column_name(key: &serde_yaml::Value) -> Result<String, String> {
    match key {
        serde_yaml::Value::String(s) => Ok(s.clone()),
        other => Err(format!("column key must be a string, got: {other:?}")),
    }
}

/// Bind a serde_yaml::Value to a sqlx query as the appropriate Postgres type.
fn bind_yaml_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    value: &'q serde_yaml::Value,
) -> Result<sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>, String> {
    match value {
        serde_yaml::Value::String(s) => Ok(query.bind(s.as_str())),
        serde_yaml::Value::Bool(b) => Ok(query.bind(*b)),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(query.bind(i))
            } else if let Some(f) = n.as_f64() {
                Ok(query.bind(f))
            } else {
                Err(format!("unsupported number value: {n}"))
            }
        }
        serde_yaml::Value::Null => Ok(query.bind(None::<String>)),
        other => Err(format!("unsupported YAML value type: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_database_url_from_env() {
        // When DATABASE_URL is set in the environment, it should be returned.
        std::env::set_var("DATABASE_URL", "postgresql://test:test@localhost/test_db");
        let url = resolve_database_url().unwrap();
        assert_eq!(url, "postgresql://test:test@localhost/test_db");
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    fn yaml_column_name_string() {
        let key = serde_yaml::Value::String("email".into());
        assert_eq!(yaml_value_to_column_name(&key).unwrap(), "email");
    }

    #[test]
    fn yaml_column_name_non_string_fails() {
        let key = serde_yaml::Value::Number(serde_yaml::Number::from(42));
        assert!(yaml_value_to_column_name(&key).is_err());
    }

    #[test]
    fn missing_seeds_dir_returns_error() {
        let exit_code = run(std::path::Path::new("/nonexistent/seeds"));
        assert_eq!(exit_code, 1);
    }

    #[test]
    fn empty_seeds_dir_returns_error() {
        let temp = tempfile::tempdir().unwrap();
        let exit_code = run(temp.path());
        assert_eq!(exit_code, 1);
    }
}
