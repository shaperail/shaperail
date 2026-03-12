use crate::FieldType;
use serde::{Deserialize, Serialize};

/// Definition of a single field in a resource schema.
///
/// Matches the inline YAML format:
/// ```yaml
/// email: { type: string, format: email, unique: true, required: true }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSchema {
    /// The data type of this field.
    #[serde(rename = "type")]
    pub field_type: FieldType,

    /// Whether this field is the primary key.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub primary: bool,

    /// Whether this field is auto-generated (e.g., uuid v4, timestamps).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generated: bool,

    /// Whether this field is required (NOT NULL + validated on input).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,

    /// Whether this field has a unique constraint.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unique: bool,

    /// Whether this field is explicitly nullable.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nullable: bool,

    /// Foreign key reference in `resource.field` format.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "ref")]
    pub reference: Option<String>,

    /// Minimum value (number) or length (string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<serde_json::Value>,

    /// Maximum value (number) or length (string).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<serde_json::Value>,

    /// String format validation (email, url, uuid).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// Allowed values for enum-type fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,

    /// Default value for this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    /// Whether this field contains sensitive data (redacted in logs).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sensitive: bool,

    /// Whether this field is included in full-text search.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub search: bool,

    /// Element type for array fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_schema_minimal() {
        let json = r#"{"type": "string"}"#;
        let fs: FieldSchema = serde_json::from_str(json).unwrap();
        assert_eq!(fs.field_type, FieldType::String);
        assert!(!fs.primary);
        assert!(!fs.generated);
        assert!(!fs.required);
        assert!(!fs.unique);
        assert!(!fs.nullable);
        assert!(fs.reference.is_none());
        assert!(fs.min.is_none());
        assert!(fs.max.is_none());
        assert!(fs.format.is_none());
        assert!(fs.values.is_none());
        assert!(fs.default.is_none());
        assert!(!fs.sensitive);
        assert!(!fs.search);
        assert!(fs.items.is_none());
    }

    #[test]
    fn field_schema_full() {
        let json = r#"{
            "type": "enum",
            "primary": false,
            "generated": false,
            "required": true,
            "unique": false,
            "nullable": false,
            "values": ["admin", "member", "viewer"],
            "default": "member"
        }"#;
        let fs: FieldSchema = serde_json::from_str(json).unwrap();
        assert_eq!(fs.field_type, FieldType::Enum);
        assert!(fs.required);
        assert_eq!(fs.values.as_ref().unwrap().len(), 3);
        assert_eq!(fs.default.as_ref().unwrap(), "member");
    }

    #[test]
    fn field_schema_with_ref() {
        let json = r#"{"type": "uuid", "ref": "organizations.id", "required": true}"#;
        let fs: FieldSchema = serde_json::from_str(json).unwrap();
        assert_eq!(fs.reference.as_deref(), Some("organizations.id"));
    }

    #[test]
    fn field_schema_serde_roundtrip() {
        let fs = FieldSchema {
            field_type: FieldType::String,
            primary: false,
            generated: false,
            required: true,
            unique: true,
            nullable: false,
            reference: None,
            min: Some(serde_json::json!(1)),
            max: Some(serde_json::json!(200)),
            format: Some("email".to_string()),
            values: None,
            default: None,
            sensitive: false,
            search: true,
            items: None,
        };
        let json = serde_json::to_string(&fs).unwrap();
        let back: FieldSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(fs, back);
    }
}
