use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use shaperail_core::ShaperailError;
use std::sync::Arc;

use super::jwt::JwtConfig;

/// Request body for issuing a new token pair.
///
/// In a real application, this would verify credentials against a database.
/// For the framework scaffold, the consumer implements the credential check
/// and calls `issue_tokens` with the verified user info.
#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    /// User ID (pre-verified by the consumer's auth logic).
    pub user_id: String,
    /// User role (pre-verified by the consumer's auth logic).
    pub role: String,
}

/// Request body for refreshing tokens using a refresh token.
#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    /// The refresh token to exchange for a new token pair.
    pub refresh_token: String,
}

/// Response containing an access + refresh token pair.
#[derive(Debug, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// Handler: POST /auth/token — issue a new token pair.
///
/// Accepts a `TokenRequest` with pre-verified user credentials.
/// Returns a `TokenPair` with access and refresh tokens.
pub async fn handle_issue_token(
    jwt: web::Data<Arc<JwtConfig>>,
    body: web::Json<TokenRequest>,
) -> Result<HttpResponse, ShaperailError> {
    let access = jwt
        .encode_access(&body.user_id, &body.role)
        .map_err(|e| ShaperailError::Internal(format!("Token encoding failed: {e}")))?;

    let refresh = jwt
        .encode_refresh(&body.user_id, &body.role)
        .map_err(|e| ShaperailError::Internal(format!("Token encoding failed: {e}")))?;

    let pair = TokenPair {
        access_token: access,
        refresh_token: refresh,
        token_type: "Bearer".to_string(),
        expires_in: jwt.access_ttl.num_seconds(),
    };

    Ok(HttpResponse::Ok().json(pair))
}

/// Handler: POST /auth/refresh — exchange a refresh token for a new token pair.
///
/// Validates the refresh token, then issues new access + refresh tokens.
pub async fn handle_refresh_token(
    jwt: web::Data<Arc<JwtConfig>>,
    body: web::Json<RefreshRequest>,
) -> Result<HttpResponse, ShaperailError> {
    let claims = jwt
        .decode(&body.refresh_token)
        .map_err(|_| ShaperailError::Unauthorized)?;

    if claims.token_type != "refresh" {
        return Err(ShaperailError::Unauthorized);
    }

    let access = jwt
        .encode_access(&claims.sub, &claims.role)
        .map_err(|e| ShaperailError::Internal(format!("Token encoding failed: {e}")))?;

    let refresh = jwt
        .encode_refresh(&claims.sub, &claims.role)
        .map_err(|e| ShaperailError::Internal(format!("Token encoding failed: {e}")))?;

    let pair = TokenPair {
        access_token: access,
        refresh_token: refresh,
        token_type: "Bearer".to_string(),
        expires_in: jwt.access_ttl.num_seconds(),
    };

    Ok(HttpResponse::Ok().json(pair))
}

/// Registers the auth token endpoints on the given Actix `ServiceConfig`.
pub fn register_auth_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/auth/token", web::post().to(handle_issue_token));
    cfg.route("/auth/refresh", web::post().to(handle_refresh_token));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_request_deserialize() {
        let json = r#"{"user_id": "u1", "role": "admin"}"#;
        let req: TokenRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.user_id, "u1");
        assert_eq!(req.role, "admin");
    }

    #[test]
    fn refresh_request_deserialize() {
        let json = r#"{"refresh_token": "some.token.here"}"#;
        let req: RefreshRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.refresh_token, "some.token.here");
    }

    #[test]
    fn token_pair_serialize() {
        let pair = TokenPair {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
        };
        let json = serde_json::to_value(&pair).unwrap();
        assert_eq!(json["token_type"], "Bearer");
        assert_eq!(json["expires_in"], 3600);
    }
}
