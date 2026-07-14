use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

/// Called before create — enforce project limits and validate within tenant scope.
///
/// - Check project limit per org based on plan (free=3, pro=20, enterprise=unlimited)
/// - Validate project name is unique within the org
/// - Auto-fill created_by from JWT
pub async fn validate_project(ctx: &mut Context) -> ControllerResult {
    let tenant_id = ctx
        .tenant_id
        .as_deref()
        .ok_or(ShaperailError::Unauthorized)?;

    // Check project limit per org based on plan
    let org: (String,) = sqlx::query_as("SELECT plan::text FROM organizations WHERE id = $1::uuid")
        .bind(tenant_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error fetching org plan: {e}")))?;

    let plan = org.0.as_str();
    let max_projects: Option<i64> = match plan {
        "free" => Some(3),
        "pro" => Some(20),
        "enterprise" => None, // unlimited
        _ => Some(3),
    };

    if let Some(limit) = max_projects {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM projects WHERE org_id = $1::uuid AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error counting projects: {e}")))?;

        if count.0 >= limit {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "org_id".into(),
                message: format!(
                    "Project limit reached for {plan} plan ({limit} projects). \
                     Upgrade your plan to create more projects."
                ),
                code: "limit_exceeded".into(),
            }]));
        }
    }

    // Validate project name is unique within the org
    if let Some(name) = ctx.input.get("name").and_then(|v| v.as_str()) {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(\
                SELECT 1 FROM projects \
                WHERE org_id = $1::uuid AND LOWER(name) = LOWER($2) AND deleted_at IS NULL\
            )",
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error checking project name: {e}")))?;

        if exists.0 {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "name".into(),
                message: "A project with this name already exists in your organization".into(),
                code: "unique".into(),
            }]));
        }
    }

    // Auto-fill created_by from JWT
    if let Some(user) = &ctx.user {
        ctx.input
            .insert("created_by".to_string(), serde_json::json!(&user.sub));
    } else {
        return Err(ShaperailError::Unauthorized);
    }

    Ok(())
}

/// Called before update — enforce project status transition rules.
///
/// - Only admins can change status to "archived"
/// - Cannot reopen a completed project (must create new one)
/// - When archiving, check no tasks are in "in_progress" status
pub async fn enforce_project_status(ctx: &mut Context) -> ControllerResult {
    let new_status = match ctx.input.get("status").and_then(|v| v.as_str()) {
        Some(status) => status.to_string(),
        None => return Ok(()), // No status change requested
    };

    let project_id = ctx
        .path_param("id")
        .ok_or_else(|| ShaperailError::Internal("Missing project ID".into()))?;

    // Fetch current project status
    let current: (String,) =
        sqlx::query_as("SELECT status::text FROM projects WHERE id = $1::uuid")
            .bind(project_id)
            .fetch_one(&ctx.pool)
            .await
            .map_err(|e| {
                ShaperailError::Internal(format!("DB error fetching project status: {e}"))
            })?;

    let current_status = current.0.as_str();

    // Cannot reopen an archived project
    if current_status == "archived" && new_status == "active" {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "status".into(),
            message: "Cannot reopen an archived project. Create a new project instead.".into(),
            code: "invalid_transition".into(),
        }]));
    }

    // Only admins can archive a project
    if new_status == "archived" {
        let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
        if user.role != "admin" && user.role != "super_admin" {
            return Err(ShaperailError::Forbidden);
        }

        // When archiving, check no tasks are in "in_progress" status
        let in_progress: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM tasks \
             WHERE project_id = $1::uuid AND status = 'in_progress' AND deleted_at IS NULL",
        )
        .bind(project_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| {
            ShaperailError::Internal(format!("DB error checking in-progress tasks: {e}"))
        })?;

        if in_progress.0 > 0 {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "status".into(),
                message: format!(
                    "Cannot archive project with {} in-progress task(s). \
                     Complete or reassign them first.",
                    in_progress.0
                ),
                code: "has_in_progress_tasks".into(),
            }]));
        }
    }

    Ok(())
}
