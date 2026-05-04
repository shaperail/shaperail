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

/// One or more controller hook names declared under `before:` or `after:`.
///
/// YAML accepts both the scalar form (single hook) and the array form
/// (chain of hooks). Rust callers iterate via `names()` regardless of shape.
///
/// ```yaml
/// controller: { before: validate_org }                      # Single
/// controller: { before: [check_currency, check_org_match] } # Multi
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookList {
    Single(String),
    Multi(Vec<String>),
}

impl HookList {
    /// Returns the hook names as a slice, regardless of whether the YAML
    /// declared the single or multi form.
    pub fn names(&self) -> &[String] {
        match self {
            HookList::Single(name) => std::slice::from_ref(name),
            HookList::Multi(names) => names.as_slice(),
        }
    }

    /// Returns `true` if any name in the list begins with `wasm:`.
    pub fn has_wasm(&self) -> bool {
        self.names().iter().any(|n| n.starts_with(WASM_HOOK_PREFIX))
    }
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
    /// Function(s) to call before the DB operation. Runs in declaration order;
    /// first error short-circuits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<HookList>,
    /// Function(s) to call after the DB operation. Runs in declaration order;
    /// first error short-circuits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<HookList>,
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
    /// Returns `true` if any `before` hook references a WASM plugin.
    pub fn has_wasm_before(&self) -> bool {
        self.before.as_ref().is_some_and(|h| h.has_wasm())
    }

    /// Returns `true` if any `after` hook references a WASM plugin.
    pub fn has_wasm_after(&self) -> bool {
        self.after.as_ref().is_some_and(|h| h.has_wasm())
    }

    /// Iterates `before` hook names; empty if no controller is declared.
    pub fn before_names(&self) -> &[String] {
        self.before.as_ref().map(|h| h.names()).unwrap_or(&[])
    }

    /// Iterates `after` hook names; empty if no controller is declared.
    pub fn after_names(&self) -> &[String] {
        self.after.as_ref().map(|h| h.names()).unwrap_or(&[])
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

/// Converts an Express-style path (with `:param` segments) to a brace-style
/// path (with `{param}` segments) used by Actix-router and OpenAPI 3.1.
///
/// A `:param` segment is recognised when the colon is followed by a Rust-like
/// identifier (`[A-Za-z_][A-Za-z0-9_]*`). Any colon not followed by an
/// identifier character is left untouched.
///
/// Examples:
/// - `/users/:id` → `/users/{id}`
/// - `/vendors/:vendor_id/webhook/:token` → `/vendors/{vendor_id}/webhook/{token}`
/// - `/literal:colon` → `/literal:colon` (colon not followed by identifier)
pub fn to_brace_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = String::with_capacity(path.len() + 4);
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' && i + 1 < bytes.len() && is_ident_start(bytes[i + 1]) {
            let start = i + 1;
            let mut end = start + 1;
            while end < bytes.len() && is_ident_continue(bytes[end]) {
                end += 1;
            }
            out.push('{');
            // SAFETY: we only consumed ASCII identifier bytes.
            out.push_str(&path[start..end]);
            out.push('}');
            i = end;
        } else {
            // Push one UTF-8 char from the original string, advancing `i` by
            // its byte length. (`bytes[i]` is the leading byte; multibyte
            // chars are not identifier bytes anyway, so this branch handles
            // them transparently.)
            let ch = path[i..].chars().next().expect("non-empty remainder");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

#[inline]
fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

#[inline]
fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_brace_path_id_only() {
        assert_eq!(to_brace_path("/users/:id"), "/users/{id}");
    }

    #[test]
    fn to_brace_path_multiple_named_params() {
        assert_eq!(
            to_brace_path("/vendors/:vendor_id/webhook/:webhook_path_token"),
            "/vendors/{vendor_id}/webhook/{webhook_path_token}"
        );
    }

    #[test]
    fn to_brace_path_no_params() {
        assert_eq!(to_brace_path("/users"), "/users");
        assert_eq!(to_brace_path(""), "");
    }

    #[test]
    fn to_brace_path_leaves_non_identifier_colons_alone() {
        // A bare `:` not followed by an identifier byte is part of the literal path.
        assert_eq!(to_brace_path("/a:/b"), "/a:/b");
        assert_eq!(to_brace_path("/a/:/b"), "/a/:/b");
    }

    #[test]
    fn to_brace_path_underscore_and_digits_in_param() {
        assert_eq!(to_brace_path("/x/:_param/y"), "/x/{_param}/y");
        assert_eq!(to_brace_path("/x/:p1/y"), "/x/{p1}/y");
    }

    #[test]
    fn to_brace_path_consecutive_params() {
        assert_eq!(to_brace_path("/:a/:b"), "/{a}/{b}");
    }

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
        assert_eq!(
            ctrl.before.as_ref().unwrap().names(),
            &["validate_org".to_string()]
        );
        assert!(ctrl.after.is_none());
        assert_eq!(ep.jobs.as_ref().unwrap(), &["send_welcome_email"]);
    }

    #[test]
    fn controller_spec_full() {
        let json = r#"{"before": "check_input", "after": "enrich"}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        assert_eq!(
            cs.before.as_ref().unwrap().names(),
            &["check_input".to_string()]
        );
        assert_eq!(cs.after.as_ref().unwrap().names(), &["enrich".to_string()]);
    }

    #[test]
    fn controller_spec_after_only() {
        let json = r#"{"after": "enrich"}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        assert!(cs.before.is_none());
        assert_eq!(cs.after.as_ref().unwrap().names(), &["enrich".to_string()]);
    }

    #[test]
    fn controller_wasm_before_detection() {
        let json = r#"{"before": "wasm:./plugins/my_validator.wasm"}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        assert!(cs.has_wasm_before());
        assert!(!cs.has_wasm_after());
    }

    #[test]
    fn controller_wasm_after_detection() {
        let json = r#"{"after": "wasm:./plugins/my_enricher.wasm"}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        assert!(!cs.has_wasm_before());
        assert!(cs.has_wasm_after());
    }

    #[test]
    fn controller_rust_not_detected_as_wasm() {
        let json = r#"{"before": "validate_org"}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        assert!(!cs.has_wasm_before());
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

    // -- endpoint_convention tests --

    #[test]
    fn endpoint_convention_known_actions() {
        assert_eq!(
            endpoint_convention("list", "users"),
            Some((HttpMethod::Get, "/users".to_string()))
        );
        assert_eq!(
            endpoint_convention("get", "users"),
            Some((HttpMethod::Get, "/users/:id".to_string()))
        );
        assert_eq!(
            endpoint_convention("create", "users"),
            Some((HttpMethod::Post, "/users".to_string()))
        );
        assert_eq!(
            endpoint_convention("update", "users"),
            Some((HttpMethod::Patch, "/users/:id".to_string()))
        );
        assert_eq!(
            endpoint_convention("delete", "users"),
            Some((HttpMethod::Delete, "/users/:id".to_string()))
        );
    }

    #[test]
    fn endpoint_convention_unknown_action_returns_none() {
        assert_eq!(endpoint_convention("archive", "users"), None);
        assert_eq!(endpoint_convention("custom_action", "orders"), None);
        assert_eq!(endpoint_convention("", "users"), None);
    }

    #[test]
    fn endpoint_convention_uses_resource_name() {
        let (method, path) = endpoint_convention("list", "orders").unwrap();
        assert_eq!(method, HttpMethod::Get);
        assert_eq!(path, "/orders");
        let (_, path) = endpoint_convention("get", "blog_posts").unwrap();
        assert_eq!(path, "/blog_posts/:id");
    }

    // -- apply_endpoint_defaults tests --

    #[test]
    fn apply_endpoint_defaults_fills_missing_method_and_path() {
        use crate::ResourceDefinition;
        use indexmap::IndexMap;

        let mut endpoints = IndexMap::new();
        endpoints.insert(
            "list".to_string(),
            EndpointSpec {
                method: None,
                path: None,
                auth: None,
                ..Default::default()
            },
        );
        endpoints.insert(
            "create".to_string(),
            EndpointSpec {
                method: None,
                path: None,
                auth: None,
                ..Default::default()
            },
        );

        let mut rd = ResourceDefinition {
            resource: "posts".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema: IndexMap::new(),
            endpoints: Some(endpoints),
            relations: None,
            indexes: None,
        };

        crate::apply_endpoint_defaults(&mut rd);

        let eps = rd.endpoints.as_ref().unwrap();
        assert_eq!(*eps["list"].method.as_ref().unwrap(), HttpMethod::Get);
        assert_eq!(eps["list"].path.as_deref(), Some("/posts"));
        assert_eq!(*eps["create"].method.as_ref().unwrap(), HttpMethod::Post);
        assert_eq!(eps["create"].path.as_deref(), Some("/posts"));
    }

    #[test]
    fn apply_endpoint_defaults_does_not_overwrite_explicit_values() {
        use crate::ResourceDefinition;
        use indexmap::IndexMap;

        let mut endpoints = IndexMap::new();
        endpoints.insert(
            "list".to_string(),
            EndpointSpec {
                method: Some(HttpMethod::Post),         // overridden
                path: Some("/custom/path".to_string()), // overridden
                auth: None,
                ..Default::default()
            },
        );

        let mut rd = ResourceDefinition {
            resource: "items".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema: IndexMap::new(),
            endpoints: Some(endpoints),
            relations: None,
            indexes: None,
        };

        crate::apply_endpoint_defaults(&mut rd);

        let eps = rd.endpoints.as_ref().unwrap();
        // Explicitly set values must not be overwritten
        assert_eq!(*eps["list"].method.as_ref().unwrap(), HttpMethod::Post);
        assert_eq!(eps["list"].path.as_deref(), Some("/custom/path"));
    }

    #[test]
    fn apply_endpoint_defaults_unknown_action_not_filled() {
        use crate::ResourceDefinition;
        use indexmap::IndexMap;

        let mut endpoints = IndexMap::new();
        endpoints.insert(
            "archive".to_string(),
            EndpointSpec {
                method: None,
                path: None,
                auth: None,
                ..Default::default()
            },
        );

        let mut rd = ResourceDefinition {
            resource: "items".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema: IndexMap::new(),
            endpoints: Some(endpoints),
            relations: None,
            indexes: None,
        };

        crate::apply_endpoint_defaults(&mut rd);

        // Unknown actions must not get defaults
        let eps = rd.endpoints.as_ref().unwrap();
        assert!(eps["archive"].method.is_none());
        assert!(eps["archive"].path.is_none());
    }

    #[test]
    fn apply_endpoint_defaults_no_endpoints_is_noop() {
        use crate::ResourceDefinition;
        use indexmap::IndexMap;

        let mut rd = ResourceDefinition {
            resource: "items".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema: IndexMap::new(),
            endpoints: None,
            relations: None,
            indexes: None,
        };

        // Must not panic
        crate::apply_endpoint_defaults(&mut rd);
        assert!(rd.endpoints.is_none());
    }

    // -- AuthRule::Display tests --

    #[test]
    fn auth_rule_display() {
        assert_eq!(AuthRule::Public.to_string(), "public");
        assert_eq!(AuthRule::Owner.to_string(), "owner");
        assert_eq!(
            AuthRule::Roles(vec!["admin".to_string(), "member".to_string()]).to_string(),
            "admin, member"
        );
        assert_eq!(
            AuthRule::Roles(vec!["viewer".to_string()]).to_string(),
            "viewer"
        );
    }

    // -- AuthRule serialization roundtrip --

    #[test]
    fn auth_rule_serde_roundtrip_public() {
        let rule = AuthRule::Public;
        let json = serde_json::to_string(&rule).unwrap();
        assert_eq!(json, r#""public""#);
        let back: AuthRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AuthRule::Public);
    }

    #[test]
    fn auth_rule_serde_roundtrip_owner() {
        let rule = AuthRule::Owner;
        let json = serde_json::to_string(&rule).unwrap();
        assert_eq!(json, r#""owner""#);
        let back: AuthRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AuthRule::Owner);
    }

    #[test]
    fn auth_rule_serde_roundtrip_roles() {
        let rule = AuthRule::Roles(vec!["admin".to_string(), "member".to_string()]);
        let json = serde_json::to_string(&rule).unwrap();
        let back: AuthRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rule);
    }

    #[test]
    fn auth_rule_deserialize_invalid_type_fails() {
        // A number is neither a string nor an array — must fail
        let result = serde_json::from_str::<AuthRule>("42");
        assert!(result.is_err());
    }

    #[test]
    fn auth_rule_deserialize_unknown_string_fails() {
        // "superadmin" is not "public" or "owner"
        let result = serde_json::from_str::<AuthRule>(r#""superadmin""#);
        assert!(result.is_err());
    }

    // -- HttpMethod serde --

    #[test]
    fn http_method_serde_roundtrip() {
        for m in [
            HttpMethod::Get,
            HttpMethod::Post,
            HttpMethod::Patch,
            HttpMethod::Put,
            HttpMethod::Delete,
        ] {
            let json = serde_json::to_string(&m).unwrap();
            let back: HttpMethod = serde_json::from_str(&json).unwrap();
            assert_eq!(m, back);
        }
    }

    // -- HookList tests --

    #[test]
    fn controller_before_accepts_scalar_string() {
        let json = r#"{"before": "validate_org"}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        let names = cs.before.as_ref().unwrap().names();
        assert_eq!(names, &["validate_org".to_string()]);
    }

    #[test]
    fn controller_before_accepts_array() {
        let json = r#"{"before": ["a", "b", "c"]}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        let names = cs.before.as_ref().unwrap().names();
        assert_eq!(names, &["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn controller_after_accepts_array() {
        let json = r#"{"after": ["enrich_a", "enrich_b"]}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        let names = cs.after.as_ref().unwrap().names();
        assert_eq!(names, &["enrich_a".to_string(), "enrich_b".to_string()]);
    }

    #[test]
    fn hook_list_wasm_detection_works_for_array() {
        let json = r#"{"before": ["validate_x", "wasm:./plugin.wasm"]}"#;
        let cs: ControllerSpec = serde_json::from_str(json).unwrap();
        assert!(cs.has_wasm_before());
    }
}
