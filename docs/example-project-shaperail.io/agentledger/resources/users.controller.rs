//! Controller for the `users` resource.
//!
//! `hash_password` is a before-hook on the create endpoint. The runtime has
//! already validated `password` against the schema rules (transient + min: 12
//! + max: 255) in phase 1. This controller argon2-hashes the plaintext and
//! writes the result to `password_hash`. Phase 2 then verifies `password_hash`
//! is present, and the runtime strips the transient `password` field before
//! the INSERT.
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHasher,
};
use serde_json::Value;
use shaperail_core::ShaperailError;
use shaperail_runtime::handlers::ControllerContext;

pub async fn hash_password(ctx: &mut ControllerContext) -> Result<(), ShaperailError> {
    // password is transient + required: phase 2 will report it missing if absent.
    let Some(password) = ctx
        .input
        .get("password")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(());
    };

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| ShaperailError::Internal(format!("argon2 hash failed: {e}")))?
        .to_string();

    ctx.input
        .insert("password_hash".into(), Value::String(hash));
    Ok(())
}
