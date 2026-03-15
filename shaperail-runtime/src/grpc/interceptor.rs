//! JWT authentication interceptor for gRPC (M16).
//!
//! Extracts Bearer tokens from the `authorization` gRPC metadata key and validates
//! them using the same JwtConfig as the REST auth layer.

use std::sync::Arc;

use tonic::{Request, Status};

use crate::auth::extractor::AuthenticatedUser;
use crate::auth::jwt::JwtConfig;

/// Extracts an `AuthenticatedUser` from gRPC request metadata.
///
/// Checks the `authorization` metadata key for a `Bearer <token>` value.
/// Returns `None` for public endpoints (no token), or `Some(user)` if valid.
/// Returns an error status if the token is present but invalid.
#[allow(clippy::result_large_err)]
pub fn extract_grpc_auth(
    req: &Request<()>,
    jwt_config: Option<&JwtConfig>,
) -> Result<Option<AuthenticatedUser>, Status> {
    let metadata = req.metadata();

    let auth_value = match metadata.get("authorization") {
        Some(v) => v,
        None => return Ok(None),
    };

    let auth_str = auth_value
        .to_str()
        .map_err(|_| Status::unauthenticated("Invalid authorization header encoding"))?;

    let token = auth_str
        .strip_prefix("Bearer ")
        .ok_or_else(|| Status::unauthenticated("Authorization must use Bearer scheme"))?;

    let jwt = jwt_config.ok_or_else(|| Status::internal("JWT not configured"))?;

    let claims = jwt
        .decode(token)
        .map_err(|_| Status::unauthenticated("Invalid or expired token"))?;

    if claims.token_type != "access" {
        return Err(Status::unauthenticated("Only access tokens are accepted"));
    }

    Ok(Some(AuthenticatedUser {
        id: claims.sub,
        role: claims.role,
    }))
}

/// Creates a tonic interceptor that validates JWT and attaches auth info.
///
/// Returns a closure suitable for use with `tonic::service::interceptor()`.
#[allow(clippy::result_large_err)]
pub fn auth_interceptor(
    jwt_config: Option<Arc<JwtConfig>>,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
    #[allow(clippy::result_large_err)]
    move |mut req: Request<()>| {
        let user = extract_grpc_auth(&req, jwt_config.as_deref())?;
        if let Some(user) = user {
            req.extensions_mut().insert(user);
        }
        Ok(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataValue;

    fn test_jwt_config() -> JwtConfig {
        JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400)
    }

    #[test]
    fn no_auth_header_returns_none() {
        let req = Request::new(());
        let result = extract_grpc_auth(&req, Some(&test_jwt_config()));
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn valid_token_extracts_user() {
        let jwt = test_jwt_config();
        let token = jwt.encode_access("user-1", "admin").unwrap();

        let mut req = Request::new(());
        req.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}")).unwrap(),
        );

        let result = extract_grpc_auth(&req, Some(&jwt));
        let user = result.unwrap().unwrap();
        assert_eq!(user.id, "user-1");
        assert_eq!(user.role, "admin");
    }

    #[test]
    fn invalid_token_returns_unauthenticated() {
        let jwt = test_jwt_config();

        let mut req = Request::new(());
        req.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from("Bearer invalid.token.here").unwrap(),
        );

        let result = extract_grpc_auth(&req, Some(&jwt));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn missing_bearer_prefix_returns_error() {
        let jwt = test_jwt_config();

        let mut req = Request::new(());
        req.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from("Basic abc123").unwrap(),
        );

        let result = extract_grpc_auth(&req, Some(&jwt));
        assert!(result.is_err());
    }

    #[test]
    fn refresh_token_rejected() {
        let jwt = test_jwt_config();
        let token = jwt.encode_refresh("user-1", "admin").unwrap();

        let mut req = Request::new(());
        req.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}")).unwrap(),
        );

        let result = extract_grpc_auth(&req, Some(&jwt));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn no_jwt_config_returns_internal() {
        let mut req = Request::new(());
        req.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from("Bearer some.token.here").unwrap(),
        );

        let result = extract_grpc_auth(&req, None);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Internal);
    }

    #[test]
    fn auth_interceptor_passes_through_no_auth() {
        let interceptor = auth_interceptor(None);
        let req = Request::new(());
        let result = interceptor(req);
        assert!(result.is_ok());
    }
}
