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

/// Per-endpoint rate limiting configuration.
///
/// Declared in resource YAML:
/// ```yaml
/// list:
///   auth: [member]
///   rate_limit: { max_requests: 100, window_secs: 60 }
/// ```
///
/// Requires Redis. Silently skipped if Redis is not configured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitSpec {
    /// Maximum requests allowed within the window.
    pub max_requests: u64,
    /// Window duration in seconds.
    pub window_secs: u64,
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

/// Controller specification for synchronous in-request business logic.
///
/// Declared per-endpoint in the resource YAML:
/// ```yaml
/// controller:
///   before: validate_org
///   after: enrich_response
/// ```
///
/// Functions live in `resources/<resource>.controller.rs` and are called
/// synchronously within the request lifecycle (before/after the DB operation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerSpec {
    /// Function to call before the DB operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Function to call after the DB operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// Known endpoint conventions. When the endpoint action name matches one of these,
/// the method and path are inferred automatically.
pub fn endpoint_convention(action: &str, resource_name: &str) -> Option<(HttpMethod, String)> {
    match action {
        "list" => Some((HttpMethod::Get, format!("/{resource_name}"))),
        "get" => Some((HttpMethod::Get, format!("/{resource_name}/:id"))),
        "create" => Some((HttpMethod::Post, format!("/{resource_name}"))),
        "update" => Some((HttpMethod::Patch, format!("/{resource_name}/:id"))),
        "delete" => Some((HttpMethod::Delete, format!("/{resource_name}/:id"))),
        _ => None,
    }
}

/// Apply convention-based defaults to all endpoints in a resource.
/// Fills in missing `method` and `path` based on the endpoint name.
pub fn apply_endpoint_defaults(resource: &mut super::ResourceDefinition) {
    let resource_name = resource.resource.clone();
    if let Some(ref mut endpoints) = resource.endpoints {
        for (action, ep) in endpoints.iter_mut() {
            if let Some((default_method, default_path)) =
                endpoint_convention(action, &resource_name)
            {
                if ep.method.is_none() {
                    ep.method = Some(default_method);
                }
                if ep.path.is_none() {
                    ep.path = Some(default_path);
                }
            }
        }
    }
}

/// WASM plugin prefix used in controller `before`/`after` fields.
///
/// When a controller name starts with `wasm:`, the remainder is interpreted
/// as a path to a `.wasm` plugin file. Example:
/// ```yaml
/// controller:
///   before: "wasm:./plugins/my_validator.wasm"
/// ```
pub const WASM_HOOK_PREFIX: &str = "wasm:";

impl ControllerSpec {
    /// Returns `true` if the `before` controller references a WASM plugin.
    pub fn has_wasm_before(&self) -> bool {
        self.before
            .as_ref()
            .is_some_and(|s| s.starts_with(WASM_HOOK_PREFIX))
    }

    /// Returns `true` if the `after` controller references a WASM plugin.
    pub fn has_wasm_after(&self) -> bool {
        self.after
            .as_ref()
            .is_some_and(|s| s.starts_with(WASM_HOOK_PREFIX))
    }

    /// Extracts the WASM plugin path from a `before` controller, if present.
    pub fn wasm_before_path(&self) -> Option<&str> {
        self.before
            .as_ref()
            .filter(|s| s.starts_with(WASM_HOOK_PREFIX))
            .map(|s| &s[WASM_HOOK_PREFIX.len()..])
    }

    /// Extracts the WASM plugin path from an `after` controller, if present.
    pub fn wasm_after_path(&self) -> Option<&str> {
        self.after
            .as_ref()
            .filter(|s| s.starts_with(WASM_HOOK_PREFIX))
            .map(|s| &s[WASM_HOOK_PREFIX.len()..])
    }
}

/// A subscriber declaration within an endpoint — auto-registered at startup.
///
/// ```yaml
/// subscribers:
///   - event: user.created
///     handler: send_welcome_email
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriberSpec {
    /// Event name pattern (e.g., "user.created", "*.deleted").
    pub event: String,
    /// Handler function name in `resources/<resource>.controller.rs`.
    pub handler: String,
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
///
/// When the endpoint name matches a known convention (list, get, create, update, delete),
/// `method` and `path` can be omitted and will be inferred from the resource name.
/// Use `apply_endpoint_defaults()` after parsing to fill them in.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointSpec {
    /// HTTP method (GET, POST, PATCH, PUT, DELETE).
    /// Optional when endpoint name is a known convention (list, get, create, update, delete).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<HttpMethod>,

    /// URL path pattern (e.g., "/users", "/users/:id").
    /// Optional when endpoint name is a known convention (list, get, create, update, delete).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

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

    /// Controller functions for synchronous in-request business logic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<ControllerSpec>,

    /// Events to emit after successful execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,

    /// Background jobs to enqueue after successful execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jobs: Option<Vec<String>>,

    /// Event subscribers auto-registered at startup; each entry maps an event pattern to a handler function.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscribers: Option<Vec<SubscriberSpec>>,

    /// Handler function name for non-convention endpoints.
    /// Required when the endpoint action name is not list/get/create/update/delete.
    /// The function must be defined in `resources/<resource>.controller.rs`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,

    /// File upload configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload: Option<UploadSpec>,

    /// Per-endpoint rate limiting. Requires Redis. Skipped if Redis is not configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitSpec>,

    /// Whether this endpoint performs a soft delete.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub soft_delete: bool,
}

impl EndpointSpec {
    /// Returns the resolved HTTP method. Panics if method is None
    /// (should never happen after `apply_endpoint_defaults`).
    pub fn method(&self) -> &HttpMethod {
        self.method
            .as_ref()
            .expect("EndpointSpec.method must be set — call apply_endpoint_defaults() first")
    }

    /// Returns the resolved path. Panics if path is None
    /// (should never happen after `apply_endpoint_defaults`).
    pub fn path(&self) -> &str {
        self.path
            .as_deref()
            .expect("EndpointSpec.path must be set — call apply_endpoint_defaults() first")
    }
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
        assert_eq!(*ep.method(), HttpMethod::Get);
        assert_eq!(ep.path(), "/users");
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
            "controller": {"before": "validate_org"},
            "events": ["user.created"],
            "jobs": ["send_welcome_email"]
        }"#;
        let ep: EndpointSpec = serde_json::from_str(json).unwrap();
        assert_eq!(*ep.method(), HttpMethod::Post);
        let ctrl = ep.controller.as_ref().unwrap();
        assert_eq!(ctrl.before.as_deref(), Some("validate_org"));
        assert!(ctrl.after.is_none());
        assert_eq!(ep.jobs.as_ref().unwrap(), &["send_welcome_email"]);
    }

    #[test]
    fn controller_spec_full() {
        let json = r#"{"before": "check_input", "after": "enrich"}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        assert_eq!(cs.before.as_deref(), Some("check_input"));
        assert_eq!(cs.after.as_deref(), Some("enrich"));
    }

    #[test]
    fn controller_spec_after_only() {
        let json = r#"{"after": "enrich"}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        assert!(cs.before.is_none());
        assert_eq!(cs.after.as_deref(), Some("enrich"));
    }

    #[test]
    fn controller_wasm_before_detection() {
        let json = r#"{"before": "wasm:./plugins/my_validator.wasm"}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        assert!(cs.has_wasm_before());
        assert!(!cs.has_wasm_after());
        assert_eq!(cs.wasm_before_path(), Some("./plugins/my_validator.wasm"));
        assert_eq!(cs.wasm_after_path(), None);
    }

    #[test]
    fn controller_wasm_after_detection() {
        let json = r#"{"after": "wasm:./plugins/my_enricher.wasm"}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        assert!(!cs.has_wasm_before());
        assert!(cs.has_wasm_after());
        assert_eq!(cs.wasm_after_path(), Some("./plugins/my_enricher.wasm"));
    }

    #[test]
    fn controller_rust_not_detected_as_wasm() {
        let json = r#"{"before": "validate_org"}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        assert!(!cs.has_wasm_before());
        assert_eq!(cs.wasm_before_path(), None);
    }

    #[test]
    fn hooks_key_rejected() {
        let json = r#"{
            "method": "POST",
            "path": "/users",
            "hooks": ["validate_org"]
        }"#;
        let result = serde_json::from_str::<EndpointSpec>(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "Expected deny_unknown_fields to reject 'hooks', got: {err}"
        );
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

    #[test]
    fn rate_limit_spec_parses_from_yaml() {
        let yaml = "max_requests: 50\nwindow_secs: 30\n";
        let spec: RateLimitSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.max_requests, 50);
        assert_eq!(spec.window_secs, 30);
    }

    #[test]
    fn endpoint_spec_rate_limit_field_roundtrips() {
        let yaml = r#"
auth: [member]
rate_limit:
  max_requests: 100
  window_secs: 60
"#;
        let spec: EndpointSpec = serde_yaml::from_str(yaml).unwrap();
        let rl = spec.rate_limit.unwrap();
        assert_eq!(rl.max_requests, 100);
        assert_eq!(rl.window_secs, 60);
    }

    #[test]
    fn endpoint_spec_rate_limit_absent_is_none() {
        let yaml = "auth: [member]\n";
        let spec: EndpointSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(spec.rate_limit.is_none());
    }

    #[test]
    fn endpoint_spec_subscribers_parse() {
        let yaml = r#"
auth: [admin]
events: [user.created]
subscribers:
  - event: user.created
    handler: send_welcome_email
  - event: "*.deleted"
    handler: cleanup_resources
"#;
        let spec: EndpointSpec = serde_yaml::from_str(yaml).unwrap();
        let subs = spec.subscribers.as_ref().unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].event, "user.created");
        assert_eq!(subs[0].handler, "send_welcome_email");
        assert_eq!(subs[1].event, "*.deleted");
    }

    #[test]
    fn subscriber_spec_unknown_field_rejected() {
        let yaml = r#"
subscribers:
  - event: user.created
    handler: send_welcome_email
    extra: bad_field
"#;
        let result = serde_yaml::from_str::<EndpointSpec>(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn custom_endpoint_handler_field_parses() {
        let yaml = r#"
method: POST
path: /invite
auth: [admin]
input: [email, role]
handler: invite_user
"#;
        let ep: EndpointSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(ep.handler.as_deref(), Some("invite_user"));
        assert_eq!(ep.input.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn endpoint_without_handler_has_none() {
        let yaml = "auth: [member]\n";
        let ep: EndpointSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(ep.handler.is_none());
    }
}
