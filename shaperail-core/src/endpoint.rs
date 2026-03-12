use serde::{Deserialize, Serialize};

/// HTTP method for an endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Patch,
    Put,
    Delete,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Patch => "PATCH",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        };
        write!(f, "{s}")
    }
}

/// Authentication rule for an endpoint.
///
/// Deserializes from `"public"`, `"owner"`, or an array of role strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthRule {
    /// No authentication required.
    Public,
    /// JWT user ID must match the record's owner field.
    Owner,
    /// Requires JWT with one of these roles.
    Roles(Vec<String>),
}

impl Serialize for AuthRule {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Public => serializer.serialize_str("public"),
            Self::Owner => serializer.serialize_str("owner"),
            Self::Roles(roles) => roles.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for AuthRule {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        match &value {
            serde_json::Value::String(s) if s == "public" => Ok(Self::Public),
            serde_json::Value::String(s) if s == "owner" => Ok(Self::Owner),
            serde_json::Value::Array(_) => {
                let roles: Vec<String> =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Roles(roles))
            }
            _ => Err(serde::de::Error::custom(
                "auth must be \"public\", \"owner\", or an array of role strings",
            )),
        }
    }
}

impl std::fmt::Display for AuthRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Public => write!(f, "public"),
            Self::Owner => write!(f, "owner"),
            Self::Roles(roles) => write!(f, "{}", roles.join(", ")),
        }
    }
}

impl AuthRule {
    /// Returns true if this rule allows public (unauthenticated) access.
    pub fn is_public(&self) -> bool {
        matches!(self, Self::Public)
    }

    /// Returns true if this rule requires ownership check.
    pub fn is_owner(&self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Returns true if "owner" appears in the roles list or is the standalone Owner variant.
    pub fn allows_owner(&self) -> bool {
        match self {
            Self::Owner => true,
            Self::Roles(roles) => roles.iter().any(|r| r == "owner"),
            Self::Public => false,
        }
    }
}

/// Pagination strategy for list endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaginationStyle {
    /// Cursor-based pagination (default).
    Cursor,
    /// Offset-based pagination.
    Offset,
}

/// Cache configuration for an endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheSpec {
    /// Time-to-live in seconds.
    pub ttl: u64,
    /// Events that invalidate this cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalidate_on: Option<Vec<String>>,
}

/// File upload configuration for an endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadSpec {
    /// Schema field that stores the file URL.
    pub field: String,
    /// Storage backend (e.g., "s3", "gcs", "local").
    pub storage: String,
    /// Maximum file size (e.g., "5mb").
    pub max_size: String,
    /// Allowed file extensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
}

/// Specification for a single endpoint in a resource.
///
/// Matches the YAML format:
/// ```yaml
/// list:
///   method: GET
///   path: /users
///   auth: [member, admin]
///   pagination: cursor
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointSpec {
    /// HTTP method (GET, POST, PATCH, PUT, DELETE).
    pub method: HttpMethod,

    /// URL path pattern (e.g., "/users", "/users/:id").
    pub path: String,

    /// Authentication/authorization rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthRule>,

    /// Fields accepted as input for create/update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Vec<String>>,

    /// Fields available as query filters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<String>>,

    /// Fields included in full-text search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<Vec<String>>,

    /// Pagination style for list endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pagination: Option<PaginationStyle>,

    /// Fields available for sorting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Vec<String>>,

    /// Cache configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheSpec>,

    /// Hook function names to execute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Vec<String>>,

    /// Events to emit after successful execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,

    /// Background jobs to enqueue after successful execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jobs: Option<Vec<String>>,

    /// File upload configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload: Option<UploadSpec>,

    /// Whether this endpoint performs a soft delete.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub soft_delete: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_method_display() {
        assert_eq!(HttpMethod::Get.to_string(), "GET");
        assert_eq!(HttpMethod::Post.to_string(), "POST");
        assert_eq!(HttpMethod::Patch.to_string(), "PATCH");
        assert_eq!(HttpMethod::Put.to_string(), "PUT");
        assert_eq!(HttpMethod::Delete.to_string(), "DELETE");
    }

    #[test]
    fn auth_rule_public() {
        let auth: AuthRule = serde_json::from_str(r#""public""#).unwrap();
        assert!(auth.is_public());

        let roles: AuthRule = serde_json::from_str(r#"["admin", "member"]"#).unwrap();
        assert!(!roles.is_public());
        if let AuthRule::Roles(r) = &roles {
            assert_eq!(r.len(), 2);
        }
    }

    #[test]
    fn auth_rule_owner_standalone() {
        let auth: AuthRule = serde_json::from_str(r#""owner""#).unwrap();
        assert!(auth.is_owner());
        assert!(auth.allows_owner());
        assert!(!auth.is_public());
    }

    #[test]
    fn auth_rule_owner_in_roles() {
        let auth: AuthRule = serde_json::from_str(r#"["owner", "admin"]"#).unwrap();
        assert!(auth.allows_owner());
        assert!(!auth.is_owner());
    }

    #[test]
    fn pagination_style_serde() {
        let p: PaginationStyle = serde_json::from_str("\"cursor\"").unwrap();
        assert_eq!(p, PaginationStyle::Cursor);
        let p: PaginationStyle = serde_json::from_str("\"offset\"").unwrap();
        assert_eq!(p, PaginationStyle::Offset);
    }

    #[test]
    fn cache_spec_minimal() {
        let json = r#"{"ttl": 60}"#;
        let cs: CacheSpec = serde_json::from_str(json).unwrap();
        assert_eq!(cs.ttl, 60);
        assert!(cs.invalidate_on.is_none());
    }

    #[test]
    fn cache_spec_with_invalidation() {
        let json = r#"{"ttl": 120, "invalidate_on": ["create", "delete"]}"#;
        let cs: CacheSpec = serde_json::from_str(json).unwrap();
        assert_eq!(cs.invalidate_on.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn upload_spec_serde() {
        let json = r#"{"field": "avatar_url", "storage": "s3", "max_size": "5mb", "types": ["jpg", "png"]}"#;
        let us: UploadSpec = serde_json::from_str(json).unwrap();
        assert_eq!(us.field, "avatar_url");
        assert_eq!(us.types.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn endpoint_spec_list() {
        let json = r#"{
            "method": "GET",
            "path": "/users",
            "auth": ["member", "admin"],
            "filters": ["role", "org_id"],
            "search": ["name", "email"],
            "pagination": "cursor",
            "cache": {"ttl": 60}
        }"#;
        let ep: EndpointSpec = serde_json::from_str(json).unwrap();
        assert_eq!(ep.method, HttpMethod::Get);
        assert_eq!(ep.path, "/users");
        assert_eq!(ep.filters.as_ref().unwrap().len(), 2);
        assert_eq!(ep.pagination, Some(PaginationStyle::Cursor));
        assert!(!ep.soft_delete);
    }

    #[test]
    fn endpoint_spec_create() {
        let json = r#"{
            "method": "POST",
            "path": "/users",
            "auth": ["admin"],
            "input": ["email", "name", "role", "org_id"],
            "hooks": ["validate_org"],
            "events": ["user.created"],
            "jobs": ["send_welcome_email"]
        }"#;
        let ep: EndpointSpec = serde_json::from_str(json).unwrap();
        assert_eq!(ep.method, HttpMethod::Post);
        assert_eq!(ep.hooks.as_ref().unwrap(), &["validate_org"]);
        assert_eq!(ep.jobs.as_ref().unwrap(), &["send_welcome_email"]);
    }

    #[test]
    fn endpoint_spec_delete_soft() {
        let json = r#"{
            "method": "DELETE",
            "path": "/users/:id",
            "auth": ["admin"],
            "soft_delete": true
        }"#;
        let ep: EndpointSpec = serde_json::from_str(json).unwrap();
        assert!(ep.soft_delete);
    }
}
