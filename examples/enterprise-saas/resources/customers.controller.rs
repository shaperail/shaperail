use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

/// Called before create — validate email uniqueness per org, enforce credit limits
/// based on plan tier, and auto-fill created_by from the authenticated user.
pub async fn validate_customer(ctx: &mut Context) -> ControllerResult {
    // Auto-fill created_by from JWT
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
    ctx.input
        .insert("created_by".to_string(), serde_json::json!(&user.sub));

    // Auto-fill org_id from tenant context
    if let Some(ref tenant_id) = ctx.tenant_id {
        ctx.input
            .insert("org_id".to_string(), serde_json::json!(tenant_id));
    }

    let org_id = ctx
        .input
        .get("org_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ShaperailError::Validation(vec![FieldError {
                field: "org_id".into(),
                message: "org_id is required".into(),
                code: "required".into(),
            }])
        })?
        .to_string();

    let email = ctx
        .input
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ShaperailError::Validation(vec![FieldError {
                field: "email".into(),
                message: "email is required".into(),
                code: "required".into(),
            }])
        })?
        .to_string();

    // Check email uniqueness within the org
    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM customers WHERE org_id = $1::uuid AND email = $2 AND deleted_at IS NULL",
    )
    .bind(&org_id)
    .bind(&email)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(format!("DB error checking email uniqueness: {e}")))?;

    if existing > 0 {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "email".into(),
            message: format!("A customer with email '{email}' already exists in this organization"),
            code: "unique".into(),
        }]));
    }

    // Enforce credit_limit_cents based on plan tier
    let plan = ctx
        .input
        .get("plan")
        .and_then(|v| v.as_str())
        .unwrap_or("free");

    let credit_limit = ctx
        .input
        .get("credit_limit_cents")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let max_credit = match plan {
        "free" => 0,
        "starter" => 50_000,
        "pro" => 500_000,
        "enterprise" => i64::MAX,
        _ => {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "plan".into(),
                message: format!("Unknown plan: {plan}"),
                code: "invalid".into(),
            }]));
        }
    };

    if credit_limit > max_credit {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "credit_limit_cents".into(),
            message: format!(
                "Credit limit {credit_limit} exceeds maximum {max_credit} for plan '{plan}'"
            ),
            code: "exceeds_plan_limit".into(),
        }]));
    }

    Ok(())
}

/// Called before update — validate plan transitions (no skipping tiers),
/// prevent downgrade when outstanding invoices exist, and log the plan
/// change for billing audit.
pub async fn enforce_plan_change(ctx: &mut Context) -> ControllerResult {
    let new_plan = match ctx.input.get("plan").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return Ok(()), // not changing plan, nothing to enforce
    };

    // Fetch current customer data to check existing plan
    let resource_id = ctx
        .path_param("id")
        .ok_or_else(|| ShaperailError::Internal("Missing customer ID".into()))?;

    let current_plan: String = sqlx::query_scalar("SELECT plan FROM customers WHERE id = $1::uuid")
        .bind(resource_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error fetching customer: {e}")))?;

    // Define tier ordering: free=0, starter=1, pro=2, enterprise=3
    let tier = |p: &str| -> i32 {
        match p {
            "free" => 0,
            "starter" => 1,
            "pro" => 2,
            "enterprise" => 3,
            _ => -1,
        }
    };

    let current_tier = tier(&current_plan);
    let new_tier = tier(&new_plan);

    if new_tier == current_tier {
        return Ok(()); // no change
    }

    // Prevent skipping tiers (can only move one tier at a time)
    if (new_tier - current_tier).abs() > 1 {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "plan".into(),
            message: format!(
                "Cannot change plan from '{current_plan}' to '{new_plan}': \
                 plan changes must move one tier at a time"
            ),
            code: "invalid_plan_transition".into(),
        }]));
    }

    // Prevent downgrade if there are outstanding (non-paid, non-void) invoices
    if new_tier < current_tier {
        let outstanding: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices \
             WHERE customer_id = $1::uuid AND status NOT IN ('paid', 'void') AND deleted_at IS NULL",
        )
        .bind(resource_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| {
            ShaperailError::Internal(format!("DB error checking outstanding invoices: {e}"))
        })?;

        if outstanding > 0 {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "plan".into(),
                message: format!(
                    "Cannot downgrade from '{current_plan}' to '{new_plan}': \
                     {outstanding} outstanding invoice(s) must be paid or voided first"
                ),
                code: "outstanding_invoices".into(),
            }]));
        }
    }

    // Log plan change to audit_logs for billing audit trail
    let user_id = ctx.user.as_ref().map(|u| u.sub.clone()).unwrap_or_default();

    let ip_address = ctx.client_ip().unwrap_or("unknown").to_string();

    sqlx::query(
        "INSERT INTO audit_logs (id, user_id, resource_type, resource_id, action, before_data, after_data, ip_address, created_at) \
         VALUES (gen_random_uuid(), $1, 'customers', $2, 'plan_change', $3, $4, $5, NOW())",
    )
    .bind(&user_id)
    .bind(resource_id)
    .bind(serde_json::json!({ "plan": current_plan }))
    .bind(serde_json::json!({ "plan": new_plan }))
    .bind(&ip_address)
    .execute(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(format!("DB error writing audit log: {e}")))?;

    Ok(())
}
