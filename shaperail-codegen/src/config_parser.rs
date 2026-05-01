use shaperail_core::ProjectConfig;

use crate::parser::ParseError;

/// Parse a YAML string into a `ProjectConfig`.
pub fn parse_config(yaml: &str) -> Result<ProjectConfig, ParseError> {
    let interpolated = interpolate_env_placeholders(yaml)?;
    let config: ProjectConfig = serde_yaml::from_str(&interpolated)?;
    Ok(config)
}

/// Parse a shaperail.config.yaml file from disk.
pub fn parse_config_file(path: &std::path::Path) -> Result<ProjectConfig, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_config(&content)
}

/// Interpolate `${VAR}` and `${VAR:default}` placeholders in YAML text.
/// Public so workspace_parser can reuse it.
pub fn interpolate_env(yaml: &str) -> Result<String, ParseError> {
    interpolate_env_placeholders(yaml)
}

fn interpolate_env_placeholders(yaml: &str) -> Result<String, ParseError> {
    let mut result = String::with_capacity(yaml.len());
    let mut index = 0usize;

    while let Some(offset) = yaml[index..].find("${") {
        let start = index + offset;
        result.push_str(&yaml[index..start]);

        let placeholder_start = start + 2;
        let end = yaml[placeholder_start..]
            .find('}')
            .map(|pos| placeholder_start + pos)
            .ok_or_else(|| {
                ParseError::ConfigInterpolation(
                    "unterminated environment placeholder in shaperail.config.yaml".to_string(),
                )
            })?;

        let placeholder = &yaml[placeholder_start..end];
        if placeholder.is_empty() {
            return Err(ParseError::ConfigInterpolation(
                "empty environment placeholder in shaperail.config.yaml".to_string(),
            ));
        }

        let (name, default) = match placeholder.split_once(':') {
            Some((name, default)) => (name, Some(default)),
            None => (placeholder, None),
        };

        if name.is_empty() {
            return Err(ParseError::ConfigInterpolation(
                "environment placeholder is missing a variable name".to_string(),
            ));
        }

        let value = match std::env::var(name) {
            Ok(value) => value,
            Err(_) => match default {
                Some(default) => default.to_string(),
                None => {
                    return Err(ParseError::ConfigInterpolation(format!(
                        "environment variable '{name}' is not set"
                    )))
                }
            },
        };

        result.push_str(&value);
        index = end + 1;
    }

    result.push_str(&yaml[index..]);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shaperail_core::WorkerCount;

    #[test]
    fn parse_minimal_config() {
        let yaml = r#"
project: my-app
"#;
        let cfg = parse_config(yaml).unwrap();
        assert_eq!(cfg.project, "my-app");
        assert_eq!(cfg.port, 3000);
        assert_eq!(cfg.workers, WorkerCount::Auto);
    }

    #[test]
    fn parse_full_config() {
        let yaml = r#"
project: my-api
port: 8080
workers: 4

databases:
  default:
    engine: postgres
    url: postgresql://localhost/my_api_db
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
        let dbs = cfg.databases.as_ref().unwrap();
        assert_eq!(
            dbs.get("default").unwrap().url,
            "postgresql://localhost/my_api_db"
        );
        let auth = cfg.auth.unwrap();
        assert_eq!(auth.provider, "jwt");
    }

    #[test]
    fn parse_config_error_missing_project() {
        let yaml = "port: 3000";
        let err = parse_config(yaml).unwrap_err();
        assert!(err.to_string().contains("missing field"));
    }

    #[test]
    fn parse_config_interpolates_env_vars() {
        let yaml = r#"
project: ${SHAPERAIL_TEST_PROJECT}
databases:
  default:
    engine: postgres
    url: postgresql://localhost/${SHAPERAIL_TEST_DB:test_db}
"#;
        std::env::set_var("SHAPERAIL_TEST_PROJECT", "shaperail-ai");
        std::env::remove_var("SHAPERAIL_TEST_DB");

        let cfg = parse_config(yaml).unwrap();
        assert_eq!(cfg.project, "shaperail-ai");
        let dbs = cfg.databases.as_ref().unwrap();
        assert_eq!(
            dbs.get("default").unwrap().url,
            "postgresql://localhost/test_db"
        );

        std::env::remove_var("SHAPERAIL_TEST_PROJECT");
    }

    #[test]
    fn parse_config_databases_multi_db() {
        let yaml = r#"
project: multi-db-app
databases:
  default:
    engine: postgres
    url: postgresql://localhost/main
    pool_size: 10
  analytics:
    engine: postgres
    url: postgresql://localhost/analytics
"#;
        let cfg = parse_config(yaml).unwrap();
        let dbs = cfg.databases.as_ref().unwrap();
        assert_eq!(dbs.len(), 2);
        assert!(dbs.contains_key("default"));
        assert!(dbs.contains_key("analytics"));
        assert_eq!(
            dbs.get("default").unwrap().url,
            "postgresql://localhost/main"
        );
    }

    #[test]
    fn parse_config_unknown_key_fails() {
        let yaml = r#"
project: my-app
port: 3000
unknown: true
"#;
        let err = parse_config(yaml).unwrap_err();
        assert!(err.to_string().contains("unknown field"));
        assert!(err.to_string().contains("unknown"));
    }

    #[test]
    fn parse_config_legacy_database_field_rejected() {
        // The singular `database:` block was removed in v0.11. Existing configs
        // must fail loudly so users migrate to `databases.default:` or DATABASE_URL.
        let yaml = r#"
project: legacy-app
database:
  type: postgresql
  name: legacy_db
"#;
        let err = parse_config(yaml)
            .expect_err("legacy `database:` block should be rejected after v0.11");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field") && msg.contains("database"),
            "expected error to name the rejected `database` field, got: {msg}"
        );
    }

    #[test]
    fn parse_config_missing_env_without_default_fails() {
        std::env::remove_var("SHAPERAIL_TEST_MISSING");
        let yaml = "project: ${SHAPERAIL_TEST_MISSING}";
        let err = parse_config(yaml).unwrap_err();
        assert!(err.to_string().contains("SHAPERAIL_TEST_MISSING"));
    }

    #[test]
    fn parse_config_protocols() {
        let yaml = r#"
project: gql-api
protocols:
  - rest
  - graphql
"#;
        let cfg = parse_config(yaml).unwrap();
        assert_eq!(cfg.protocols, vec!["rest", "graphql"]);
    }
}
