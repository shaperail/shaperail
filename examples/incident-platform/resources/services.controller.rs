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

pub async fn prepare_service(ctx: &mut Context) -> ControllerResult {
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

    let slug_source = ctx
        .input
        .get("slug")
        .and_then(|value| value.as_str())
        .or_else(|| ctx.input.get("name").and_then(|value| value.as_str()))
        .ok_or_else(|| validation_error("slug", "slug is required", "required"))?;

    let normalized_slug = slugify(slug_source);
    if normalized_slug.is_empty() {
        return Err(validation_error(
            "slug",
            "slug must contain at least one alphanumeric character",
            "invalid_slug",
        ));
    }
    ctx.input
        .insert("slug".to_string(), serde_json::json!(normalized_slug));

    let tier = ctx
        .input
        .get("tier")
        .and_then(|value| value.as_str())
        .unwrap_or("standard");

    if tier == "critical"
        && ctx
            .input
            .get("runbook_url")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .is_empty()
    {
        return Err(validation_error(
            "runbook_url",
            "critical services must include a runbook_url",
            "runbook_required",
        ));
    }

    Ok(())
}
