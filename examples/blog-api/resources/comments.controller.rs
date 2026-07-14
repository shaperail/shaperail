use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

/// Before-controller for **create**: validates a new comment before insertion.
///
/// 1. Checks that the referenced post exists and is published (not draft/archived).
/// 2. Auto-fills `created_by` from the JWT if the user is authenticated.
/// 3. Strips HTML tags from `body` as basic XSS prevention.
/// 4. Rate-limits to 10 comments per user per hour via a DB query.
pub async fn validate_comment(ctx: &mut Context) -> ControllerResult {
    let post_id = ctx
        .input
        .get("post_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ShaperailError::Validation(vec![FieldError {
                field: "post_id".into(),
                message: "post_id is required".into(),
                code: "required".into(),
            }])
        })?
        .to_owned();

    // --- 1. Verify the referenced post exists and is published ---
    let row = sqlx::query_as::<_, (String,)>("SELECT status FROM posts WHERE id = $1::uuid")
        .bind(&post_id)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error: {e}")))?;

    match row {
        None => {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "post_id".into(),
                message: "Referenced post does not exist".into(),
                code: "invalid_reference".into(),
            }]));
        }
        Some((status,)) if status != "published" => {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "post_id".into(),
                message: format!(
                    "Cannot comment on a {status} post; only published posts accept comments"
                ),
                code: "post_not_published".into(),
            }]));
        }
        _ => {}
    }

    // --- 2. Auto-fill created_by from JWT ---
    let subject = ctx
        .user
        .as_ref()
        .ok_or(ShaperailError::Unauthorized)?
        .sub
        .clone();
    ctx.input
        .insert("created_by".into(), serde_json::json!(&subject));

    // --- 3. Strip HTML tags from body (basic XSS prevention) ---
    if let Some(body) = ctx.input.get("body").and_then(|v| v.as_str()) {
        let stripped = strip_html_tags(body);
        if stripped.trim().is_empty() {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "body".into(),
                message: "Comment body cannot be empty after removing HTML".into(),
                code: "required".into(),
            }]));
        }
        ctx.input.insert("body".into(), serde_json::json!(stripped));
    }

    // --- 4. Rate limit: max 10 comments per user per hour ---
    let row = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM comments WHERE created_by = $1 AND created_at > NOW() - INTERVAL '1 hour'",
    )
    .bind(&subject)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(format!("DB error: {e}")))?;

    if row.0 >= 10 {
        return Err(ShaperailError::RateLimited);
    }

    Ok(())
}

/// Before-controller for **update**: checks ownership and edit window.
///
/// 1. Verifies the user owns the comment OR has the admin role.
/// 2. Disallows editing comments older than 15 minutes (except for admins).
pub async fn check_comment_ownership(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;

    let is_admin = user.role == "admin";

    let comment_id = ctx
        .path_param("id")
        .ok_or_else(|| ShaperailError::Internal("Missing comment ID in update context".into()))?;

    let row = sqlx::query_as::<_, (String, chrono::DateTime<chrono::Utc>)>(
        "SELECT created_by, created_at FROM comments WHERE id = $1::uuid",
    )
    .bind(comment_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(format!("DB error: {e}")))?
    .ok_or(ShaperailError::NotFound)?;

    let (owner_id, created_at) = row;

    // --- 1. Ownership check ---
    if owner_id != user.sub && !is_admin {
        return Err(ShaperailError::Forbidden);
    }

    // --- 2. 15-minute edit window (admins exempt) ---
    if !is_admin {
        let now = chrono::Utc::now();
        let age = now - created_at;
        if age > chrono::Duration::minutes(15) {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "id".into(),
                message: "Comments can only be edited within 15 minutes of creation".into(),
                code: "edit_window_expired".into(),
            }]));
        }
    }

    Ok(())
}

/// Strips HTML tags from a string using a simple state machine.
/// This is a basic defense; production apps should use a dedicated sanitizer.
fn strip_html_tags(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut inside_tag = false;

    for ch in input.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => result.push(ch),
            _ => {}
        }
    }

    result
}
