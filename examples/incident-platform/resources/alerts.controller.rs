use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

fn validation_error(field: &str, message: &str, code: &str) -> ShaperailError {
    ShaperailError::Validation(vec![FieldError {
        field: field.to_string(),
        message: message.to_string(),
        code: code.to_string(),
    }])
}

pub async fn ingest_alert(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;

    ctx.input.insert(
        "created_by".to_string(),
        serde_json::json!(user.sub.clone()),
    );

    let org_id = ctx
        .tenant_id
        .as_deref()
        .or_else(|| ctx.input.get("org_id").and_then(|value| value.as_str()))
        .ok_or_else(|| validation_error("org_id", "org_id is required", "required"))?;

    let service_id = ctx
        .input
        .get("service_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| validation_error("service_id", "service_id is required", "required"))?;

    let service_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM services WHERE id = $1::uuid AND org_id = $2::uuid AND deleted_at IS NULL",
    )
    .bind(service_id)
    .bind(org_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|error| ShaperailError::Internal(format!("Failed to verify service: {error}")))?;

    if service_exists == 0 {
        return Err(validation_error(
            "service_id",
            "service_id must reference an active service in the same organization",
            "service_not_found",
        ));
    }

    let fingerprint = ctx
        .input
        .get("fingerprint")
        .and_then(|value| value.as_str())
        .ok_or_else(|| validation_error("fingerprint", "fingerprint is required", "required"))?;

    let duplicate_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alerts
         WHERE org_id = $1::uuid
           AND fingerprint = $2
           AND created_at >= NOW() - INTERVAL '10 minutes'
           AND status != 'ignored'",
    )
    .bind(org_id)
    .bind(fingerprint)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|error| {
        ShaperailError::Internal(format!("Failed to check alert dedupe window: {error}"))
    })?;

    let status = if ctx
        .input
        .get("incident_id")
        .and_then(|value| value.as_str())
        .is_some()
    {
        "linked"
    } else if duplicate_count > 0 {
        "deduped"
    } else {
        "received"
    };

    ctx.input
        .insert("status".to_string(), serde_json::json!(status));

    Ok(())
}

pub async fn reconcile_alert_link(ctx: &mut Context) -> ControllerResult {
    let Some(status) = ctx.input.get("status").and_then(|value| value.as_str()) else {
        return Ok(());
    };

    if status == "linked" {
        let incident_id = ctx
            .input
            .get("incident_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                validation_error(
                    "incident_id",
                    "incident_id is required when status is linked",
                    "incident_required",
                )
            })?;

        let incident_exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM incidents WHERE id = $1::uuid AND deleted_at IS NULL",
        )
        .bind(incident_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|error| {
            ShaperailError::Internal(format!("Failed to verify incident link: {error}"))
        })?;

        if incident_exists == 0 {
            return Err(validation_error(
                "incident_id",
                "incident_id must reference an active incident",
                "incident_not_found",
            ));
        }
    }

    Ok(())
}
