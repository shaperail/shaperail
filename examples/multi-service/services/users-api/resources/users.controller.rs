use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};
use tracing::info;

/// Blocked email domains that cannot be used for registration.
const BLOCKED_DOMAINS: &[&str] = &["tempmail.com", "throwaway.io", "fakeinbox.net"];

/// Before-create: normalize email, validate its domain, and set the default role.
pub async fn prepare_user(ctx: &mut Context) -> ControllerResult {
    // Normalize email to lowercase.
    let email = ctx
        .input
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ShaperailError::Validation(vec![FieldError {
                field: "email".to_string(),
                message: "Email is required".to_string(),
                code: "required".to_string(),
            }])
        })?
        .to_lowercase();

    // Validate email domain is not in the blocked list.
    let domain = email.rsplit('@').next().unwrap_or("");
    if BLOCKED_DOMAINS.contains(&domain) {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "email".to_string(),
            message: format!("Email domain '{domain}' is not allowed"),
            code: "blocked_domain".to_string(),
        }]));
    }
    ctx.input
        .insert("email".to_string(), serde_json::json!(email));

    // Set default role to "member" if not explicitly provided.
    if !ctx.input.contains_key("role") {
        ctx.input
            .insert("role".to_string(), serde_json::json!("member"));
    }

    Ok(())
}

/// After-create: log user creation and add X-User-Created response header.
pub async fn provision_defaults(ctx: &mut Context) -> ControllerResult {
    let user_id = ctx
        .data
        .as_ref()
        .and_then(|d| d.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let request_id = ctx.headers.get("x-request-id").cloned().unwrap_or_default();

    info!(
        user_id = user_id,
        request_id = request_id.as_str(),
        "New user created"
    );

    ctx.response_headers
        .push(("X-User-Created".to_string(), user_id.to_string()));

    Ok(())
}

/// Before-update: enforce role-change restrictions.
///
/// Rules:
/// - Non-admins cannot change their own role.
/// - Cannot deactivate the last admin in the system.
pub async fn validate_user_update(ctx: &mut Context) -> ControllerResult {
    let caller = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;

    // If the request is changing the role field, apply restrictions.
    if let Some(new_role) = ctx.input.get("role").and_then(|v| v.as_str()) {
        // Non-admins cannot change their own role.
        if caller.role != "admin" {
            return Err(ShaperailError::Forbidden);
        }

        // Cannot demote the last admin. Check how many admins exist.
        if new_role != "admin" {
            let user_id = ctx
                .path_param("id")
                .ok_or_else(|| ShaperailError::Internal("Missing user ID".into()))?;
            let current_role: (String,) =
                sqlx::query_as("SELECT role::text FROM users WHERE id = $1::uuid")
                    .bind(user_id)
                    .fetch_one(&ctx.pool)
                    .await?;

            if current_role.0 == "admin" {
                let admin_count: (i64,) =
                    sqlx::query_as("SELECT COUNT(*) FROM users WHERE role = 'admin'")
                        .fetch_one(&ctx.pool)
                        .await
                        .map_err(|e| ShaperailError::Internal(e.to_string()))?;

                if admin_count.0 <= 1 {
                    return Err(ShaperailError::Validation(vec![FieldError {
                        field: "role".to_string(),
                        message: "Cannot remove the last admin from the system".to_string(),
                        code: "last_admin".to_string(),
                    }]));
                }
            }
        }
    }

    Ok(())
}
