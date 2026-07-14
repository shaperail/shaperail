use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

/// Called before create — validate task against project state and org membership.
///
/// - Verify the project exists and is active (not archived)
/// - If assigned_to is set, verify that user belongs to the same org
/// - Auto-fill created_by from JWT
/// - Default priority to "medium" if not set
pub async fn validate_task(ctx: &mut Context) -> ControllerResult {
    let tenant_id = ctx
        .tenant_id
        .as_deref()
        .ok_or(ShaperailError::Unauthorized)?;

    // Verify the project exists and is active
    let project_id = ctx
        .input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ShaperailError::Validation(vec![FieldError {
                field: "project_id".into(),
                message: "project_id is required".into(),
                code: "required".into(),
            }])
        })?
        .to_string();

    let project: Option<(String,)> = sqlx::query_as(
        "SELECT status::text FROM projects \
         WHERE id = $1::uuid AND org_id = $2::uuid AND deleted_at IS NULL",
    )
    .bind(&project_id)
    .bind(tenant_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(format!("DB error fetching project: {e}")))?;

    match project {
        None => {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "project_id".into(),
                message: "Project not found or does not belong to your organization".into(),
                code: "not_found".into(),
            }]));
        }
        Some((status,)) if status == "archived" => {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "project_id".into(),
                message: "Cannot create tasks in an archived project".into(),
                code: "project_archived".into(),
            }]));
        }
        _ => {}
    }

    // Members may self-assign; tenant admins may assign another external subject.
    if let Some(assigned_to) = ctx.input.get("assigned_to").and_then(|v| v.as_str()) {
        if !assigned_to.is_empty() {
            let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
            if user.role != "admin" && user.role != "super_admin" && assigned_to != user.sub {
                return Err(ShaperailError::Forbidden);
            }
        }
    }

    // Auto-fill created_by from JWT
    if let Some(user) = &ctx.user {
        ctx.input
            .insert("created_by".to_string(), serde_json::json!(&user.sub));
    } else {
        return Err(ShaperailError::Unauthorized);
    }

    // Default priority to "medium" if not set
    if !ctx.input.contains_key("priority") || ctx.input["priority"].is_null() {
        ctx.input
            .insert("priority".to_string(), serde_json::json!("medium"));
    }

    Ok(())
}

/// Called before update — enforce task status transitions and project state.
///
/// - Cannot update tasks in archived projects
/// - Only the assignee or admin can change status to "done"
/// - Cannot reassign if task is "done"
/// - Validate status transitions: todo -> in_progress -> done -> archived
pub async fn enforce_task_rules(ctx: &mut Context) -> ControllerResult {
    let task_id = ctx
        .path_param("id")
        .ok_or_else(|| ShaperailError::Internal("Missing task ID".into()))?;

    // Fetch current task state and its project status
    let task: (String, String, Option<String>) = sqlx::query_as(
        "SELECT t.status::text, p.status::text, t.assigned_to::text \
         FROM tasks t \
         JOIN projects p ON p.id = t.project_id \
         WHERE t.id = $1::uuid",
    )
    .bind(task_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(format!("DB error fetching task: {e}")))?;

    let (current_status, project_status, current_assignee) = task;

    // Cannot update tasks in archived projects
    if project_status == "archived" {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "status".into(),
            message: "Cannot update tasks in an archived project".into(),
            code: "project_archived".into(),
        }]));
    }

    // Cannot reassign if task is "done"
    if current_status == "done" && ctx.input.contains_key("assigned_to") {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "assigned_to".into(),
            message: "Cannot reassign a completed task".into(),
            code: "task_done".into(),
        }]));
    }

    // Validate status transitions
    if let Some(new_status) = ctx.input.get("status").and_then(|v| v.as_str()) {
        let valid_transition = matches!(
            (current_status.as_str(), new_status),
            ("todo", "in_progress")
                | ("in_progress", "done")
                | ("done", "archived")
                // Allow staying in the same status
                | ("todo", "todo")
                | ("in_progress", "in_progress")
                | ("done", "done")
                | ("archived", "archived")
        );

        if !valid_transition {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "status".into(),
                message: format!(
                    "Invalid status transition: {current_status} -> {new_status}. \
                     Allowed: todo -> in_progress -> done -> archived."
                ),
                code: "invalid_transition".into(),
            }]));
        }

        // Only the assignee or admin can change status to "done"
        if new_status == "done" {
            let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;

            let is_assignee = current_assignee
                .as_deref()
                .map(|a| a == user.sub)
                .unwrap_or(false);
            let is_admin = user.role == "admin" || user.role == "super_admin";

            if !is_assignee && !is_admin {
                return Err(ShaperailError::Forbidden);
            }
        }
    }

    Ok(())
}

/// Called after update — add notification headers when assignment or status changes.
///
/// - If assigned_to changed, add X-Notification header
/// - If status changed to "done", add X-Completed-By header
pub async fn notify_assignee(ctx: &mut Context) -> ControllerResult {
    // Check if assigned_to was in the update input (meaning it may have changed)
    if let Some(new_assignee) = ctx.input.get("assigned_to").and_then(|v| v.as_str()) {
        ctx.response_headers.push((
            "X-Notification".into(),
            format!("task-assigned:{new_assignee}"),
        ));
    }

    // If status changed to "done", add X-Completed-By header
    if let Some(status) = ctx.input.get("status").and_then(|v| v.as_str()) {
        if status == "done" {
            if let Some(user) = &ctx.user {
                ctx.response_headers
                    .push(("X-Completed-By".into(), user.sub.clone()));
            }
        }
    }

    Ok(())
}
