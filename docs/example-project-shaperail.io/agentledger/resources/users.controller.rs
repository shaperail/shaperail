//! Controller for the `users` resource.
//!
//! `hash_password` is a before-hook on the create endpoint. It reads the
//! plaintext `password` from `ctx.input`, argon2-hashes it, removes
//! `password` from the input map, and writes the hash to `password_hash`
//! before the runtime persists the row.
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHasher,
};
use serde_json::Value;
use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::ControllerContext;

fn validation(field: &str, code: &str, message: &str) -> ShaperailError {
    ShaperailError::Validation(vec![FieldError {
        field: field.to_string(),
        message: message.to_string(),
        code: code.to_string(),
    }])
}

pub async fn hash_password(ctx: &mut ControllerContext) -> Result<(), ShaperailError> {
    let password = ctx
        .input
        .get("password")
        .and_then(Value::as_str)
        .ok_or_else(|| validation("password", "required", "password is required"))?
        .to_string();

    if password.len() < 12 {
        return Err(validation(
            "password",
            "too_short",
            "password must be at least 12 characters",
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| ShaperailError::Internal(format!("argon2 hash failed: {e}")))?
        .to_string();

    ctx.input.remove("password");
    ctx.input
        .insert("password_hash".into(), Value::String(hash));
    Ok(())
}
