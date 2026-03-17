use std::fs;
use std::path::Path;

use shaperail_codegen::parser::parse_resource;

/// Scaffold a new resource YAML file and its initial migration.
pub fn run_create(name: &str, archetype: &str) -> i32 {
    if let Err(e) = validate_resource_name(name) {
        eprintln!("Error: {e}");
        return 1;
    }

    let resources_dir = Path::new("resources");
    if !resources_dir.is_dir() {
        eprintln!("Error: No resources/ directory found. Run this from a Shaperail project root.");
        return 1;
    }

    let resource_path = resources_dir.join(format!("{name}.yaml"));
    if resource_path.exists() {
        eprintln!("Error: resources/{name}.yaml already exists");
        return 1;
    }

    let yaml = match scaffold_resource_yaml(name, archetype) {
        Ok(yaml) => yaml,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    // Validate the generated YAML to ensure it's correct
    if let Err(e) = parse_resource(&yaml) {
        eprintln!("Internal error: generated YAML is invalid: {e}");
        return 1;
    }

    if let Err(e) = fs::write(&resource_path, &yaml) {
        eprintln!("Error writing {}: {e}", resource_path.display());
        return 1;
    }
    println!(
        "Created {} (archetype: {archetype})",
        resource_path.display()
    );

    // Generate migration SQL
    let migrations_dir = Path::new("migrations");
    if migrations_dir.is_dir() {
        match generate_initial_migration(name, &yaml, migrations_dir) {
            Ok(path) => println!("Created {path}"),
            Err(e) => eprintln!("Warning: could not generate migration: {e}"),
        }
    }

    println!();
    println!("Next steps:");
    println!("  1. Edit resources/{name}.yaml to customize fields");
    println!("  2. Run: shaperail validate");
    println!("  3. Run: shaperail migrate");
    println!("  4. Run: shaperail serve");
    0
}

fn validate_resource_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Resource name cannot be empty".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(
            "Resource name must be lowercase alphanumeric with underscores (e.g., 'blog_posts')"
                .into(),
        );
    }
    if name.starts_with('_') || name.starts_with(|c: char| c.is_ascii_digit()) {
        return Err("Resource name must start with a letter".into());
    }
    Ok(())
}

/// Available archetypes for resource scaffolding.
const ARCHETYPES: &[&str] = &["basic", "user", "content", "tenant", "lookup"];

fn scaffold_resource_yaml(name: &str, archetype: &str) -> Result<String, String> {
    match archetype {
        "basic" => Ok(archetype_basic(name)),
        "user" => Ok(archetype_user(name)),
        "content" => Ok(archetype_content(name)),
        "tenant" => Ok(archetype_tenant(name)),
        "lookup" => Ok(archetype_lookup(name)),
        _ => Err(format!(
            "Unknown archetype '{archetype}'. Available: {}",
            ARCHETYPES.join(", ")
        )),
    }
}

/// Basic archetype: id + timestamps, CRUD endpoints.
/// Convention-based defaults: method/path omitted for standard actions.
fn archetype_basic(name: &str) -> String {
    format!(
        r#"resource: {name}
version: 1

schema:
  id:         {{ type: uuid, primary: true, generated: true }}
  # Add your fields here:
  # title:    {{ type: string, required: true, min: 1, max: 200 }}
  # status:   {{ type: enum, values: [draft, published], default: draft }}
  created_at: {{ type: timestamp, generated: true }}
  updated_at: {{ type: timestamp, generated: true }}

endpoints:
  list:
    auth: public
    pagination: cursor

  get:
    auth: public

  create:
    auth: [admin]
    input: []

  update:
    auth: [admin]
    input: []

  delete:
    auth: [admin]
"#
    )
}

/// User archetype: email, name, role, org_id, timestamps.
fn archetype_user(name: &str) -> String {
    format!(
        r#"resource: {name}
version: 1

schema:
  id:         {{ type: uuid, primary: true, generated: true }}
  email:      {{ type: string, format: email, unique: true, required: true, sensitive: true }}
  name:       {{ type: string, min: 1, max: 200, required: true }}
  role:       {{ type: enum, values: [admin, member, viewer], default: member }}
  org_id:     {{ type: uuid, ref: organizations.id, required: true }}
  created_at: {{ type: timestamp, generated: true }}
  updated_at: {{ type: timestamp, generated: true }}

endpoints:
  list:
    auth: [member, admin]
    filters: [role, org_id]
    search: [name, email]
    pagination: cursor
    sort: [created_at, name]

  get:
    auth: [member, admin]

  create:
    auth: [admin]
    input: [email, name, role, org_id]

  update:
    auth: [admin, owner]
    input: [name, role]

  delete:
    auth: [admin]
    soft_delete: true

relations:
  organization: {{ resource: organizations, type: belongs_to, key: org_id }}

indexes:
  - {{ fields: [email], unique: true }}
  - {{ fields: [org_id, role] }}
  - {{ fields: [created_at], order: desc }}
"#
    )
}

/// Content archetype: title, slug, body, status, author_id, timestamps.
fn archetype_content(name: &str) -> String {
    format!(
        r#"resource: {name}
version: 1

schema:
  id:         {{ type: uuid, primary: true, generated: true }}
  title:      {{ type: string, min: 1, max: 500, required: true }}
  slug:       {{ type: string, unique: true, required: true }}
  body:       {{ type: string, required: true }}
  status:     {{ type: enum, values: [draft, published, archived], default: draft }}
  author_id:  {{ type: uuid, required: true }}
  created_at: {{ type: timestamp, generated: true }}
  updated_at: {{ type: timestamp, generated: true }}

endpoints:
  list:
    auth: public
    filters: [status, author_id]
    search: [title, body]
    pagination: cursor
    sort: [created_at, title]
    cache: {{ ttl: 60, invalidate_on: [create, update, delete] }}

  get:
    auth: public
    cache: {{ ttl: 300 }}

  create:
    auth: [admin, member]
    input: [title, slug, body, status, author_id]

  update:
    auth: [admin, owner]
    input: [title, slug, body, status]

  delete:
    auth: [admin]
    soft_delete: true

indexes:
  - {{ fields: [slug], unique: true }}
  - {{ fields: [author_id, status] }}
  - {{ fields: [created_at], order: desc }}
"#
    )
}

/// Tenant archetype: tenant-scoped resource with org_id as tenant key.
fn archetype_tenant(name: &str) -> String {
    format!(
        r#"resource: {name}
version: 1
tenant_key: org_id

schema:
  id:         {{ type: uuid, primary: true, generated: true }}
  org_id:     {{ type: uuid, ref: organizations.id, required: true }}
  name:       {{ type: string, min: 1, max: 200, required: true }}
  created_at: {{ type: timestamp, generated: true }}
  updated_at: {{ type: timestamp, generated: true }}

endpoints:
  list:
    auth: [member, admin]
    filters: [org_id]
    search: [name]
    pagination: cursor

  get:
    auth: [member, admin]

  create:
    auth: [admin]
    input: [org_id, name]

  update:
    auth: [admin]
    input: [name]

  delete:
    auth: [admin]
    soft_delete: true

relations:
  organization: {{ resource: organizations, type: belongs_to, key: org_id }}

indexes:
  - {{ fields: [org_id] }}
  - {{ fields: [created_at], order: desc }}
"#
    )
}

/// Lookup archetype: simple reference data (code + label).
fn archetype_lookup(name: &str) -> String {
    format!(
        r#"resource: {name}
version: 1

schema:
  id:    {{ type: uuid, primary: true, generated: true }}
  code:  {{ type: string, unique: true, required: true, min: 1, max: 50 }}
  label: {{ type: string, required: true, min: 1, max: 200 }}

endpoints:
  list:
    auth: public
    pagination: offset
    sort: [code, label]
    cache: {{ ttl: 3600 }}

  get:
    auth: public
    cache: {{ ttl: 3600 }}

  create:
    auth: [admin]
    input: [code, label]

  update:
    auth: [admin]
    input: [label]

  delete:
    auth: [admin]

indexes:
  - {{ fields: [code], unique: true }}
"#
    )
}

fn generate_initial_migration(
    name: &str,
    yaml: &str,
    migrations_dir: &Path,
) -> Result<String, String> {
    let resource = parse_resource(yaml).map_err(|e| format!("Failed to parse resource: {e}"))?;
    let sql = super::migrate::render_migration_sql(&resource);

    let existing = fs::read_dir(migrations_dir)
        .map_err(|e| format!("Failed to read migrations/: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.ends_with(".sql")))
        .count();

    let next_num = existing + 1;
    let filename = format!("{next_num:04}_create_{name}.sql");
    let path = migrations_dir.join(&filename);

    fs::write(&path, &sql).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_resource_name_accepts_valid() {
        assert!(validate_resource_name("users").is_ok());
        assert!(validate_resource_name("blog_posts").is_ok());
        assert!(validate_resource_name("orders2").is_ok());
    }

    #[test]
    fn validate_resource_name_rejects_invalid() {
        assert!(validate_resource_name("").is_err());
        assert!(validate_resource_name("Users").is_err());
        assert!(validate_resource_name("blog-posts").is_err());
        assert!(validate_resource_name("_private").is_err());
        assert!(validate_resource_name("2things").is_err());
    }

    #[test]
    fn scaffolded_yaml_parses_successfully() {
        let yaml = scaffold_resource_yaml("comments", "basic").unwrap();
        let rd = parse_resource(&yaml).expect("scaffolded YAML must parse");
        assert_eq!(rd.resource, "comments");
        assert_eq!(rd.version, 1);
        assert!(rd.schema.contains_key("id"));
        assert!(rd.schema.contains_key("created_at"));
        assert!(rd.schema.contains_key("updated_at"));
        assert!(rd.endpoints.is_some());
    }

    #[test]
    fn all_archetypes_parse_successfully() {
        for archetype in ARCHETYPES {
            let yaml = scaffold_resource_yaml("test_items", archetype).unwrap();
            let rd = parse_resource(&yaml)
                .unwrap_or_else(|e| panic!("archetype '{archetype}' failed to parse: {e}"));
            assert_eq!(rd.resource, "test_items");
            assert!(rd.schema.contains_key("id"));
        }
    }

    #[test]
    fn unknown_archetype_returns_error() {
        let result = scaffold_resource_yaml("items", "unknown");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown archetype"));
    }

    #[test]
    fn convention_defaults_applied_to_basic_archetype() {
        let yaml = scaffold_resource_yaml("items", "basic").unwrap();
        let rd = parse_resource(&yaml).unwrap();
        let endpoints = rd.endpoints.as_ref().unwrap();
        let list = &endpoints["list"];
        // method and path should be filled in by convention defaults
        assert_eq!(*list.method(), shaperail_core::HttpMethod::Get);
        assert_eq!(list.path(), "/items");
    }
}
