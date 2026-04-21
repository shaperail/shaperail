use actix_web::{web, HttpRequest, HttpResponse};
use shaperail_core::{InboundWebhookConfig, ShaperailError};

use super::webhook::verify_hmac_signature;
use crate::events::EventEmitter;

/// State for inbound webhook endpoints.
pub struct InboundWebhookState {
    /// The secret used for signature verification.
    pub secret: String,
    /// Accepted event names (empty = accept all).
    pub accepted_events: Vec<String>,
    /// Event emitter for re-emitting received events.
    pub emitter: EventEmitter,
    /// Which HTTP header carries the HMAC signature for this endpoint.
    /// Stored as metadata; runtime signature detection in `verify_signature` handles
    /// all known provider headers automatically.
    #[allow(dead_code)]
    pub signature_header: String,
}

/// Configures Actix-web routes for inbound webhook endpoints.
pub fn configure_inbound_routes(
    cfg: &mut web::ServiceConfig,
    configs: &[InboundWebhookConfig],
    emitter: &EventEmitter,
) {
    for config in configs {
        let secret = std::env::var(&config.secret_env).unwrap_or_default();
        let state = web::Data::new(InboundWebhookState {
            secret,
            accepted_events: config.events.clone(),
            emitter: emitter.clone(),
            signature_header: config.signature_header.clone(),
        });

        let path = config.path.clone();
        cfg.service(
            web::resource(&path)
                .app_data(state)
                .route(web::post().to(handle_inbound_webhook)),
        );
    }
}

/// Verifies the signature from an inbound webhook request.
///
/// Supports three signature header formats:
/// - `X-Shaperail-Signature: sha256=<hex>` (Shaperail format)
/// - `X-Hub-Signature-256: sha256=<hex>` (GitHub format)
/// - `Stripe-Signature: t=<ts>,v1=<sig>` (Stripe format)
pub fn verify_signature(
    req: &HttpRequest,
    body: &[u8],
    secret: &str,
) -> Result<(), ShaperailError> {
    // Try Shaperail signature header
    if let Some(sig_header) = req.headers().get("X-Shaperail-Signature") {
        let sig_str = sig_header
            .to_str()
            .map_err(|_| ShaperailError::Unauthorized)?;
        let hex_sig = sig_str
            .strip_prefix("sha256=")
            .ok_or(ShaperailError::Unauthorized)?;
        if verify_hmac_signature(body, secret.as_bytes(), hex_sig) {
            return Ok(());
        }
        return Err(ShaperailError::Unauthorized);
    }

    // Try GitHub signature header
    if let Some(sig_header) = req.headers().get("X-Hub-Signature-256") {
        let sig_str = sig_header
            .to_str()
            .map_err(|_| ShaperailError::Unauthorized)?;
        let hex_sig = sig_str
            .strip_prefix("sha256=")
            .ok_or(ShaperailError::Unauthorized)?;
        if verify_hmac_signature(body, secret.as_bytes(), hex_sig) {
            return Ok(());
        }
        return Err(ShaperailError::Unauthorized);
    }

    // Try Stripe signature header (simplified: extract v1= portion)
    if let Some(sig_header) = req.headers().get("Stripe-Signature") {
        let sig_str = sig_header
            .to_str()
            .map_err(|_| ShaperailError::Unauthorized)?;
        // Stripe format: t=<timestamp>,v1=<signature>
        let v1_sig = sig_str
            .split(',')
            .find_map(|part: &str| part.strip_prefix("v1="))
            .ok_or(ShaperailError::Unauthorized)?;

        // Stripe signs: timestamp.body
        let timestamp = sig_str
            .split(',')
            .find_map(|part: &str| part.strip_prefix("t="))
            .unwrap_or("");
        let signed_payload = format!("{timestamp}.{}", String::from_utf8_lossy(body));

        if verify_hmac_signature(signed_payload.as_bytes(), secret.as_bytes(), v1_sig) {
            return Ok(());
        }
        return Err(ShaperailError::Unauthorized);
    }

    Err(ShaperailError::Unauthorized)
}

/// Handler for inbound webhook POST requests.
async fn handle_inbound_webhook(
    req: HttpRequest,
    state: web::Data<InboundWebhookState>,
    body: web::Bytes,
) -> Result<HttpResponse, ShaperailError> {
    // Verify signature
    verify_signature(&req, &body, &state.secret)?;

    // Parse the JSON body
    let payload: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
        ShaperailError::Validation(vec![shaperail_core::FieldError {
            field: "body".to_string(),
            message: format!("Invalid JSON: {e}"),
            code: "invalid_json".to_string(),
        }])
    })?;

    // Extract event type from common patterns
    let event_type = payload
        .get("type")
        .or_else(|| payload.get("event"))
        .or_else(|| payload.get("action"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Check if this event is accepted
    if !state.accepted_events.is_empty() && !state.accepted_events.iter().any(|e| e == &event_type)
    {
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "accepted": false,
            "reason": "event type not in accepted list"
        })));
    }

    // Re-emit the event internally
    let event_name = format!("inbound.{event_type}");
    let _ = state
        .emitter
        .emit(&event_name, "webhook", &event_type, payload)
        .await;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "accepted": true
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_signature_hmac_round_trip() {
        let body = b"test payload";
        let secret = "test-secret";
        let sig = super::super::webhook::compute_hmac_signature(body, secret.as_bytes());
        assert!(verify_hmac_signature(body, secret.as_bytes(), &sig));
    }
}
