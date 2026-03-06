use serde::{Deserialize, Serialize};

/// Type of relationship between two resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    /// This resource has a foreign key pointing to the related resource.
    BelongsTo,
    /// The related resource has a foreign key pointing to this resource (many records).
    HasMany,
    /// The related resource has a foreign key pointing to this resource (one record).
    HasOne,
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::BelongsTo => "belongs_to",
            Self::HasMany => "has_many",
            Self::HasOne => "has_one",
        };
        write!(f, "{s}")
    }
}

/// Specification for a relationship to another resource.
///
/// ```yaml
/// organization: { resource: organizations, type: belongs_to, key: org_id }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationSpec {
    /// Name of the related resource.
    pub resource: String,

    /// Type of relationship.
    #[serde(rename = "type")]
    pub relation_type: RelationType,

    /// Local foreign key field (for belongs_to).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,

    /// Foreign key on the related resource (for has_many/has_one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreign_key: Option<String>,
}

/// Specification for a database index.
///
/// ```yaml
/// indexes:
///   - fields: [org_id, role]
///   - fields: [created_at], order: desc
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexSpec {
    /// Fields included in this index.
    pub fields: Vec<String>,

    /// Whether this is a unique index.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unique: bool,

    /// Sort order for the index (asc/desc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_type_display() {
        assert_eq!(RelationType::BelongsTo.to_string(), "belongs_to");
        assert_eq!(RelationType::HasMany.to_string(), "has_many");
        assert_eq!(RelationType::HasOne.to_string(), "has_one");
    }

    #[test]
    fn relation_type_serde() {
        let rt: RelationType = serde_json::from_str("\"belongs_to\"").unwrap();
        assert_eq!(rt, RelationType::BelongsTo);
        let rt: RelationType = serde_json::from_str("\"has_many\"").unwrap();
        assert_eq!(rt, RelationType::HasMany);
        let rt: RelationType = serde_json::from_str("\"has_one\"").unwrap();
        assert_eq!(rt, RelationType::HasOne);
    }

    #[test]
    fn relation_spec_belongs_to() {
        let json = r#"{"resource": "organizations", "type": "belongs_to", "key": "org_id"}"#;
        let rs: RelationSpec = serde_json::from_str(json).unwrap();
        assert_eq!(rs.resource, "organizations");
        assert_eq!(rs.relation_type, RelationType::BelongsTo);
        assert_eq!(rs.key.as_deref(), Some("org_id"));
        assert!(rs.foreign_key.is_none());
    }

    #[test]
    fn relation_spec_has_many() {
        let json = r#"{"resource": "orders", "type": "has_many", "foreign_key": "user_id"}"#;
        let rs: RelationSpec = serde_json::from_str(json).unwrap();
        assert_eq!(rs.relation_type, RelationType::HasMany);
        assert_eq!(rs.foreign_key.as_deref(), Some("user_id"));
        assert!(rs.key.is_none());
    }

    #[test]
    fn relation_spec_has_one() {
        let json = r#"{"resource": "profiles", "type": "has_one", "foreign_key": "user_id"}"#;
        let rs: RelationSpec = serde_json::from_str(json).unwrap();
        assert_eq!(rs.relation_type, RelationType::HasOne);
    }

    #[test]
    fn index_spec_composite() {
        let json = r#"{"fields": ["org_id", "role"]}"#;
        let idx: IndexSpec = serde_json::from_str(json).unwrap();
        assert_eq!(idx.fields, vec!["org_id", "role"]);
        assert!(!idx.unique);
        assert!(idx.order.is_none());
    }

    #[test]
    fn index_spec_with_order() {
        let json = r#"{"fields": ["created_at"], "order": "desc"}"#;
        let idx: IndexSpec = serde_json::from_str(json).unwrap();
        assert_eq!(idx.order.as_deref(), Some("desc"));
    }

    #[test]
    fn index_spec_unique() {
        let json = r#"{"fields": ["email"], "unique": true}"#;
        let idx: IndexSpec = serde_json::from_str(json).unwrap();
        assert!(idx.unique);
    }
}
