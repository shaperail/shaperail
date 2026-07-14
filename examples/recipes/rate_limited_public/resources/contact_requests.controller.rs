use shaperail_core::ShaperailError;
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

pub async fn capture_source_ip(ctx: &mut Context) -> ControllerResult {
    let source_ip = ctx
        .client_ip()
        .ok_or_else(|| ShaperailError::Internal("Client IP is unavailable".to_string()))?
        .to_string();
    ctx.input
        .insert("source_ip".to_string(), serde_json::json!(source_ip));
    Ok(())
}
