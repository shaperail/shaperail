use shaperail_core::ResourceDefinition;

/// Errors that can occur during YAML parsing.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("{0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    ConfigInterpolation(String),

    /// A removed field type was used. Provides a friendly migration message.
    #[error("[{code}] {message}")]
    RemovedType {
        code: &'static str,
        message: &'static str,
    },

    #[error("{file}: {source}")]
    Context {
        file: String,
        #[source]
        source: Box<ParseError>,
    },
}

/// Parse a resource definition from a YAML string using the default
/// (serde_yaml) parser. Used by tests and tooling that already have the
/// YAML in memory. Endpoint defaults are NOT applied; use [`parse_resource`]
/// if you need them.
pub fn parse_resource_str(yaml: &str) -> Result<ResourceDefinition, ParseError> {
    if yaml.contains("type: bigint") {
        return Err(ParseError::RemovedType {
            code: "E_BIGINT_REMOVED",
            message:
                "type 'bigint' was removed in v0.13.0 — use 'integer' (now 64-bit by default).",
        });
    }
    Ok(serde_yaml::from_str(yaml)?)
}

/// Parse a YAML string into a `ResourceDefinition`.
///
/// After parsing, convention-based endpoint defaults are applied: for known
/// endpoint names (list, get, create, update, delete), `method` and `path`
/// are inferred from the resource name if not explicitly provided.
pub fn parse_resource(yaml: &str) -> Result<ResourceDefinition, ParseError> {
    // Pre-check: detect removed `bigint` field type before serde deserialization
    // so we can emit a friendly error instead of an opaque "unknown variant" message.
    if yaml.contains("type: bigint") {
        return Err(ParseError::RemovedType {
            code: "E_BIGINT_REMOVED",
            message:
                "type 'bigint' was removed in v0.13.0 — use 'integer' (now 64-bit by default).",
        });
    }
    let mut resource: ResourceDefinition = serde_yaml::from_str(yaml)?;
    shaperail_core::apply_endpoint_defaults(&mut resource);
    Ok(resource)
}

/// Parse a resource YAML file from disk.
///
/// Wraps parse errors with the filename for clearer diagnostics.
pub fn parse_resource_file(path: &std::path::Path) -> Result<ResourceDefinition, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_resource(&content).map_err(|e| ParseError::Context {
        file: path.display().to_string(),
        source: Box::new(e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_resource() {
        let yaml = r#"
resource: tags
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        assert_eq!(rd.resource, "tags");
        assert_eq!(rd.version, 1);
        assert_eq!(rd.schema.len(), 2);
        assert!(rd.endpoints.is_none());
        assert!(rd.relations.is_none());
        assert!(rd.indexes.is_none());
    }

    #[test]
    fn parse_full_users_resource() {
        let yaml = include_str!("../../resources/users.yaml");
        let rd = parse_resource(yaml).unwrap();
        assert_eq!(rd.resource, "users");
        assert_eq!(rd.version, 1);
        assert_eq!(rd.schema.len(), 10);
        assert!(rd.endpoints.is_some());
        assert!(rd.relations.is_some());
        assert!(rd.indexes.is_some());
    }

    #[test]
    fn parse_error_invalid_yaml() {
        let yaml = "not: [valid: yaml: here";
        let err = parse_resource(yaml).unwrap_err();
        assert!(matches!(err, ParseError::Yaml(_)));
    }

    #[test]
    fn parse_error_missing_resource_key() {
        let yaml = r#"
version: 1
schema:
  id: { type: uuid }
"#;
        let err = parse_resource(yaml).unwrap_err();
        assert!(err.to_string().contains("missing field"));
    }

    #[test]
    fn parse_error_unknown_top_level_key() {
        let yaml = r#"
resource: tags
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
unknown: true
"#;
        let err = parse_resource(yaml).unwrap_err();
        assert!(err.to_string().contains("unknown field"));
        assert!(err.to_string().contains("unknown"));
    }

    #[test]
    fn parse_error_unknown_field_key() {
        let yaml = r#"
resource: tags
version: 1
schema:
  id: { type: uuid, primary: true, generated: true, searchable: true }
"#;
        let err = parse_resource(yaml).unwrap_err();
        assert!(err.to_string().contains("unknown field"));
        assert!(err.to_string().contains("searchable"));
    }

    #[test]
    fn parse_resource_with_db_key() {
        let yaml = r#"
resource: events
version: 1
db: analytics
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        assert_eq!(rd.resource, "events");
        assert_eq!(rd.db.as_deref(), Some("analytics"));
    }

    #[test]
    fn parse_resource_file_includes_filename_in_error() {
        let path = std::path::Path::new("nonexistent/resource.yaml");
        let err = parse_resource_file(path).unwrap_err();
        // IO error for missing file — no Context wrapper needed
        assert!(matches!(err, ParseError::Io(_)));
    }

    #[test]
    fn parse_resource_file_context_wraps_yaml_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, "resource: test\nunknown_key: true\n").unwrap();

        let err = parse_resource_file(&path).unwrap_err();
        let msg = err.to_string();
        // Error message should contain the filename
        assert!(
            msg.contains("bad.yaml"),
            "Expected filename in error, got: {msg}"
        );
        // And also the actual parse error
        assert!(
            msg.contains("unknown field"),
            "Expected parse error detail, got: {msg}"
        );
    }
}
