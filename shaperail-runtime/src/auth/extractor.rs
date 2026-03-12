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
            let claims = jwt_config
                .decode(token)
                .map_err(|_| ShaperailError::Unauthorized)?;
            if claims.token_type != "access" {
                return Err(ShaperailError::Unauthorized);
            }
            return Ok(AuthenticatedUser {
                id: claims.sub,
                role: claims.role,
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
    fn authenticated_user_debug() {
        let user = AuthenticatedUser {
            id: "u1".to_string(),
            role: "admin".to_string(),
        };
        assert_eq!(user.id, "u1");
        assert_eq!(user.role, "admin");
        // Clone works
        let cloned = user.clone();
        assert_eq!(cloned.id, "u1");
    }
}
