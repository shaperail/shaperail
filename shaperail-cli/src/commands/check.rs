use std::collections::HashMap;
use std::path::{Path, PathBuf};

use shaperail_core::{FieldType, ResourceDefinition};

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
    let mut parsed: Vec<ResourceDefinition> = Vec::new();

    for file in &files {
        let diags = match shaperail_codegen::parser::diagnose_file(file) {
            Ok(d) => d,
            Err(e) => {
                // Parse error → emit a single SR000 diagnostic, same shape as today.
                vec![shaperail_codegen::diagnostics::Diagnostic::error(
                    "SR000",
                    format!("YAML parse error in {}: {}", file.display(), e),
                    "fix the YAML syntax error in the file",
                    "<see error message above>",
                )]
            }
        };

        // For the cross-resource drift check we still need parsed ResourceDefinitions.
        // Re-parse (cheaply, serde_yaml) when the diagnose path succeeded.
        if diags.iter().all(|d| d.code != "SR000") {
            if let Ok(rd) = shaperail_codegen::parser::parse_resource_file(file) {
                parsed.push(rd);
            }
        }

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
                        "severity": d.severity,
                        "doc_url": d.doc_url,
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

    let migrations_dir = Path::new("migrations");
    let drift = check_integer_column_drift(&parsed, migrations_dir);
    for w in &drift {
        if json_output {
            all_diagnostics.push(serde_json::json!({
                "file": w.migration.display().to_string(),
                "code": "SR100",
                "level": "warning",
                "error": w.error_text(),
                "fix": w.fix_text(),
                "example": w.example_text(),
            }));
        } else {
            eprintln!(
                "W [SR100] {}.{}: existing migration {} declares INTEGER, but type: integer now emits BIGINT in v0.13.0+.",
                w.resource,
                w.column,
                w.migration.display()
            );
            eprintln!("    Fix: {}", w.fix_text());
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

/// One drift warning per (resource, column) where the latest migration declares
/// `INTEGER` but the resource schema now declares `type: integer` (which emits
/// `BIGINT` as of v0.13.0).
struct IntegerDrift {
    resource: String,
    column: String,
    migration: PathBuf,
}

impl IntegerDrift {
    fn error_text(&self) -> String {
        format!(
            "{}.{}: existing migration {} declares INTEGER, but type: integer now emits BIGINT in v0.13.0+",
            self.resource,
            self.column,
            self.migration.display()
        )
    }

    fn fix_text(&self) -> String {
        format!(
            "Add an `ALTER TABLE {} ALTER COLUMN {} TYPE BIGINT` migration before deploying v0.13.0 generated code.",
            self.resource, self.column
        )
    }

    fn example_text(&self) -> String {
        format!(
            "ALTER TABLE {} ALTER COLUMN {} TYPE BIGINT;",
            self.resource, self.column
        )
    }
}

/// Walk every committed migration in sorted order, track the latest declared
/// type per `(table, column)`, and warn for every resource field declared
/// `type: integer` whose latest migration still says `INTEGER`. Returns an
/// empty vec if no migrations directory exists or no fields drifted.
fn check_integer_column_drift(
    resources: &[ResourceDefinition],
    migrations_dir: &Path,
) -> Vec<IntegerDrift> {
    if !migrations_dir.is_dir() {
        return Vec::new();
    }

    let mut migration_files: Vec<PathBuf> = match std::fs::read_dir(migrations_dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "sql"))
            .collect(),
        Err(_) => return Vec::new(),
    };
    migration_files.sort();

    // (table, column) -> (declared sql type uppercase, file that declared it).
    let mut latest: HashMap<(String, String), (String, PathBuf)> = HashMap::new();

    for file in &migration_files {
        let Ok(content) = std::fs::read_to_string(file) else {
            continue;
        };
        apply_migration_to_state(&content, file, &mut latest);
    }

    let mut warnings = Vec::new();
    for r in resources {
        for (col_name, field) in &r.schema {
            if field.field_type != FieldType::Integer {
                continue;
            }
            let key = (r.resource.clone(), col_name.clone());
            if let Some((sql_type, file)) = latest.get(&key) {
                if sql_type == "INTEGER" {
                    warnings.push(IntegerDrift {
                        resource: r.resource.clone(),
                        column: col_name.clone(),
                        migration: file.clone(),
                    });
                }
            }
        }
    }
    warnings
}

/// Tiny SQL state machine: tracks the most recent declared type per
/// `(table, column)` based on `CREATE TABLE` blocks and `ALTER TABLE … TYPE`
/// statements emitted by shaperail's own migration writer. Not a full parser —
/// only handles patterns shaperail emits and the canonical hand-written ALTER.
fn apply_migration_to_state(
    content: &str,
    file: &Path,
    latest: &mut HashMap<(String, String), (String, PathBuf)>,
) {
    let mut current_table: Option<String> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }

        if let Some(table) = parse_create_table_open(line) {
            current_table = Some(table);
            continue;
        }
        if line.starts_with(')') || line.starts_with(");") {
            current_table = None;
            continue;
        }

        if let Some(table) = current_table.as_deref() {
            if let Some((col, sql_type)) = parse_column_declaration(line) {
                latest.insert((table.to_string(), col), (sql_type, file.to_path_buf()));
                continue;
            }
        }

        if let Some((table, col, sql_type)) = parse_alter_column_type(line) {
            latest.insert((table, col), (sql_type, file.to_path_buf()));
        }
    }
}

/// Match `CREATE TABLE "name"` / `CREATE TABLE name` openings, returning the
/// table identifier (without quotes) on success.
fn parse_create_table_open(line: &str) -> Option<String> {
    let upper = line.to_uppercase();
    let prefix = upper.strip_prefix("CREATE TABLE")?;
    let prefix = prefix.strip_prefix(" IF NOT EXISTS").unwrap_or(prefix);
    let rest = prefix.trim_start();
    if !rest.contains('(') && !line.trim_end().ends_with('(') {
        // Some emitters split the opening paren onto the next line; be tolerant.
    }
    // Slice the original (case-preserving) line at the same offset.
    let consumed = line.len() - rest.len();
    let rest_orig = line[consumed..].trim_start();
    let name_token = rest_orig
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches('(');
    let name = name_token.trim_matches('"').trim_matches('`');
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Match `"col" TYPE …` or `col TYPE …` inside a CREATE TABLE block.
/// Returns `(column_name, uppercase_type_keyword)`.
fn parse_column_declaration(line: &str) -> Option<(String, String)> {
    let line = line.trim_end_matches(',').trim();
    let (first, rest) = line.split_once(char::is_whitespace)?;
    let col = first.trim_matches('"').trim_matches('`');
    if col.is_empty() || !is_identifier(col) {
        return None;
    }
    let type_token = rest.split_whitespace().next()?.to_uppercase();
    let type_token = type_token.trim_end_matches(',').to_string();
    if type_token.is_empty() {
        return None;
    }
    Some((col.to_string(), type_token))
}

/// Match `ALTER TABLE <table> ALTER COLUMN <col> TYPE <TYPE>`.
fn parse_alter_column_type(line: &str) -> Option<(String, String, String)> {
    let upper = line.to_uppercase();
    if !upper.starts_with("ALTER TABLE") {
        return None;
    }
    let mut parts = line.split_whitespace();
    parts.next()?; // ALTER
    parts.next()?; // TABLE
    let table = parts
        .next()?
        .trim_matches('"')
        .trim_matches('`')
        .to_string();
    let alter = parts.next()?;
    let column = parts.next()?;
    if !alter.eq_ignore_ascii_case("ALTER") || !column.eq_ignore_ascii_case("COLUMN") {
        return None;
    }
    let col = parts
        .next()?
        .trim_matches('"')
        .trim_matches('`')
        .to_string();
    let type_kw = parts.next()?;
    if !type_kw.eq_ignore_ascii_case("TYPE") {
        return None;
    }
    let sql_type = parts.next()?.trim_end_matches(';').to_uppercase();
    Some((table, col, sql_type))
}

fn is_identifier(s: &str) -> bool {
    let first = s.chars().next();
    matches!(first, Some(c) if c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn integer_resource(name: &str, col: &str) -> ResourceDefinition {
        let yaml = format!(
            "resource: {name}\nversion: 1\nschema:\n  id: {{ type: uuid, primary: true, generated: true }}\n  {col}: {{ type: integer, required: true }}\n"
        );
        serde_yaml::from_str(&yaml).expect("test resource yaml parses")
    }

    #[test]
    fn drift_detected_when_migration_declares_integer() {
        let dir = tempdir().unwrap();
        let migrations = dir.path().join("migrations");
        fs::create_dir_all(&migrations).unwrap();
        fs::write(
            migrations.join("0001_create_policies.sql"),
            "CREATE TABLE \"policies\" (\n  \"id\" UUID PRIMARY KEY,\n  \"cap_minor\" INTEGER NOT NULL\n);\n",
        )
        .unwrap();

        let resources = vec![integer_resource("policies", "cap_minor")];
        let warnings = check_integer_column_drift(&resources, &migrations);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].resource, "policies");
        assert_eq!(warnings[0].column, "cap_minor");
    }

    #[test]
    fn drift_cleared_by_later_alter_to_bigint() {
        let dir = tempdir().unwrap();
        let migrations = dir.path().join("migrations");
        fs::create_dir_all(&migrations).unwrap();
        fs::write(
            migrations.join("0001_create_policies.sql"),
            "CREATE TABLE \"policies\" (\n  \"cap_minor\" INTEGER NOT NULL\n);\n",
        )
        .unwrap();
        fs::write(
            migrations.join("0002_alter_cap_minor_to_bigint.sql"),
            "ALTER TABLE policies ALTER COLUMN cap_minor TYPE BIGINT;\n",
        )
        .unwrap();

        let resources = vec![integer_resource("policies", "cap_minor")];
        let warnings = check_integer_column_drift(&resources, &migrations);

        assert!(warnings.is_empty());
    }

    #[test]
    fn no_drift_when_create_emits_bigint() {
        let dir = tempdir().unwrap();
        let migrations = dir.path().join("migrations");
        fs::create_dir_all(&migrations).unwrap();
        fs::write(
            migrations.join("0001_create_policies.sql"),
            "CREATE TABLE \"policies\" (\n  \"cap_minor\" BIGINT NOT NULL\n);\n",
        )
        .unwrap();

        let resources = vec![integer_resource("policies", "cap_minor")];
        let warnings = check_integer_column_drift(&resources, &migrations);

        assert!(warnings.is_empty());
    }

    #[test]
    fn no_warnings_without_migrations_directory() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        let resources = vec![integer_resource("policies", "cap_minor")];
        let warnings = check_integer_column_drift(&resources, &missing);
        assert!(warnings.is_empty());
    }
}
