use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

/// Called before create — validate org creation rules beyond auth.
///
/// - Only super_admin can create orgs (double-check beyond auth layer)
/// - Validate org name is unique (case-insensitive)
/// - Set default plan to "free" if not specified
/// - Enforce max org name length and strip whitespace
pub async fn validate_org_creation(ctx: &mut Context) -> ControllerResult {
    // Double-check: only super_admin can create organizations
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
    if user.role != "super_admin" {
        return Err(ShaperailError::Forbidden);
    }

    // Strip whitespace from org name
    if let Some(name) = ctx.input.get("name").and_then(|v| v.as_str()) {
        let trimmed = name.trim().to_string();

        // Enforce max name length after trimming
        if trimmed.len() > 200 {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "name".into(),
                message: "Organization name must be 200 characters or fewer".into(),
                code: "too_long".into(),
            }]));
        }

        if trimmed.is_empty() {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "name".into(),
                message: "Organization name cannot be blank".into(),
                code: "required".into(),
            }]));
        }

        // Write trimmed name back to input
        ctx.input
            .insert("name".to_string(), serde_json::json!(&trimmed));

        // Validate org name is unique (case-insensitive)
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM organizations WHERE LOWER(name) = LOWER($1))",
        )
        .bind(&trimmed)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error checking org name: {e}")))?;

        if exists.0 {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "name".into(),
                message: "An organization with this name already exists".into(),
                code: "unique".into(),
            }]));
        }
    }

    // Set default plan to "free" if not specified
    if !ctx.input.contains_key("plan") || ctx.input["plan"].is_null() {
        ctx.input
            .insert("plan".to_string(), serde_json::json!("free"));
    }

    Ok(())
}

/// Called before update — enforce plan transition rules.
///
/// - Cannot downgrade from enterprise to free (must go through support)
/// - When upgrading plan, log the change for billing
/// - Validate plan transitions: free -> pro -> enterprise (no skipping)
pub async fn enforce_plan_rules(ctx: &mut Context) -> ControllerResult {
    let new_plan = match ctx.input.get("plan").and_then(|v| v.as_str()) {
        Some(plan) => plan.to_string(),
        None => return Ok(()), // No plan change requested
    };

    // Fetch the current plan from the database
    let org_id = ctx
        .path_param("id")
        .ok_or_else(|| ShaperailError::Internal("Missing organization ID".into()))?;

    let current: (String,) =
        sqlx::query_as("SELECT plan::text FROM organizations WHERE id = $1::uuid")
            .bind(org_id)
            .fetch_one(&ctx.pool)
            .await
            .map_err(|e| {
                ShaperailError::Internal(format!("DB error fetching current plan: {e}"))
            })?;

    let current_plan = current.0.as_str();

    // Cannot downgrade from enterprise to free
    if current_plan == "enterprise" && new_plan == "free" {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "plan".into(),
            message: "Cannot downgrade from enterprise to free. Contact support for downgrades."
                .into(),
            code: "invalid_transition".into(),
        }]));
    }

    // Validate plan transitions: free -> pro -> enterprise (no skipping)
    let valid_transition = matches!(
        (current_plan, new_plan.as_str()),
        ("free", "pro")
            | ("pro", "enterprise")
            | ("enterprise", "pro")
            | ("pro", "free")
            | ("free", "free")
            | ("pro", "pro")
            | ("enterprise", "enterprise")
    );

    if !valid_transition {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "plan".into(),
            message: format!(
                "Invalid plan transition: {current_plan} -> {new_plan}. \
                 Plans must follow: free -> pro -> enterprise (no skipping)."
            ),
            code: "invalid_transition".into(),
        }]));
    }

    // Log plan change for billing when upgrading
    if current_plan != new_plan {
        tracing::info!(
            org_id = org_id,
            from_plan = %current_plan,
            to_plan = %new_plan,
            "Plan change detected — billing event logged"
        );
    }

    Ok(())
}
