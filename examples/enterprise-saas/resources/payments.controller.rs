use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

/// Called before create — verify invoice is in a payable state, check amount
/// does not exceed total, detect duplicate payments for idempotency, and
/// auto-fill processed_by from the JWT.
pub async fn validate_payment(ctx: &mut Context) -> ControllerResult {
    // Auto-fill processed_by from JWT
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
    ctx.input
        .insert("processed_by".to_string(), serde_json::json!(&user.sub));

    // Auto-fill org_id from tenant context
    if let Some(ref tenant_id) = ctx.tenant_id {
        ctx.input
            .insert("org_id".to_string(), serde_json::json!(tenant_id));
    }

    let invoice_id = ctx
        .input
        .get("invoice_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ShaperailError::Validation(vec![FieldError {
                field: "invoice_id".into(),
                message: "invoice_id is required".into(),
                code: "required".into(),
            }])
        })?
        .to_string();

    let amount_cents = ctx
        .input
        .get("amount_cents")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| {
            ShaperailError::Validation(vec![FieldError {
                field: "amount_cents".into(),
                message: "amount_cents is required and must be a number".into(),
                code: "required".into(),
            }])
        })?;

    // Verify invoice exists and is in a payable state (sent or overdue)
    let invoice_row: Option<(String, i64)> = sqlx::query_as(
        "SELECT status, total_cents FROM invoices WHERE id = $1::uuid AND deleted_at IS NULL",
    )
    .bind(&invoice_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(format!("DB error fetching invoice: {e}")))?;

    let (invoice_status, invoice_total) = match invoice_row {
        Some(row) => row,
        None => {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "invoice_id".into(),
                message: format!("Invoice '{invoice_id}' not found"),
                code: "not_found".into(),
            }]));
        }
    };

    if invoice_status != "sent" && invoice_status != "overdue" {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "invoice_id".into(),
            message: format!(
                "Invoice has status '{invoice_status}'; payments can only be made against 'sent' or 'overdue' invoices"
            ),
            code: "invalid_invoice_status".into(),
        }]));
    }

    // Sum existing completed/pending payments for this invoice
    let paid_so_far: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_cents), 0) FROM payments \
         WHERE invoice_id = $1::uuid AND status IN ('pending', 'completed')",
    )
    .bind(&invoice_id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(format!("DB error summing payments: {e}")))?;

    let remaining = invoice_total - paid_so_far;
    if amount_cents > remaining {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "amount_cents".into(),
            message: format!(
                "Payment of {amount_cents} exceeds remaining balance of {remaining} \
                 (invoice total: {invoice_total}, already paid: {paid_so_far})"
            ),
            code: "exceeds_balance".into(),
        }]));
    }

    // Idempotency: check for duplicate payments (same invoice + amount within 5 minutes)
    let duplicate: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM payments \
         WHERE invoice_id = $1::uuid AND amount_cents = $2 \
         AND created_at > NOW() - INTERVAL '5 minutes'",
    )
    .bind(&invoice_id)
    .bind(amount_cents)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(format!("DB error checking duplicates: {e}")))?;

    if duplicate > 0 {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "amount_cents".into(),
            message: format!(
                "Duplicate payment detected: a payment of {amount_cents} for invoice '{invoice_id}' \
                 was recorded within the last 5 minutes"
            ),
            code: "duplicate_payment".into(),
        }]));
    }

    Ok(())
}

/// Called before update — enforce payment modification rules:
/// - Only admin can mark as refunded
/// - Cannot modify completed or refunded payments
/// - When completing a payment, check if it fully covers the invoice and
///   auto-update the invoice status to "paid"
pub async fn enforce_payment_rules(ctx: &mut Context) -> ControllerResult {
    let resource_id = ctx
        .path_param("id")
        .ok_or_else(|| ShaperailError::Internal("Missing payment ID".into()))?;

    // Fetch current payment state
    let row: (String, String) =
        sqlx::query_as("SELECT status, invoice_id::text FROM payments WHERE id = $1::uuid")
            .bind(resource_id)
            .fetch_one(&ctx.pool)
            .await
            .map_err(|e| ShaperailError::Internal(format!("DB error fetching payment: {e}")))?;

    let (current_status, invoice_id) = row;

    // Cannot modify completed or refunded payments
    if current_status == "completed" || current_status == "refunded" {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "status".into(),
            message: format!("Cannot modify a payment with status '{current_status}'"),
            code: "immutable_status".into(),
        }]));
    }

    let new_status = match ctx.input.get("status").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return Ok(()), // not changing status
    };

    // Only admin can mark as refunded
    if new_status == "refunded" {
        let user_role = ctx
            .user
            .as_ref()
            .map(|u| u.role.as_str())
            .unwrap_or("unknown");

        if user_role != "admin" {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "status".into(),
                message: "Only admin role can mark payments as refunded".into(),
                code: "insufficient_role".into(),
            }]));
        }
    }

    // When completing a payment, check if invoice is fully paid
    if new_status == "completed" {
        // Get the payment amount and invoice total
        let payment_amount: i64 =
            sqlx::query_scalar("SELECT amount_cents FROM payments WHERE id = $1::uuid")
                .bind(resource_id)
                .fetch_one(&ctx.pool)
                .await
                .map_err(|e| {
                    ShaperailError::Internal(format!("DB error fetching payment amount: {e}"))
                })?;

        let invoice_total: i64 =
            sqlx::query_scalar("SELECT total_cents FROM invoices WHERE id = $1::uuid")
                .bind(&invoice_id)
                .fetch_one(&ctx.pool)
                .await
                .map_err(|e| {
                    ShaperailError::Internal(format!("DB error fetching invoice total: {e}"))
                })?;

        // Sum all completed payments for this invoice (excluding current one being updated)
        let already_paid: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_cents), 0) FROM payments \
             WHERE invoice_id = $1::uuid AND status = 'completed' AND id != $2::uuid",
        )
        .bind(&invoice_id)
        .bind(resource_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error summing payments: {e}")))?;

        let total_after_completion = already_paid + payment_amount;

        // If this payment fully covers the invoice, auto-mark invoice as paid
        if total_after_completion >= invoice_total {
            sqlx::query(
                "UPDATE invoices SET status = 'paid', paid_at = NOW(), updated_at = NOW() WHERE id = $1::uuid",
            )
            .bind(&invoice_id)
            .execute(&ctx.pool)
            .await
            .map_err(|e| {
                ShaperailError::Internal(format!("DB error auto-updating invoice status: {e}"))
            })?;

            // Audit the automatic invoice status change
            let user_id = ctx.user.as_ref().map(|u| u.sub.clone()).unwrap_or_default();

            let ip_address = ctx.client_ip().unwrap_or("unknown").to_string();

            sqlx::query(
                "INSERT INTO audit_logs (id, user_id, resource_type, resource_id, action, before_data, after_data, ip_address, created_at) \
                 VALUES (gen_random_uuid(), $1, 'invoices', $2, 'auto_paid', $3, $4, $5, NOW())",
            )
            .bind(&user_id)
            .bind(&invoice_id)
            .bind(serde_json::json!({ "status": "sent", "payment_id": resource_id }))
            .bind(serde_json::json!({ "status": "paid", "total_paid": total_after_completion }))
            .bind(&ip_address)
            .execute(&ctx.pool)
            .await
            .map_err(|e| ShaperailError::Internal(format!("DB error writing audit log: {e}")))?;
        }
    }

    Ok(())
}
