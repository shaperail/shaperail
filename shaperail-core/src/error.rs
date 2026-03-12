use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use serde::{Deserialize, Serialize};

/// A single field-level validation error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldError {
    /// The field name that failed validation.
    pub field: String,
    /// Human-readable error message.
    pub message: String,
    /// Machine-readable error code (e.g., "required", "too_short").
    pub code: String,
}

/// Unified error type used across all Shaperail crates.
///
/// Maps to the PRD error response format:
/// ```json
/// {
///   "error": {
///     "code": "NOT_FOUND",
///     "status": 404,
///     "message": "Resource not found",
///     "request_id": "abc-123",
///     "details": null
///   }
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum ShaperailError {
    /// 404 — Resource not found.
    #[error("Resource not found")]
    NotFound,

    /// 401 — Missing or invalid authentication.
    #[error("Unauthorized")]
    Unauthorized,

    /// 403 — Authenticated but insufficient permissions.
    #[error("Forbidden")]
    Forbidden,

    /// 422 — One or more fields failed validation.
    #[error("Validation failed")]
    Validation(Vec<FieldError>),

    /// 409 — Conflict (e.g., unique constraint violation).
    #[error("Conflict: {0}")]
    Conflict(String),

    /// 429 — Rate limit exceeded.
    #[error("Rate limit exceeded")]
    RateLimited,

    /// 500 — Internal server error.
    #[error("Internal server error: {0}")]
    Internal(String),
}

impl ShaperailError {
    /// Returns the machine-readable error code string.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "NOT_FOUND",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::Conflict(_) => "CONFLICT",
            Self::RateLimited => "RATE_LIMITED",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    /// Returns the HTTP status code for this error.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Builds the PRD-mandated JSON error response body.
    ///
    /// The `request_id` is passed in by the caller (typically from middleware).
    pub fn to_error_body(&self, request_id: &str) -> serde_json::Value {
        let details = match self {
            Self::Validation(errors) => Some(serde_json::to_value(errors).unwrap_or_default()),
            _ => None,
        };

        serde_json::json!({
            "error": {
                "code": self.code(),
                "status": self.status().as_u16(),
                "message": self.to_string(),
                "request_id": request_id,
                "details": details,
            }
        })
    }
}

impl actix_web::ResponseError for ShaperailError {
    fn status_code(&self) -> StatusCode {
        self.status()
    }

    fn error_response(&self) -> HttpResponse {
        let body = self.to_error_body("unknown");
        HttpResponse::build(self.status()).json(body)
    }
}

impl From<sqlx::Error> for ShaperailError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => Self::NotFound,
            sqlx::Error::Database(db_err) => {
                // PostgreSQL unique violation code
                if db_err.code().as_deref() == Some("23505") {
                    Self::Conflict(db_err.message().to_string())
                } else {
                    Self::Internal(err.to_string())
                }
            }
            _ => Self::Internal(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes() {
        assert_eq!(ShaperailError::NotFound.code(), "NOT_FOUND");
        assert_eq!(ShaperailError::Unauthorized.code(), "UNAUTHORIZED");
        assert_eq!(ShaperailError::Forbidden.code(), "FORBIDDEN");
        assert_eq!(
            ShaperailError::Validation(vec![]).code(),
            "VALIDATION_ERROR"
        );
        assert_eq!(
            ShaperailError::Conflict("dup".to_string()).code(),
            "CONFLICT"
        );
        assert_eq!(ShaperailError::RateLimited.code(), "RATE_LIMITED");
        assert_eq!(
            ShaperailError::Internal("oops".to_string()).code(),
            "INTERNAL_ERROR"
        );
    }

    #[test]
    fn error_status_codes() {
        assert_eq!(ShaperailError::NotFound.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            ShaperailError::Unauthorized.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(ShaperailError::Forbidden.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            ShaperailError::Validation(vec![]).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            ShaperailError::Conflict("x".to_string()).status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            ShaperailError::RateLimited.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            ShaperailError::Internal("x".to_string()).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn error_display() {
        assert_eq!(ShaperailError::NotFound.to_string(), "Resource not found");
        assert_eq!(ShaperailError::Unauthorized.to_string(), "Unauthorized");
        assert_eq!(ShaperailError::Forbidden.to_string(), "Forbidden");
        assert_eq!(
            ShaperailError::Validation(vec![]).to_string(),
            "Validation failed"
        );
        assert_eq!(
            ShaperailError::Conflict("duplicate email".to_string()).to_string(),
            "Conflict: duplicate email"
        );
        assert_eq!(
            ShaperailError::RateLimited.to_string(),
            "Rate limit exceeded"
        );
        assert_eq!(
            ShaperailError::Internal("db down".to_string()).to_string(),
            "Internal server error: db down"
        );
    }

    #[test]
    fn error_body_matches_prd_shape() {
        let body = ShaperailError::NotFound.to_error_body("req-123");
        let error = &body["error"];
        assert_eq!(error["code"], "NOT_FOUND");
        assert_eq!(error["status"], 404);
        assert_eq!(error["message"], "Resource not found");
        assert_eq!(error["request_id"], "req-123");
        assert!(error["details"].is_null());
    }

    #[test]
    fn error_body_validation_includes_details() {
        let errors = vec![
            FieldError {
                field: "email".to_string(),
                message: "is required".to_string(),
                code: "required".to_string(),
            },
            FieldError {
                field: "name".to_string(),
                message: "too short".to_string(),
                code: "too_short".to_string(),
            },
        ];
        let body = ShaperailError::Validation(errors).to_error_body("req-456");
        let details = &body["error"]["details"];
        assert!(details.is_array());
        assert_eq!(details.as_array().unwrap().len(), 2);
        assert_eq!(details[0]["field"], "email");
    }

    #[test]
    fn field_error_serde() {
        let fe = FieldError {
            field: "email".to_string(),
            message: "is required".to_string(),
            code: "required".to_string(),
        };
        let json = serde_json::to_string(&fe).unwrap();
        let back: FieldError = serde_json::from_str(&json).unwrap();
        assert_eq!(fe, back);
    }

    #[test]
    fn from_sqlx_row_not_found() {
        let err: ShaperailError = sqlx::Error::RowNotFound.into();
        assert!(matches!(err, ShaperailError::NotFound));
    }
}
