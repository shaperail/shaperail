use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

fn validation_error(field: &str, message: &str, code: &str) -> ShaperailError {
    ShaperailError::Validation(vec![FieldError {
        field: field.to_string(),
        message: message.to_string(),
        code: code.to_string(),
    }])
}

fn slugify(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut previous_was_dash = false;

    for ch in value.chars() {
        let normalized = ch.to_ascii_lowercase();
        if normalized.is_ascii_alphanumeric() {
            slug.push(normalized);
            previous_was_dash = false;
        } else if !previous_was_dash {
            slug.push('-');
            previous_was_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

pub async fn open_incident(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;

    let created_by = ctx
        .input
        .get("created_by")
        .and_then(|value| value.as_str())
        .ok_or_else(|| validation_error("created_by", "created_by is required", "required"))?;

    if created_by != user.sub.as_str() {
        return Err(validation_error(
            "created_by",
            "created_by must match the authenticated user",
            "created_by_mismatch",
        ));
    }

    let service_id = ctx
        .input
        .get("service_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| validation_error("service_id", "service_id is required", "required"))?;

    let org_id = ctx
        .tenant_id
        .as_deref()
        .or_else(|| ctx.input.get("org_id").and_then(|value| value.as_str()))
        .ok_or_else(|| validation_error("org_id", "org_id is required", "required"))?;

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

    let slug_source = ctx
        .input
        .get("slug")
        .and_then(|value| value.as_str())
        .or_else(|| ctx.input.get("title").and_then(|value| value.as_str()))
        .ok_or_else(|| validation_error("slug", "slug is required", "required"))?;

    let normalized_slug = slugify(slug_source);
    if normalized_slug.is_empty() {
        return Err(validation_error(
            "slug",
            "slug must contain at least one alphanumeric character",
            "invalid_slug",
        ));
    }
    ctx.input.insert(
        "slug".to_string(),
        serde_json::json!(normalized_slug.clone()),
    );
    ctx.input.insert(
        "room_key".to_string(),
        serde_json::json!(format!("incident:{normalized_slug}")),
    );

    let severity = ctx
        .input
        .get("severity")
        .and_then(|value| value.as_str())
        .unwrap_or("sev3");

    if matches!(severity, "sev1" | "sev2")
        && ctx
            .input
            .get("commander_id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .is_empty()
    {
        return Err(validation_error(
            "commander_id",
            "sev1 and sev2 incidents require commander_id at creation time",
            "commander_required",
        ));
    }

    Ok(())
}

pub async fn enforce_incident_update(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;

    let Some(status) = ctx
        .input
        .get("status")
        .and_then(|value| value.as_str())
        .map(str::to_string)
    else {
        return Ok(());
    };

    if status == "acknowledged"
        && ctx
            .input
            .get("commander_id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .is_empty()
    {
        return Err(validation_error(
            "commander_id",
            "commander_id is required when acknowledging an incident",
            "commander_required",
        ));
    }

    if matches!(status.as_str(), "resolved" | "closed")
        && user.role.as_str() != "admin"
        && !ctx.headers.contains_key("x-resolution-reason")
    {
        return Err(validation_error(
            "status",
            "resolved and closed transitions require an admin role or X-Resolution-Reason",
            "resolution_reason_required",
        ));
    }

    if status == "acknowledged" {
        ctx.input.insert(
            "acknowledged_at".to_string(),
            serde_json::json!(chrono::Utc::now().to_rfc3339()),
        );
    }

    if matches!(status.as_str(), "resolved" | "closed") {
        ctx.input.insert(
            "resolved_at".to_string(),
            serde_json::json!(chrono::Utc::now().to_rfc3339()),
        );
    }

    Ok(())
}

pub async fn write_incident_audit(ctx: &mut Context) -> ControllerResult {
    let Some(data) = ctx.data.as_ref() else {
        return Ok(());
    };

    let incident_id = data
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let org_id = data
        .get("org_id")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let status = data
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let created_by = ctx
        .user
        .as_ref()
        .map(|user| user.sub.clone())
        .unwrap_or_else(|| "system".to_string());

    sqlx::query(
        "INSERT INTO incident_audit_log (id, incident_id, org_id, action, record_data, created_by, created_at)
         VALUES (gen_random_uuid(), $1::uuid, $2::uuid, $3, $4, $5, NOW())",
    )
    .bind(incident_id)
    .bind(org_id)
    .bind(format!("incident_{status}"))
    .bind(data)
    .bind(created_by)
    .execute(&ctx.pool)
    .await
    .map_err(|error| ShaperailError::Internal(format!("Failed to write incident audit log: {error}")))?;

    Ok(())
}
