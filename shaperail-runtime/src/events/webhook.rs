use std::time::Instant;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use shaperail_core::ShaperailError;

/// Outbound webhook dispatcher.
///
/// Delivers event payloads to external URLs with HMAC-SHA256 signature headers.
/// Signature format: `X-Shaperail-Signature: sha256=<hex-digest>`
#[derive(Clone)]
pub struct WebhookDispatcher {
    secret: String,
    timeout_secs: u64,
}

/// Result of a webhook delivery attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryResult {
    /// HTTP status code (0 if connection failed).
    pub status_code: u16,
    /// Whether delivery was successful (2xx status).
    pub success: bool,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// Error message if delivery failed.
    pub error: Option<String>,
}

impl WebhookDispatcher {
    /// Creates a new webhook dispatcher.
    pub fn new(secret: String, timeout_secs: u64) -> Self {
        Self {
            secret,
            timeout_secs,
        }
    }

    /// Creates a dispatcher from environment configuration.
    ///
    /// Reads the secret from the specified env var (defaults to WEBHOOK_SECRET).
    pub fn from_env(secret_env: &str, timeout_secs: u64) -> Result<Self, ShaperailError> {
        let secret = std::env::var(secret_env).map_err(|_| {
            ShaperailError::Internal(format!("Webhook secret env var '{secret_env}' not set"))
        })?;
        Ok(Self::new(secret, timeout_secs))
    }

    /// Computes the HMAC-SHA256 signature for a payload.
    pub fn sign(&self, body: &[u8]) -> String {
        compute_hmac_signature(body, self.secret.as_bytes())
    }

    /// Returns the configured timeout in seconds.
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }

    /// Delivers a webhook payload to the given URL.
    ///
    /// This is a "fire" method that performs the actual HTTP POST.
    /// In production, this is called from a job handler with retry support.
    ///
    /// Note: This uses a minimal HTTP client built on `actix_web::client` or
    /// raw TCP. For the milestone we track timing and return delivery results
    /// without making actual HTTP calls — the job handler integration handles
    /// the real delivery path.
    pub fn build_delivery_request(
        &self,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<WebhookRequest, ShaperailError> {
        let body = serde_json::to_vec(payload).map_err(|e| {
            ShaperailError::Internal(format!("Failed to serialize webhook body: {e}"))
        })?;
        let signature = self.sign(&body);

        Ok(WebhookRequest {
            url: url.to_string(),
            body,
            signature,
            timeout_secs: self.timeout_secs,
        })
    }
}

/// A prepared webhook request ready for delivery.
#[derive(Debug, Clone)]
pub struct WebhookRequest {
    /// Target URL.
    pub url: String,
    /// JSON body bytes.
    pub body: Vec<u8>,
    /// HMAC-SHA256 signature (hex-encoded).
    pub signature: String,
    /// HTTP timeout in seconds.
    pub timeout_secs: u64,
}

impl WebhookRequest {
    /// Returns the `X-Shaperail-Signature` header value.
    pub fn signature_header(&self) -> String {
        format!("sha256={}", self.signature)
    }

    /// Simulates delivery and returns a result (for testing without actual HTTP).
    ///
    /// In production, the job handler uses an HTTP client to actually POST.
    pub fn simulate_delivery(&self, status_code: u16) -> DeliveryResult {
        let start = Instant::now();
        let latency_ms = start.elapsed().as_millis() as u64;

        DeliveryResult {
            status_code,
            success: (200..300).contains(&status_code),
            latency_ms,
            error: if (200..300).contains(&status_code) {
                None
            } else {
                Some(format!("HTTP {status_code}"))
            },
        }
    }
}

/// Computes an HMAC-SHA256 signature over a body using the given secret.
///
/// Returns the hex-encoded digest.
pub fn compute_hmac_signature(body: &[u8], secret: &[u8]) -> String {
    // HMAC-SHA256 accepts keys of any size, so this never fails in practice.
    // We handle the error branch defensively to satisfy the no-unwrap rule.
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else {
        return String::new();
    };
    mac.update(body);
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

/// Verifies an HMAC-SHA256 signature.
///
/// `signature` should be the hex-encoded digest (without "sha256=" prefix).
pub fn verify_hmac_signature(body: &[u8], secret: &[u8], signature: &str) -> bool {
    let expected = compute_hmac_signature(body, secret);
    // Constant-time comparison to prevent timing attacks
    constant_time_eq(expected.as_bytes(), signature.as_bytes())
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let secret = b"test-secret";
        let body = b"hello world";
        let sig = compute_hmac_signature(body, secret);
        assert!(verify_hmac_signature(body, secret, &sig));
    }

    #[test]
    fn wrong_secret_fails() {
        let body = b"hello world";
        let sig = compute_hmac_signature(body, b"correct-secret");
        assert!(!verify_hmac_signature(body, b"wrong-secret", &sig));
    }

    #[test]
    fn tampered_body_fails() {
        let secret = b"test-secret";
        let sig = compute_hmac_signature(b"original", secret);
        assert!(!verify_hmac_signature(b"tampered", secret, &sig));
    }

    #[test]
    fn webhook_request_signature_header() {
        let dispatcher = WebhookDispatcher::new("my-secret".to_string(), 30);
        let payload = serde_json::json!({"event": "test"});
        let req = dispatcher
            .build_delivery_request("https://example.com/hook", &payload)
            .unwrap();
        assert!(req.signature_header().starts_with("sha256="));
    }

    #[test]
    fn delivery_result_success() {
        let dispatcher = WebhookDispatcher::new("secret".to_string(), 30);
        let payload = serde_json::json!({"event": "test"});
        let req = dispatcher
            .build_delivery_request("https://example.com", &payload)
            .unwrap();
        let result = req.simulate_delivery(200);
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[test]
    fn delivery_result_failure() {
        let dispatcher = WebhookDispatcher::new("secret".to_string(), 30);
        let payload = serde_json::json!({"event": "test"});
        let req = dispatcher
            .build_delivery_request("https://example.com", &payload)
            .unwrap();
        let result = req.simulate_delivery(500);
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("HTTP 500"));
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
    }

    #[test]
    fn dispatcher_sign_deterministic() {
        let dispatcher = WebhookDispatcher::new("secret".to_string(), 30);
        let body = b"test body";
        let sig1 = dispatcher.sign(body);
        let sig2 = dispatcher.sign(body);
        assert_eq!(sig1, sig2);
    }
}
