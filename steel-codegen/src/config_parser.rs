use steel_core::ProjectConfig;

use crate::parser::ParseError;

/// Parse a YAML string into a `ProjectConfig`.
pub fn parse_config(yaml: &str) -> Result<ProjectConfig, ParseError> {
    let config: ProjectConfig = serde_yaml::from_str(yaml)?;
    Ok(config)
}

/// Parse a steel.config.yaml file from disk.
pub fn parse_config_file(path: &std::path::Path) -> Result<ProjectConfig, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_config(&content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_core::WorkerCount;

    #[test]
    fn parse_minimal_config() {
        let yaml = r#"
project: my-app
"#;
        let cfg = parse_config(yaml).unwrap();
        assert_eq!(cfg.project, "my-app");
        assert_eq!(cfg.port, 3000);
        assert_eq!(cfg.workers, WorkerCount::Auto);
        assert!(cfg.database.is_none());
    }

    #[test]
    fn parse_full_config() {
        let yaml = r#"
project: my-api
port: 8080
workers: 4

database:
  type: postgresql
  host: localhost
  port: 5432
  name: my_api_db
  pool_size: 20

cache:
  type: redis
  url: redis://localhost:6379

auth:
  provider: jwt
  secret_env: JWT_SECRET
  expiry: 24h
  refresh_expiry: 30d

storage:
  provider: s3
  bucket: my-bucket
  region: us-east-1

logging:
  level: info
  format: json
  otlp_endpoint: http://localhost:4317
"#;
        let cfg = parse_config(yaml).unwrap();
        assert_eq!(cfg.project, "my-api");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.workers, WorkerCount::Fixed(4));
        let db = cfg.database.unwrap();
        assert_eq!(db.db_type, "postgresql");
        assert_eq!(db.name, "my_api_db");
        let auth = cfg.auth.unwrap();
        assert_eq!(auth.provider, "jwt");
    }

    #[test]
    fn parse_config_error_missing_project() {
        let yaml = "port: 3000";
        let err = parse_config(yaml).unwrap_err();
        assert!(err.to_string().contains("missing field"));
    }
}
