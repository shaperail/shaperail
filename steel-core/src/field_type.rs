/// All supported field types in a SteelAPI resource schema.
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
}
