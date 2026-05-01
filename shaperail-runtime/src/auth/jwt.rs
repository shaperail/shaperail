use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// JWT claims stored in every Shaperail-issued access or refresh token.
///
/// # Required claims
///
/// - `sub`: subject — the user ID, typically a UUID string.
/// - `role`: must match a role declared in the resource YAML's `auth:` lists
///   (or be `super_admin` for unrestricted access).
/// - `iat` / `exp`: issued-at and expiration, both unix seconds.
/// - `token_type`: `"access"` or `"refresh"`. Only `"access"` tokens authorize
///   protected requests; `"refresh"` is valid only against the refresh endpoint.
///
/// # Optional claims
///
/// - `tenant_id`: required for non-`super_admin` roles to access tenant-scoped
///   resources (M18). Missing or null on a tenant-scoped request → 401.
///
/// # Minting tokens for tests
///
/// Use [`JwtConfig::encode_access_with_tenant`]:
///
/// ```rust,no_run
/// use shaperail_runtime::auth::JwtConfig;
///
/// let config = JwtConfig::new("test-secret-at-least-32-bytes-long!", 3600, 86400);
/// let token = config
///     .encode_access_with_tenant("user-123", "admin", Some("org-abc"))
///     .unwrap();
/// // Send as `Authorization: Bearer {token}`.
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the user ID.
    pub sub: String,
    /// User role.
    pub role: String,
    /// Issued at (unix timestamp).
    pub iat: i64,
    /// Expiration (unix timestamp).
    pub exp: i64,
    /// Token type: "access" or "refresh".
    #[serde(default = "default_token_type")]
    pub token_type: String,
    /// Tenant ID (M18). Optional — present when multi-tenancy is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

fn default_token_type() -> String {
    "access".to_string()
}

/// Configuration for JWT signing and validation.
#[derive(Debug, Clone)]
pub struct JwtConfig {
    /// The secret key bytes used for HMAC-SHA256.
    secret: Vec<u8>,
    /// Access token lifetime.
    pub access_ttl: Duration,
    /// Refresh token lifetime.
    pub refresh_ttl: Duration,
}

impl JwtConfig {
    /// Creates a new JwtConfig from a secret string.
    ///
    /// `access_ttl_secs` — lifetime for access tokens in seconds.
    /// `refresh_ttl_secs` — lifetime for refresh tokens in seconds.
    pub fn new(secret: &str, access_ttl_secs: i64, refresh_ttl_secs: i64) -> Self {
        Self {
            secret: secret.as_bytes().to_vec(),
            access_ttl: Duration::seconds(access_ttl_secs),
            refresh_ttl: Duration::seconds(refresh_ttl_secs),
        }
    }

    /// Creates a JwtConfig from the `JWT_SECRET` environment variable.
    ///
    /// Returns `None` if the variable is not set or is empty.
    pub fn from_env() -> Option<Self> {
        let secret = std::env::var("JWT_SECRET").ok()?;
        if secret.is_empty() {
            return None;
        }
        // Default: 24h access, 30d refresh
        Some(Self::new(&secret, 86400, 2_592_000))
    }

    /// Encodes an access token for the given user ID and role.
    pub fn encode_access(
        &self,
        user_id: &str,
        role: &str,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        self.encode_access_with_tenant(user_id, role, None)
    }

    /// Encodes an access token with an optional tenant_id claim (M18).
    pub fn encode_access_with_tenant(
        &self,
        user_id: &str,
        role: &str,
        tenant_id: Option<&str>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            role: role.to_string(),
            iat: now.timestamp(),
            exp: (now + self.access_ttl).timestamp(),
            token_type: "access".to_string(),
            tenant_id: tenant_id.map(ToString::to_string),
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.secret),
        )
    }

    /// Encodes a refresh token for the given user ID and role.
    pub fn encode_refresh(
        &self,
        user_id: &str,
        role: &str,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            role: role.to_string(),
            iat: now.timestamp(),
            exp: (now + self.refresh_ttl).timestamp(),
            token_type: "refresh".to_string(),
            tenant_id: None,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.secret),
        )
    }

    /// Decodes and validates a JWT token, returning the claims.
    pub fn decode(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.secret),
            &Validation::default(),
        )?;
        Ok(data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> JwtConfig {
        JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400)
    }

    #[test]
    fn encode_decode_access_token() {
        let cfg = test_config();
        let token = cfg.encode_access("user-123", "admin").unwrap();
        let claims = cfg.decode(&token).unwrap();
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.role, "admin");
        assert_eq!(claims.token_type, "access");
    }

    #[test]
    fn encode_decode_refresh_token() {
        let cfg = test_config();
        let token = cfg.encode_refresh("user-456", "member").unwrap();
        let claims = cfg.decode(&token).unwrap();
        assert_eq!(claims.sub, "user-456");
        assert_eq!(claims.role, "member");
        assert_eq!(claims.token_type, "refresh");
    }

    #[test]
    fn invalid_token_fails() {
        let cfg = test_config();
        let result = cfg.decode("garbage.token.here");
        assert!(result.is_err());
    }

    #[test]
    fn wrong_secret_fails() {
        let cfg1 = test_config();
        let cfg2 = JwtConfig::new("different-secret-key-also-long-enough!", 3600, 86400);
        let token = cfg1.encode_access("user-123", "admin").unwrap();
        let result = cfg2.decode(&token);
        assert!(result.is_err());
    }

    #[test]
    fn expired_token_fails() {
        let cfg = JwtConfig::new("test-secret-key-at-least-32-bytes-long!", -120, -120);
        let token = cfg.encode_access("user-123", "admin").unwrap();
        let result = cfg.decode(&token);
        assert!(result.is_err());
    }

    #[test]
    fn encode_access_with_tenant_id() {
        let cfg = test_config();
        let token = cfg
            .encode_access_with_tenant("user-123", "admin", Some("org-abc"))
            .unwrap();
        let claims = cfg.decode(&token).unwrap();
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.role, "admin");
        assert_eq!(claims.tenant_id.as_deref(), Some("org-abc"));
    }

    #[test]
    fn encode_access_without_tenant_id() {
        let cfg = test_config();
        let token = cfg.encode_access("user-123", "admin").unwrap();
        let claims = cfg.decode(&token).unwrap();
        assert!(claims.tenant_id.is_none());
    }
}
