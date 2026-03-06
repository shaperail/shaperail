use steel_core::ResourceDefinition;

/// Errors that can occur during YAML parsing.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("{0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Parse a YAML string into a `ResourceDefinition`.
pub fn parse_resource(yaml: &str) -> Result<ResourceDefinition, ParseError> {
    let resource: ResourceDefinition = serde_yaml::from_str(yaml)?;
    Ok(resource)
}

/// Parse a resource YAML file from disk.
pub fn parse_resource_file(path: &std::path::Path) -> Result<ResourceDefinition, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_resource(&content)
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
        assert_eq!(rd.schema.len(), 9);
        assert!(rd.endpoints.is_some());
        assert!(rd.relations.is_some());
        assert!(rd.indexes.is_some());
    }

    #[test]
    fn parse_error_invalid_yaml() {
        let yaml = "not: [valid: yaml: here";
        let err = parse_resource(yaml).unwrap_err();
        assert!(err.to_string().contains("did not find"));
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
}
