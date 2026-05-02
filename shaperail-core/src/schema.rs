use crate::FieldType;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Element specification for `type: array` fields.
///
/// Accepts two YAML shapes — both are equivalent for fields that need no element
/// constraints:
///
/// ```yaml
/// items: string                             # bare-name shorthand
/// items: { type: string }                   # equivalent map form
/// items: { type: string, min: 3, max: 3 }   # element-level constraints
/// items: { type: enum, values: [a, b] }     # element allowlist
/// items: { type: uuid, ref: organizations.id }  # FK array (Postgres only)
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemsSpec {
    #[serde(rename = "type")]
    pub field_type: FieldType,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<serde_json::Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<serde_json::Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,

    #[serde(default, skip_serializing_if = "Option::is_none", rename = "ref")]
    pub reference: Option<String>,
}

impl ItemsSpec {
    /// Constructs a bare `ItemsSpec` with only `field_type` set.
    pub fn of(field_type: FieldType) -> Self {
        Self {
            field_type,
            min: None,
            max: None,
            format: None,
            values: None,
            reference: None,
        }
    }
}

impl<'de> Deserialize<'de> for ItemsSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ItemsSpecVisitor;

        impl<'de> Visitor<'de> for ItemsSpecVisitor {
            type Value = ItemsSpec;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a type name (e.g. \"string\") or a constraint map with `type:`")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                let field_type = FieldType::deserialize(de::value::StrDeserializer::new(v))?;
                Ok(ItemsSpec::of(field_type))
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Inner {
                    #[serde(rename = "type")]
                    field_type: FieldType,
                    #[serde(default)]
                    min: Option<serde_json::Value>,
                    #[serde(default)]
                    max: Option<serde_json::Value>,
                    #[serde(default)]
                    format: Option<String>,
                    #[serde(default)]
                    values: Option<Vec<String>>,
                    #[serde(default, rename = "ref")]
                    reference: Option<String>,
                }
                let inner = Inner::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(ItemsSpec {
                    field_type: inner.field_type,
                    min: inner.min,
                    max: inner.max,
                    format: inner.format,
                    values: inner.values,
                    reference: inner.reference,
                })
            }
        }

        deserializer.deserialize_any(ItemsSpecVisitor)
    }
}

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
    pub items: Option<ItemsSpec>,

    /// Input-only field: validated and exposed to the before-controller in `ctx.input`,
    /// but never persisted (no migration column, no SQL reference) and never returned
    /// in API responses. Stripped from `ctx.input` after the before-controller runs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub transient: bool,
}

impl FieldSchema {
    /// Returns true if this field is stored in the database. Transient fields exist only
    /// at the API boundary and are never persisted.
    pub fn is_persisted(&self) -> bool {
        !self.transient
    }
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
        assert!(!fs.transient);
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
            transient: false,
        };
        let json = serde_json::to_string(&fs).unwrap();
        let back: FieldSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(fs, back);
    }

    #[test]
    fn items_spec_bare_string_form() {
        let yaml = r#"type: array
items: string"#;
        let fs: FieldSchema = serde_yaml::from_str(yaml).unwrap();
        let items = fs.items.expect("items present");
        assert_eq!(items.field_type, FieldType::String);
        assert!(items.min.is_none());
        assert!(items.max.is_none());
        assert!(items.values.is_none());
        assert!(items.reference.is_none());
        assert!(items.format.is_none());
    }

    #[test]
    fn items_spec_full_map_form() {
        let yaml = r#"type: array
items: { type: string, min: 3, max: 3 }"#;
        let fs: FieldSchema = serde_yaml::from_str(yaml).unwrap();
        let items = fs.items.expect("items present");
        assert_eq!(items.field_type, FieldType::String);
        assert_eq!(items.min, Some(serde_json::json!(3)));
        assert_eq!(items.max, Some(serde_json::json!(3)));
    }

    #[test]
    fn items_spec_enum_form() {
        let yaml = r#"type: array
items: { type: enum, values: [a, b, c] }"#;
        let fs: FieldSchema = serde_yaml::from_str(yaml).unwrap();
        let items = fs.items.expect("items present");
        assert_eq!(items.field_type, FieldType::Enum);
        assert_eq!(
            items.values.as_deref(),
            Some(["a".to_string(), "b".to_string(), "c".to_string()].as_slice())
        );
    }

    #[test]
    fn items_spec_uuid_ref_form() {
        let yaml = r#"type: array
items: { type: uuid, ref: organizations.id }"#;
        let fs: FieldSchema = serde_yaml::from_str(yaml).unwrap();
        let items = fs.items.expect("items present");
        assert_eq!(items.field_type, FieldType::Uuid);
        assert_eq!(items.reference.as_deref(), Some("organizations.id"));
    }

    #[test]
    fn items_spec_unknown_field_rejected() {
        let yaml = r#"type: array
items: { type: string, unknown_key: 1 }"#;
        let result: Result<FieldSchema, _> = serde_yaml::from_str(yaml);
        assert!(
            result.is_err(),
            "unknown field on ItemsSpec must be rejected"
        );
    }
}
