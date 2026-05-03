use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use shaperail_core::{ShaperailError, WASM_HOOK_PREFIX};

use crate::auth::extractor::AuthenticatedUser;
#[cfg(feature = "wasm-plugins")]
use crate::plugins::{PluginContext, PluginUser, WasmRuntime};

/// Context passed to controller functions for synchronous in-request business logic.
///
/// # Lifecycle
///
/// One `Context` is constructed per CRUD request and **survives both phases**:
///
/// 1. `before:` controller — `data` is `None`. May read/mutate `input`, `session`,
///    `response_extras`, `response_headers`, `tenant_id`.
/// 2. CRUD operation runs. The runtime sets `data` to the persisted record.
/// 3. `after:` controller — `data` is `Some(record)`. May read everything,
///    mutate `data`, `session`, `response_extras`, `response_headers`.
///
/// Anything written to `session` in `before:` is visible in `after:`. Anything
/// written to `response_extras` in either phase is merged into the JSON response
/// body (under the `data:` envelope key) but **never persisted**.
///
/// `input` is **not** reset between phases — by `after:` it reflects what the
/// before-hook wrote, but it is no longer authoritative for the persisted record.
///
/// # Example: minting a one-time secret
///
/// ```rust,ignore
/// async fn mint_mcp_secret(ctx: &mut Context) -> ControllerResult {
///     if ctx.data.is_none() {
///         // before-phase
///         let plaintext = generate_random_secret_32_bytes();
///         let hash = hash_secret(&plaintext);
///         ctx.input.insert("mcp_secret_hash".into(), serde_json::json!(hash));
///         ctx.session.insert("plaintext".into(), serde_json::json!(plaintext));
///     } else {
///         // after-phase
///         if let Some(plaintext) = ctx.session.remove("plaintext") {
///             ctx.response_extras.insert("mcp_secret".into(), plaintext);
///         }
///     }
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct Context {
    /// Mutable input data. Before-controllers can modify what gets written to DB.
    pub input: serde_json::Map<String, serde_json::Value>,
    /// DB result data. `None` in before-controllers, `Some(...)` in after-controllers.
    pub data: Option<serde_json::Value>,
    /// The authenticated user, if present.
    pub user: Option<AuthenticatedUser>,
    /// Database pool for custom queries within the controller.
    pub pool: sqlx::PgPool,
    /// Request headers (read-only).
    pub headers: HashMap<String, String>,
    /// Extra response headers the controller wants to add.
    pub response_headers: Vec<(String, String)>,
    /// The tenant ID extracted from the authenticated user (M18).
    /// Present when the resource has `tenant_key` and the user has a `tenant_id` claim.
    pub tenant_id: Option<String>,
    /// Cross-phase scratch space. Anything written here in a `before:` controller
    /// is visible in the matching `after:` controller for the same request. Never
    /// persisted to the database, never sent to the client.
    pub session: serde_json::Map<String, serde_json::Value>,
    /// Fields to inject into the JSON response body without persisting them.
    ///
    /// Merged into the response under the `data:` envelope key after the after-hook
    /// returns. Useful for one-time values (minted secrets, server-computed URLs,
    /// signed download tokens). Keys here will **shadow** any same-named field on
    /// the persisted record.
    pub response_extras: serde_json::Map<String, serde_json::Value>,
}

/// Type alias for controller function results.
pub type ControllerResult = Result<(), ShaperailError>;

/// Trait for controller functions that can be stored in the registry.
pub trait ControllerHandler: Send + Sync {
    fn call<'a>(
        &'a self,
        ctx: &'a mut Context,
    ) -> Pin<Box<dyn Future<Output = ControllerResult> + Send + 'a>>;
}

/// Blanket implementation for named async functions that take `&mut Context`.
///
/// Use named async functions (not closures) for controller registration:
///
/// ```rust,ignore
/// async fn normalize_email(ctx: &mut Context) -> ControllerResult {
///     // modify ctx.input...
///     Ok(())
/// }
/// map.register("users", "normalize_email", normalize_email);
/// ```
impl<F> ControllerHandler for F
where
    F: for<'a> AsyncControllerFn<'a> + Send + Sync,
{
    fn call<'a>(
        &'a self,
        ctx: &'a mut Context,
    ) -> Pin<Box<dyn Future<Output = ControllerResult> + Send + 'a>> {
        Box::pin(self.call_async(ctx))
    }
}

/// Helper trait to express the async function signature with proper lifetimes.
pub trait AsyncControllerFn<'a> {
    type Fut: Future<Output = ControllerResult> + Send + 'a;
    fn call_async(&self, ctx: &'a mut Context) -> Self::Fut;
}

impl<'a, F, Fut> AsyncControllerFn<'a> for F
where
    F: Fn(&'a mut Context) -> Fut + Send + Sync,
    Fut: Future<Output = ControllerResult> + Send + 'a,
{
    type Fut = Fut;
    fn call_async(&self, ctx: &'a mut Context) -> Self::Fut {
        (self)(ctx)
    }
}

/// Registry that maps (resource_name, function_name) to controller functions.
///
/// Follows the same pattern as `StoreRegistry` — generated code populates this
/// at startup, and handlers look up controllers by name at request time.
pub struct ControllerMap {
    fns: HashMap<(String, String), Arc<dyn ControllerHandler>>,
}

impl ControllerMap {
    /// Creates an empty controller registry.
    pub fn new() -> Self {
        Self {
            fns: HashMap::new(),
        }
    }

    /// Registers a controller function for a resource.
    pub fn register<F>(&mut self, resource: &str, name: &str, f: F)
    where
        F: ControllerHandler + 'static,
    {
        self.fns
            .insert((resource.to_string(), name.to_string()), Arc::new(f));
    }

    /// Calls a controller function by resource and name.
    ///
    /// Returns `Ok(())` if no controller is registered for this (resource, name) pair.
    pub async fn call(&self, resource: &str, name: &str, ctx: &mut Context) -> ControllerResult {
        if let Some(f) = self.fns.get(&(resource.to_string(), name.to_string())) {
            f.call(ctx).await
        } else {
            Err(ShaperailError::Internal(format!(
                "Controller '{name}' not found for resource '{resource}'. \
                 Ensure the function exists in resources/{resource}.controller.rs"
            )))
        }
    }

    /// Returns true if a controller is registered for this (resource, name) pair.
    pub fn has(&self, resource: &str, name: &str) -> bool {
        self.fns
            .contains_key(&(resource.to_string(), name.to_string()))
    }
}

impl Default for ControllerMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Dispatches a controller call, handling both Rust and WASM controllers.
///
/// If `name` starts with `wasm:`, delegates to the WASM runtime.
/// Otherwise, looks up and calls a registered Rust controller function.
#[cfg(feature = "wasm-plugins")]
pub async fn dispatch_controller(
    name: &str,
    resource: &str,
    ctx: &mut Context,
    controllers: Option<&ControllerMap>,
    wasm_runtime: Option<&WasmRuntime>,
) -> ControllerResult {
    if let Some(wasm_path) = name.strip_prefix(WASM_HOOK_PREFIX) {
        // WASM plugin path
        let runtime = wasm_runtime.ok_or_else(|| {
            ShaperailError::Internal(
                "WASM plugin declared but no WasmRuntime configured".to_string(),
            )
        })?;

        // Determine hook name based on whether we're in before or after phase.
        // The caller should set ctx.data = None for before, Some(...) for after.
        let hook_name = if ctx.data.is_none() {
            "before_hook"
        } else {
            "after_hook"
        };

        let plugin_ctx = PluginContext {
            input: ctx.input.clone(),
            data: ctx.data.clone(),
            user: ctx.user.as_ref().map(|u| PluginUser {
                id: u.sub.to_string(),
                role: u.role.clone(),
            }),
            headers: ctx.headers.clone(),
            tenant_id: ctx.tenant_id.clone(),
        };

        let result = runtime.call_hook(wasm_path, hook_name, &plugin_ctx).await?;

        if !result.ok {
            let msg = result
                .error
                .unwrap_or_else(|| "WASM plugin returned error".to_string());
            return Err(ShaperailError::Validation(vec![
                shaperail_core::FieldError {
                    field: "wasm_plugin".to_string(),
                    message: msg,
                    code: "wasm_error".to_string(),
                },
            ]));
        }

        // Apply modifications from plugin back to context
        if let Some(modified_ctx) = result.ctx {
            ctx.input = modified_ctx.input;
            if modified_ctx.data.is_some() {
                ctx.data = modified_ctx.data;
            }
        }

        Ok(())
    } else {
        dispatch_rust_controller(name, resource, ctx, controllers).await
    }
}

/// Dispatches a controller call (Rust controllers only, WASM disabled).
///
/// Any `wasm:` prefix controllers return an error explaining the feature is not enabled.
#[cfg(not(feature = "wasm-plugins"))]
pub async fn dispatch_controller(
    name: &str,
    resource: &str,
    ctx: &mut Context,
    controllers: Option<&ControllerMap>,
    _wasm_runtime: Option<&()>,
) -> ControllerResult {
    if name.starts_with(WASM_HOOK_PREFIX) {
        return Err(ShaperailError::Internal(
            "WASM plugin declared but the 'wasm-plugins' feature is not enabled. \
             Add `features = [\"wasm-plugins\"]` to your shaperail-runtime dependency."
                .to_string(),
        ));
    }
    dispatch_rust_controller(name, resource, ctx, controllers).await
}

/// Shared Rust controller dispatch used by both feature-gated variants.
async fn dispatch_rust_controller(
    name: &str,
    resource: &str,
    ctx: &mut Context,
    controllers: Option<&ControllerMap>,
) -> ControllerResult {
    let map = controllers.ok_or_else(|| {
        ShaperailError::Internal(format!(
            "Controller '{name}' declared for '{resource}' but no ControllerMap configured"
        ))
    })?;
    map.call(resource, name, ctx).await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn normalize_email(ctx: &mut Context) -> ControllerResult {
        if let Some(email) = ctx.input.get("email").and_then(|v| v.as_str()) {
            let lower = email.to_lowercase();
            ctx.input["email"] = serde_json::json!(lower);
        }
        Ok(())
    }

    async fn noop(_ctx: &mut Context) -> ControllerResult {
        Ok(())
    }

    fn test_pool() -> sqlx::PgPool {
        sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap()
    }

    #[tokio::test]
    async fn controller_map_register_and_call() {
        let mut map = ControllerMap::new();
        map.register("users", "normalize_email", normalize_email);

        let mut input = serde_json::Map::new();
        input.insert("email".to_string(), serde_json::json!("USER@EXAMPLE.COM"));

        let mut ctx = Context {
            input,
            data: None,
            user: None,
            pool: test_pool(),
            headers: HashMap::new(),
            response_headers: vec![],
            tenant_id: None,
            session: serde_json::Map::new(),
            response_extras: serde_json::Map::new(),
        };

        map.call("users", "normalize_email", &mut ctx)
            .await
            .unwrap();
        assert_eq!(ctx.input["email"], serde_json::json!("user@example.com"));
    }

    #[tokio::test]
    async fn controller_map_missing_returns_error() {
        let map = ControllerMap::new();
        let mut ctx = Context {
            input: serde_json::Map::new(),
            data: None,
            user: None,
            pool: test_pool(),
            headers: HashMap::new(),
            response_headers: vec![],
            tenant_id: None,
            session: serde_json::Map::new(),
            response_extras: serde_json::Map::new(),
        };

        let result = map.call("users", "nonexistent", &mut ctx).await;
        assert!(result.is_err());
    }

    #[test]
    fn controller_map_has() {
        let mut map = ControllerMap::new();
        assert!(!map.has("users", "check"));
        map.register("users", "check", noop);
        assert!(map.has("users", "check"));
    }
}
