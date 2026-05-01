/// All supported field types in a Shaperail resource schema.
///
/// Each variant maps to a specific SQL type, Rust type, and validation behavior.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// UUID primary keys and foreign keys.
    Uuid,
    /// Variable-length text. Use `max` constraint for VARCHAR(n).
    String,
    /// 32-bit signed integer.
    Integer,
    /// 64-bit signed integer.
    Bigint,
    /// 64-bit floating point (SQL NUMERIC).
    Number,
    /// Boolean true/false.
    Boolean,
    /// Timestamp with timezone (SQL TIMESTAMPTZ).
    Timestamp,
    /// Date without time component.
    Date,
    /// Enumerated string values. Requires `values` constraint.
    Enum,
    /// Arbitrary JSON (SQL JSONB).
    Json,
    /// Array of a sub-type. Use `items` to specify element type.
    Array,
    /// File reference stored as URL text. Backed by object storage.
    File,
}

impl FieldType {
    /// Returns the corresponding Rust type string for code generation.
    pub fn to_rust_type(&self, required: bool, nullable: bool, generated: bool) -> String {
        let base = match self {
            Self::Uuid => "uuid::Uuid",
            Self::String | Self::Enum | Self::File => "String",
            Self::Integer => "i32",
            Self::Bigint => "i64",
            Self::Number => "f64",
            Self::Boolean => "bool",
            Self::Timestamp => "chrono::DateTime<chrono::Utc>",
            Self::Date => "chrono::NaiveDate",
            Self::Json => "serde_json::Value",
            Self::Array => "Vec<serde_json::Value>",
        };
        if nullable || generated || !required {
            format!("Option<{base}>")
        } else {
            base.to_string()
        }
    }
}

impl std::fmt::Display for FieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Uuid => "uuid",
            Self::String => "string",
            Self::Integer => "integer",
            Self::Bigint => "bigint",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Timestamp => "timestamp",
            Self::Date => "date",
            Self::Enum => "enum",
            Self::Json => "json",
            Self::Array => "array",
            Self::File => "file",
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_type_display() {
        assert_eq!(FieldType::Uuid.to_string(), "uuid");
        assert_eq!(FieldType::String.to_string(), "string");
        assert_eq!(FieldType::Integer.to_string(), "integer");
        assert_eq!(FieldType::Bigint.to_string(), "bigint");
        assert_eq!(FieldType::Number.to_string(), "number");
        assert_eq!(FieldType::Boolean.to_string(), "boolean");
        assert_eq!(FieldType::Timestamp.to_string(), "timestamp");
        assert_eq!(FieldType::Date.to_string(), "date");
        assert_eq!(FieldType::Enum.to_string(), "enum");
        assert_eq!(FieldType::Json.to_string(), "json");
        assert_eq!(FieldType::Array.to_string(), "array");
        assert_eq!(FieldType::File.to_string(), "file");
    }

    #[test]
    fn field_type_serde_roundtrip() {
        let variants = vec![
            FieldType::Uuid,
            FieldType::String,
            FieldType::Integer,
            FieldType::Bigint,
            FieldType::Number,
            FieldType::Boolean,
            FieldType::Timestamp,
            FieldType::Date,
            FieldType::Enum,
            FieldType::Json,
            FieldType::Array,
            FieldType::File,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let back: FieldType = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }

    #[test]
    fn field_type_deserializes_from_string() {
        let ft: FieldType = serde_json::from_str("\"uuid\"").unwrap();
        assert_eq!(ft, FieldType::Uuid);
        let ft: FieldType = serde_json::from_str("\"timestamp\"").unwrap();
        assert_eq!(ft, FieldType::Timestamp);
    }

    #[test]
    fn to_rust_type_optional_for_non_required_fields() {
        assert_eq!(
            FieldType::String.to_rust_type(false, false, false),
            "Option<String>"
        );
        assert_eq!(FieldType::String.to_rust_type(true, false, false), "String");
    }

    #[test]
    fn to_rust_type_all_required_variants() {
        let cases: &[(&FieldType, &str)] = &[
            (&FieldType::Uuid, "uuid::Uuid"),
            (&FieldType::String, "String"),
            (&FieldType::Integer, "i32"),
            (&FieldType::Bigint, "i64"),
            (&FieldType::Number, "f64"),
            (&FieldType::Boolean, "bool"),
            (&FieldType::Timestamp, "chrono::DateTime<chrono::Utc>"),
            (&FieldType::Date, "chrono::NaiveDate"),
            (&FieldType::Json, "serde_json::Value"),
            (&FieldType::Array, "Vec<serde_json::Value>"),
            (&FieldType::Enum, "String"),
            (&FieldType::File, "String"),
        ];
        for (ft, expected) in cases {
            assert_eq!(
                ft.to_rust_type(true, false, false),
                *expected,
                "required=true for {ft}"
            );
        }
    }

    #[test]
    fn to_rust_type_wrapped_when_nullable() {
        assert_eq!(
            FieldType::Integer.to_rust_type(true, true, false),
            "Option<i32>"
        );
        assert_eq!(
            FieldType::Uuid.to_rust_type(true, true, false),
            "Option<uuid::Uuid>"
        );
    }

    #[test]
    fn to_rust_type_wrapped_when_generated() {
        assert_eq!(
            FieldType::Timestamp.to_rust_type(false, false, true),
            "Option<chrono::DateTime<chrono::Utc>>"
        );
        assert_eq!(
            FieldType::Uuid.to_rust_type(true, false, true),
            "Option<uuid::Uuid>"
        );
    }

    #[test]
    fn to_rust_type_not_optional_when_required_not_nullable_not_generated() {
        assert_eq!(FieldType::Boolean.to_rust_type(true, false, false), "bool");
        assert_eq!(FieldType::Number.to_rust_type(true, false, false), "f64");
        assert_eq!(
            FieldType::Date.to_rust_type(true, false, false),
            "chrono::NaiveDate"
        );
    }
}
