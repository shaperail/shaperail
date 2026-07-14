use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

/// Before-controller for **create**: prepares a new post for insertion.
///
/// 1. Auto-fills `created_by` from the authenticated user's JWT.
/// 2. Generates a URL-safe `slug` from the title (lowercase, hyphens, no special chars).
/// 3. Defaults `status` to `"draft"` when the client omits it.
/// 4. Validates that `body` is not empty or whitespace-only.
pub async fn prepare_post(ctx: &mut Context) -> ControllerResult {
    // --- 1. Auto-fill created_by from JWT ---
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;

    ctx.input
        .insert("created_by".into(), serde_json::json!(user.sub));

    // --- 2. Generate slug from title ---
    let title = ctx
        .input
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-");

    if slug.is_empty() {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "title".into(),
            message: "Title must produce a non-empty slug".into(),
            code: "invalid_title".into(),
        }]));
    }

    ctx.input.insert("slug".into(), serde_json::json!(slug));

    // --- 3. Default status to "draft" ---
    if !ctx.input.contains_key("status") {
        ctx.input
            .insert("status".into(), serde_json::json!("draft"));
    }

    // --- 4. Validate body is not empty/whitespace ---
    let body = ctx.input.get("body").and_then(|v| v.as_str()).unwrap_or("");

    if body.trim().is_empty() {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "body".into(),
            message: "Post body cannot be empty".into(),
            code: "required".into(),
        }]));
    }

    Ok(())
}

/// Before-controller for **update**: enforces editing rules on existing posts.
///
/// 1. Only draft or published posts can be edited (not archived).
/// 2. Non-admin users cannot change `status` to `"published"`.
/// 3. Changing from published to draft requires an `X-Edit-Reason` header.
/// 4. Auto-updates `slug` when the title changes.
pub async fn enforce_edit_rules(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;

    let is_admin = user.role == "admin";

    // Fetch the current post from the database to check its status.
    let post_id = ctx
        .path_param("id")
        .ok_or_else(|| ShaperailError::Internal("Missing post ID in update context".into()))?;

    let row = sqlx::query_as::<_, (String,)>("SELECT status FROM posts WHERE id = $1::uuid")
        .bind(post_id)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error: {e}")))?
        .ok_or(ShaperailError::NotFound)?;

    let current_status = row.0.as_str();

    // --- 1. Block edits to archived posts ---
    if current_status == "archived" {
        return Err(ShaperailError::Forbidden);
    }

    // --- 2. Non-admins cannot publish ---
    if let Some(new_status) = ctx.input.get("status").and_then(|v| v.as_str()) {
        if new_status == "published" && !is_admin {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "status".into(),
                message: "Only admins can set status to published".into(),
                code: "forbidden_status".into(),
            }]));
        }

        // --- 3. Published -> draft requires a reason header ---
        if current_status == "published"
            && new_status == "draft"
            && !ctx.headers.contains_key("x-edit-reason")
        {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "status".into(),
                message: "Reverting a published post to draft requires an X-Edit-Reason header"
                    .into(),
                code: "reason_required".into(),
            }]));
        }
    }

    // --- 4. Auto-update slug when title changes ---
    if let Some(new_title) = ctx.input.get("title").and_then(|v| v.as_str()) {
        let slug: String = new_title
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' {
                    c
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join("-");

        if !slug.is_empty() {
            ctx.input.insert("slug".into(), serde_json::json!(slug));
        }
    }

    Ok(())
}

/// After-controller for **delete**: logs orphaned comments and sets a response header.
///
/// 1. Queries the count of comments belonging to the deleted post.
/// 2. Adds an `X-Comments-Archived` response header with the count.
pub async fn cleanup_comments(ctx: &mut Context) -> ControllerResult {
    let post_id = ctx
        .data
        .as_ref()
        .and_then(|d| d.get("id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShaperailError::Internal("Missing post ID in delete context".into()))?;

    let row = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM comments WHERE post_id = $1::uuid")
        .bind(post_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error: {e}")))?;

    let comment_count = row.0;

    tracing::info!(
        post_id = post_id,
        comment_count = comment_count,
        "Post deleted; archived associated comments"
    );

    ctx.response_headers
        .push(("X-Comments-Archived".into(), comment_count.to_string()));

    Ok(())
}
