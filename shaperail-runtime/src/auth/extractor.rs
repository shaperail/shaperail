use std::future::{ready, Ready};
use std::sync::Arc;

use actix_web::dev::Payload;
use actix_web::{web, FromRequest, HttpRequest};
use shaperail_core::ShaperailError;

use super::api_key::ApiKeyStore;
use super::jwt::JwtConfig;

/// Authenticated user extracted from a valid JWT Bearer token or API key.
///
/// Use as an Actix-web extractor in handler signatures:
/// ```no_run
/// use actix_web::Responder;
/// use shaperail_runtime::auth::AuthenticatedUser;
///
/// async fn handler(user: AuthenticatedUser) -> impl Responder {
///     format!("Hello, user {}", user.id)
/// }
/// ```
/// Returns 401 if no valid JWT or API key is present.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    /// The user's unique ID (from JWT `sub` claim or API key mapping).
    pub id: String,
    /// The user's role (from JWT `role` claim or API key mapping).
    pub role: String,
    /// The user's tenant ID (M18). Present when multi-tenancy is enabled.
    pub tenant_id: Option<String>,
}

impl FromRequest for AuthenticatedUser {
    type Error = ShaperailError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        ready(extract_auth(req))
    }
}

/// Attempts to extract an authenticated user from the request.
///
/// Checks in order:
/// 1. `Authorization: Bearer <jwt>` header
/// 2. `X-API-Key: <key>` header
fn extract_auth(req: &HttpRequest) -> Result<AuthenticatedUser, ShaperailError> {
    // Try JWT first
    if let Some(auth_header) = req.headers().get("Authorization") {
        let header_str = auth_header
            .to_str()
            .map_err(|_| ShaperailError::Unauthorized)?;
        if let Some(token) = header_str.strip_prefix("Bearer ") {
            let jwt_config = req
                .app_data::<web::Data<Arc<JwtConfig>>>()
                .ok_or(ShaperailError::Internal("JWT not configured".to_string()))?;
            let claims = jwt_config.decode(token).map_err(|err| {
                tracing::warn!(
                    error = %err,
                    "JWT rejected: decode failed"
                );
                ShaperailError::Unauthorized
            })?;
            if claims.token_type != "access" {
                tracing::warn!(
                    token_type = %claims.token_type,
                    sub = %claims.sub,
                    "JWT rejected: token_type must be \"access\""
                );
                return Err(ShaperailError::Unauthorized);
            }
            return Ok(AuthenticatedUser {
                id: claims.sub,
                role: claims.role,
                tenant_id: claims.tenant_id,
            });
        }
    }

    // Try API key
    if let Some(api_key_header) = req.headers().get("X-API-Key") {
        let key = api_key_header
            .to_str()
            .map_err(|_| ShaperailError::Unauthorized)?;
        let store = req
            .app_data::<web::Data<Arc<ApiKeyStore>>>()
            .ok_or(ShaperailError::Unauthorized)?;
        if let Some(user) = store.lookup(key) {
            return Ok(user);
        }
        return Err(ShaperailError::Unauthorized);
    }

    Err(ShaperailError::Unauthorized)
}

/// Optionally extracts auth — returns `None` for public endpoints instead of 401.
pub fn try_extract_auth(req: &HttpRequest) -> Option<AuthenticatedUser> {
    extract_auth(req).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authenticated_user_uses_sub_field() {
        let user = AuthenticatedUser {
            sub: "u1".to_string(),
            role: "admin".to_string(),
            tenant_id: None,
        };
        assert_eq!(user.sub, "u1");
        let cloned = user.clone();
        assert_eq!(cloned.sub, "u1");
    }

    #[test]
    fn try_extract_auth_returns_none_with_no_header() {
        let req = actix_web::test::TestRequest::default().to_http_request();
        let result = try_extract_auth(&req);
        assert!(result.is_none());
    }

    #[test]
    fn try_extract_auth_returns_none_with_garbage_bearer() {
        let req = actix_web::test::TestRequest::default()
            .insert_header(("Authorization", "Bearer garbage.token.notvalid"))
            .to_http_request();
        let result = try_extract_auth(&req);
        assert!(
            result.is_none(),
            "garbage JWT must not yield an authenticated user"
        );
    }

    #[test]
    fn try_extract_auth_returns_none_with_no_jwt_config() {
        // Bearer header present but no JwtConfig registered in app_data → None
        let req = actix_web::test::TestRequest::default()
            .insert_header(("Authorization", "Bearer any.token.here"))
            .to_http_request();
        let result = try_extract_auth(&req);
        assert!(result.is_none());
    }

    #[test]
    fn try_extract_auth_returns_some_with_valid_jwt() {
        use std::sync::Arc;

        use actix_web::web::Data;

        use super::super::jwt::JwtConfig;

        let jwt_config = JwtConfig::new("secret-key-that-is-at-least-32-bytes-long!", 3600, 86400);
        let token = jwt_config.encode_access("user-42", "member").unwrap();

        let req = actix_web::test::TestRequest::default()
            .app_data(Data::new(Arc::new(jwt_config)))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_http_request();

        let result = try_extract_auth(&req);
        assert!(
            result.is_some(),
            "valid JWT must yield an authenticated user"
        );
        let user = result.unwrap();
        assert_eq!(user.id, "user-42");
        assert_eq!(user.role, "member");
    }

    #[test]
    fn try_extract_auth_returns_none_with_api_key_but_no_store() {
        // X-API-Key present but no ApiKeyStore in app_data → None
        let req = actix_web::test::TestRequest::default()
            .insert_header(("X-API-Key", "test-api-key"))
            .to_http_request();
        let result = try_extract_auth(&req);
        assert!(result.is_none());
    }
}
