use std::collections::HashSet;

use steel_core::ResourceDefinition;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Initializes structured JSON logging via the `tracing` crate.
///
/// - Outputs structured JSON to stdout (one line per event).
/// - Respects `RUST_LOG` env var for filtering (defaults to `info`).
/// - Attaches `request_id` to every log line via span fields.
pub fn init_logging() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_target(true)
                .with_thread_ids(false)
                .with_span_events(FmtSpan::CLOSE)
                .flatten_event(true),
        )
        .init();
}

/// Collects the set of field names marked `sensitive: true` across all resources.
pub fn sensitive_fields(resources: &[ResourceDefinition]) -> HashSet<String> {
    let mut fields = HashSet::new();
    for resource in resources {
        for (name, schema) in &resource.schema {
            if schema.sensitive {
                fields.insert(name.clone());
            }
        }
    }
    fields
}

/// Redacts sensitive fields from a JSON value in-place.
///
/// Any key matching a sensitive field name has its value replaced with `"[REDACTED]"`.
pub fn redact_sensitive(
    value: &serde_json::Value,
    sensitive: &HashSet<String>,
) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut redacted = serde_json::Map::new();
            for (key, val) in map {
                if sensitive.contains(key) {
                    redacted.insert(
                        key.clone(),
                        serde_json::Value::String("[REDACTED]".to_string()),
                    );
                } else {
                    redacted.insert(key.clone(), redact_sensitive(val, sensitive));
                }
            }
            serde_json::Value::Object(redacted)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| redact_sensitive(v, sensitive)).collect())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_fields_collected() {
        use indexmap::IndexMap;
        use steel_core::{FieldSchema, FieldType};

        let mut schema = IndexMap::new();
        schema.insert(
            "email".to_string(),
            FieldSchema {
                field_type: FieldType::String,
                primary: false,
                generated: false,
                required: true,
                unique: false,
                nullable: false,
                reference: None,
                min: None,
                max: None,
                format: None,
                values: None,
                default: None,
                sensitive: true,
                search: false,
                items: None,
            },
        );
        schema.insert(
            "name".to_string(),
            FieldSchema {
                field_type: FieldType::String,
                primary: false,
                generated: false,
                required: true,
                unique: false,
                nullable: false,
                reference: None,
                min: None,
                max: None,
                format: None,
                values: None,
                default: None,
                sensitive: false,
                search: false,
                items: None,
            },
        );

        let resources = vec![ResourceDefinition {
            resource: "users".to_string(),
            version: 1,
            schema,
            endpoints: None,
            relations: None,
            indexes: None,
        }];

        let fields = sensitive_fields(&resources);
        assert!(fields.contains("email"));
        assert!(!fields.contains("name"));
    }

    #[test]
    fn redact_sensitive_values() {
        let mut sensitive = HashSet::new();
        sensitive.insert("password".to_string());
        sensitive.insert("ssn".to_string());

        let value = serde_json::json!({
            "name": "Alice",
            "password": "secret123",
            "ssn": "123-45-6789",
            "nested": {
                "password": "also_secret"
            }
        });

        let redacted = redact_sensitive(&value, &sensitive);
        assert_eq!(redacted["name"], "Alice");
        assert_eq!(redacted["password"], "[REDACTED]");
        assert_eq!(redacted["ssn"], "[REDACTED]");
        assert_eq!(redacted["nested"]["password"], "[REDACTED]");
    }

    #[test]
    fn redact_handles_arrays() {
        let mut sensitive = HashSet::new();
        sensitive.insert("secret".to_string());

        let value = serde_json::json!([
            {"secret": "a", "public": "b"},
            {"secret": "c", "public": "d"},
        ]);

        let redacted = redact_sensitive(&value, &sensitive);
        let arr = redacted.as_array().unwrap();
        assert_eq!(arr[0]["secret"], "[REDACTED]");
        assert_eq!(arr[0]["public"], "b");
        assert_eq!(arr[1]["secret"], "[REDACTED]");
    }
}
