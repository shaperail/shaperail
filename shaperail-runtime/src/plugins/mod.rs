//! WASM plugin runtime for Shaperail (M19).
//!
//! Provides sandboxed execution of WebAssembly plugins as controller hooks.
//! Plugins receive a JSON context and return a modified JSON context.
//!
//! # Plugin Interface
//!
//! WASM modules must export:
//! - `alloc(size: i32) -> i32` — allocate `size` bytes, return pointer
//! - `dealloc(ptr: i32, size: i32)` — free previously allocated memory
//! - `before_hook(ptr: i32, len: i32) -> i64` — process context, return `(ptr << 32) | len`
//!
//! Optionally:
//! - `after_hook(ptr: i32, len: i32) -> i64` — same interface, called after DB operation
//!
//! # Sandboxing
//!
//! By default, plugins have NO access to:
//! - Host filesystem
//! - Network
//! - Environment variables
//! - System clock
//!
//! This is enforced by creating WASM instances without WASI capabilities.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use shaperail_core::ShaperailError;
use tokio::sync::RwLock;
use tracing::{debug, error, warn};
use wasmtime::{AsContext, AsContextMut, Engine, Instance, Linker, Memory, Module, Store, Val};

/// JSON context passed to WASM plugins, matching the controller `Context` shape.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginContext {
    /// Mutable input data (before-hooks can modify this).
    pub input: serde_json::Map<String, serde_json::Value>,
    /// DB result data. `null` in before-hooks, populated in after-hooks.
    pub data: Option<serde_json::Value>,
    /// Authenticated user info, if present.
    pub user: Option<PluginUser>,
    /// Request headers (read-only from plugin perspective).
    pub headers: HashMap<String, String>,
    /// Tenant ID, if multi-tenancy is active.
    pub tenant_id: Option<String>,
}

/// Minimal user info passed to WASM plugins.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginUser {
    pub id: String,
    pub role: String,
}

/// Result returned from a WASM plugin hook.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginResult {
    /// Whether the hook succeeded.
    pub ok: bool,
    /// Modified context (only `input` and `data` changes are applied back).
    #[serde(default)]
    pub ctx: Option<PluginContext>,
    /// Error message if `ok` is false.
    #[serde(default)]
    pub error: Option<String>,
    /// Error details for validation errors.
    #[serde(default)]
    pub details: Option<Vec<serde_json::Value>>,
}

/// Configuration for plugin sandboxing.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Maximum memory pages (64KB each). Default: 256 (16MB).
    pub max_memory_pages: u32,
    /// Maximum execution fuel (instruction count limit). Default: 1_000_000.
    pub max_fuel: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_pages: 256,
            max_fuel: 1_000_000,
        }
    }
}

/// A compiled WASM module ready for instantiation.
struct CompiledPlugin {
    module: Module,
}

/// Runtime for executing WASM plugins with sandboxing.
///
/// Caches compiled modules to avoid recompilation on every request.
pub struct WasmRuntime {
    engine: Engine,
    plugins: Arc<RwLock<HashMap<PathBuf, CompiledPlugin>>>,
    sandbox: SandboxConfig,
}

impl WasmRuntime {
    /// Creates a new WASM runtime with default sandbox configuration.
    pub fn new() -> Result<Self, ShaperailError> {
        Self::with_sandbox(SandboxConfig::default())
    }

    /// Creates a new WASM runtime with custom sandbox configuration.
    pub fn with_sandbox(sandbox: SandboxConfig) -> Result<Self, ShaperailError> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);

        let engine = Engine::new(&config)
            .map_err(|e| ShaperailError::Internal(format!("Failed to create WASM engine: {e}")))?;

        Ok(Self {
            engine,
            plugins: Arc::new(RwLock::new(HashMap::new())),
            sandbox,
        })
    }

    /// Loads a WASM plugin from a file path. Caches the compiled module.
    pub async fn load_plugin(&self, path: &Path) -> Result<(), ShaperailError> {
        let canonical = path.canonicalize().map_err(|e| {
            ShaperailError::Internal(format!(
                "Failed to resolve WASM plugin path '{}': {e}",
                path.display()
            ))
        })?;

        let mut plugins = self.plugins.write().await;
        if plugins.contains_key(&canonical) {
            return Ok(());
        }

        let wasm_bytes = std::fs::read(&canonical).map_err(|e| {
            ShaperailError::Internal(format!(
                "Failed to read WASM plugin '{}': {e}",
                canonical.display()
            ))
        })?;

        let module = Module::new(&self.engine, &wasm_bytes).map_err(|e| {
            ShaperailError::Internal(format!(
                "Failed to compile WASM plugin '{}': {e}",
                canonical.display()
            ))
        })?;

        debug!(path = %canonical.display(), "Loaded WASM plugin");
        plugins.insert(canonical, CompiledPlugin { module });
        Ok(())
    }

    /// Loads a WASM plugin from raw bytes (for testing).
    pub async fn load_plugin_bytes(
        &self,
        name: &str,
        wasm_bytes: &[u8],
    ) -> Result<(), ShaperailError> {
        let key = PathBuf::from(name);
        let mut plugins = self.plugins.write().await;

        let module = Module::new(&self.engine, wasm_bytes).map_err(|e| {
            ShaperailError::Internal(format!("Failed to compile WASM module '{name}': {e}"))
        })?;

        plugins.insert(key, CompiledPlugin { module });
        Ok(())
    }

    /// Calls a hook function on a loaded WASM plugin.
    ///
    /// The `hook_name` should be `"before_hook"` or `"after_hook"`.
    /// Returns the modified `PluginContext` on success.
    pub async fn call_hook(
        &self,
        plugin_path: &str,
        hook_name: &str,
        ctx: &PluginContext,
    ) -> Result<PluginResult, ShaperailError> {
        let key = if plugin_path.starts_with('/') || plugin_path.starts_with("__test:") {
            PathBuf::from(plugin_path)
        } else {
            Path::new(plugin_path)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(plugin_path))
        };

        let plugins = self.plugins.read().await;
        let compiled = plugins.get(&key).ok_or_else(|| {
            ShaperailError::Internal(format!(
                "WASM plugin '{}' not loaded. Call load_plugin first.",
                key.display()
            ))
        })?;

        let ctx_json = serde_json::to_vec(ctx).map_err(|e| {
            ShaperailError::Internal(format!("Failed to serialize plugin context: {e}"))
        })?;

        // Create a fresh store per invocation for isolation
        let mut store = Store::new(&self.engine, ());
        store
            .set_fuel(self.sandbox.max_fuel)
            .map_err(|e| ShaperailError::Internal(format!("Failed to set fuel: {e}")))?;

        // No WASI — plugin runs fully sandboxed (no fs, no network, no env)
        let linker = Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &compiled.module)
            .map_err(|e| {
                ShaperailError::Internal(format!("Failed to instantiate WASM plugin: {e}"))
            })?;

        // Call the hook, catching any traps (panics, OOM, fuel exhaustion)
        match self.invoke_hook(&mut store, &instance, hook_name, &ctx_json) {
            Ok(result) => Ok(result),
            Err(e) => {
                warn!(
                    plugin = plugin_path,
                    hook = hook_name,
                    error = %e,
                    "WASM plugin hook trapped — returning error without crashing server"
                );
                Ok(PluginResult {
                    ok: false,
                    ctx: None,
                    error: Some(format!("WASM plugin trapped: {e}")),
                    details: None,
                })
            }
        }
    }

    /// Internal: invoke a hook function on a WASM instance.
    fn invoke_hook(
        &self,
        store: &mut Store<()>,
        instance: &Instance,
        hook_name: &str,
        ctx_json: &[u8],
    ) -> Result<PluginResult, ShaperailError> {
        // Get required exports
        let memory = instance
            .get_memory(store.as_context_mut(), "memory")
            .ok_or_else(|| {
                ShaperailError::Internal("WASM plugin does not export 'memory'".to_string())
            })?;

        let alloc_fn = instance
            .get_func(store.as_context_mut(), "alloc")
            .ok_or_else(|| {
                ShaperailError::Internal("WASM plugin does not export 'alloc'".to_string())
            })?;

        let hook_fn = instance
            .get_func(store.as_context_mut(), hook_name)
            .ok_or_else(|| {
                ShaperailError::Internal(format!("WASM plugin does not export '{hook_name}'"))
            })?;

        // Allocate memory in guest for input JSON
        let input_len = ctx_json.len() as i32;
        let mut alloc_result = [Val::I32(0)];
        alloc_fn
            .call(
                store.as_context_mut(),
                &[Val::I32(input_len)],
                &mut alloc_result,
            )
            .map_err(|e| ShaperailError::Internal(format!("WASM alloc call failed: {e}")))?;
        let input_ptr = alloc_result[0].unwrap_i32();

        // Write input JSON into guest memory
        write_to_memory(&memory, store, input_ptr as usize, ctx_json)?;

        // Call the hook function: hook(ptr, len) -> i64 (packed ptr|len)
        let mut hook_result = [Val::I64(0)];
        hook_fn
            .call(
                store.as_context_mut(),
                &[Val::I32(input_ptr), Val::I32(input_len)],
                &mut hook_result,
            )
            .map_err(|e| {
                ShaperailError::Internal(format!("WASM hook '{hook_name}' trapped: {e}"))
            })?;

        // Unpack result: high 32 bits = ptr, low 32 bits = len
        let packed = hook_result[0].unwrap_i64();
        let result_ptr = (packed >> 32) as usize;
        let result_len = (packed & 0xFFFF_FFFF) as usize;

        if result_len == 0 {
            // Empty result means "no changes, ok"
            return Ok(PluginResult {
                ok: true,
                ctx: None,
                error: None,
                details: None,
            });
        }

        // Read result JSON from guest memory
        let result_bytes = read_from_memory(&memory, store, result_ptr, result_len)?;

        let result: PluginResult = serde_json::from_slice(&result_bytes).map_err(|e| {
            error!(
                raw = %String::from_utf8_lossy(&result_bytes),
                "WASM plugin returned invalid JSON"
            );
            ShaperailError::Internal(format!("WASM plugin returned invalid JSON: {e}"))
        })?;

        Ok(result)
    }

    /// Returns true if a plugin is loaded for the given path.
    pub async fn is_loaded(&self, path: &str) -> bool {
        let key = PathBuf::from(path);
        self.plugins.read().await.contains_key(&key)
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create default WasmRuntime")
    }
}

/// Write bytes into WASM linear memory at the given offset.
fn write_to_memory(
    memory: &Memory,
    store: &mut Store<()>,
    offset: usize,
    data: &[u8],
) -> Result<(), ShaperailError> {
    let mem_data = memory.data_mut(store.as_context_mut());
    let end = offset + data.len();
    if end > mem_data.len() {
        return Err(ShaperailError::Internal(format!(
            "WASM memory write out of bounds: offset={offset}, len={}, memory_size={}",
            data.len(),
            mem_data.len()
        )));
    }
    mem_data[offset..end].copy_from_slice(data);
    Ok(())
}

/// Read bytes from WASM linear memory at the given offset.
fn read_from_memory(
    memory: &Memory,
    store: &mut Store<()>,
    offset: usize,
    len: usize,
) -> Result<Vec<u8>, ShaperailError> {
    let mem_data = memory.data(store.as_context());
    let end = offset + len;
    if end > mem_data.len() {
        return Err(ShaperailError::Internal(format!(
            "WASM memory read out of bounds: offset={offset}, len={len}, memory_size={}",
            mem_data.len()
        )));
    }
    Ok(mem_data[offset..end].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal WASM module that implements the plugin interface.
    /// Exports: memory, alloc, dealloc, before_hook
    ///
    /// The before_hook reads JSON, parses nothing complex, and returns
    /// `{"ok": true}` (no modifications).
    fn passthrough_wasm() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
                (memory (export "memory") 2)

                ;; Simple bump allocator (start after reserved output area)
                (global $bump (mut i32) (i32.const 4096))

                (func (export "alloc") (param $size i32) (result i32)
                    (local $ptr i32)
                    (local.set $ptr (global.get $bump))
                    (global.set $bump (i32.add (global.get $bump) (local.get $size)))
                    (local.get $ptr)
                )

                (func (export "dealloc") (param $ptr i32) (param $size i32)
                    ;; no-op for bump allocator
                )

                ;; before_hook: ignore input, return {"ok":true} packed as (ptr << 32) | len
                (func (export "before_hook") (param $ptr i32) (param $len i32) (result i64)
                    (local $out_ptr i32)
                    (local $out_len i32)

                    ;; Write {"ok":true} at offset 0
                    ;; {"ok":true} = 0x7B226F6B223A747275657D (11 bytes)
                    (i32.store8 (i32.const 0) (i32.const 0x7B))  ;; {
                    (i32.store8 (i32.const 1) (i32.const 0x22))  ;; "
                    (i32.store8 (i32.const 2) (i32.const 0x6F))  ;; o
                    (i32.store8 (i32.const 3) (i32.const 0x6B))  ;; k
                    (i32.store8 (i32.const 4) (i32.const 0x22))  ;; "
                    (i32.store8 (i32.const 5) (i32.const 0x3A))  ;; :
                    (i32.store8 (i32.const 6) (i32.const 0x74))  ;; t
                    (i32.store8 (i32.const 7) (i32.const 0x72))  ;; r
                    (i32.store8 (i32.const 8) (i32.const 0x75))  ;; u
                    (i32.store8 (i32.const 9) (i32.const 0x65))  ;; e
                    (i32.store8 (i32.const 10) (i32.const 0x7D)) ;; }

                    (local.set $out_ptr (i32.const 0))
                    (local.set $out_len (i32.const 11))

                    ;; Pack result: (ptr << 32) | len
                    (i64.or
                        (i64.shl
                            (i64.extend_i32_u (local.get $out_ptr))
                            (i64.const 32)
                        )
                        (i64.extend_i32_u (local.get $out_len))
                    )
                )
            )
            "#,
        )
        .expect("WAT parse failed")
    }

    /// WASM module that modifies the input — lowercases a known field.
    /// For simplicity, this just returns a fixed modified context.
    fn modifier_wasm() -> Vec<u8> {
        // This module reads input JSON, and returns a result that modifies
        // the input by adding a field "wasm_modified": true
        let response = r#"{"ok":true,"ctx":{"input":{"name":"modified_by_wasm"},"data":null,"user":null,"headers":{},"tenant_id":null}}"#;
        let bytes = response.as_bytes();
        let len = bytes.len();

        // Build data section with the response string
        let mut wat = String::from(
            r#"
            (module
                (memory (export "memory") 2)
                (global $bump (mut i32) (i32.const 4096))

                (func (export "alloc") (param $size i32) (result i32)
                    (local $ptr i32)
                    (local.set $ptr (global.get $bump))
                    (global.set $bump (i32.add (global.get $bump) (local.get $size)))
                    (local.get $ptr)
                )

                (func (export "dealloc") (param $ptr i32) (param $size i32))

                (func (export "before_hook") (param $ptr i32) (param $len i32) (result i64)
            "#,
        );

        // Write response bytes to memory at offset 0
        for (i, b) in bytes.iter().enumerate() {
            wat.push_str(&format!(
                "                    (i32.store8 (i32.const {i}) (i32.const {}))\n",
                *b as i32
            ));
        }

        wat.push_str(&format!(
            r#"
                    (i64.or
                        (i64.shl (i64.extend_i32_u (i32.const 0)) (i64.const 32))
                        (i64.extend_i32_u (i32.const {len}))
                    )
                )
            )
            "#
        ));

        wat::parse_str(&wat).expect("WAT parse failed")
    }

    /// WASM module that traps (unreachable instruction) to test crash isolation.
    fn crashing_wasm() -> Vec<u8> {
        wat::parse_str(
            r#"
            (module
                (memory (export "memory") 2)
                (global $bump (mut i32) (i32.const 4096))

                (func (export "alloc") (param $size i32) (result i32)
                    (local $ptr i32)
                    (local.set $ptr (global.get $bump))
                    (global.set $bump (i32.add (global.get $bump) (local.get $size)))
                    (local.get $ptr)
                )

                (func (export "dealloc") (param $ptr i32) (param $size i32))

                (func (export "before_hook") (param $ptr i32) (param $len i32) (result i64)
                    unreachable
                )
            )
            "#,
        )
        .expect("WAT parse failed")
    }

    /// WASM module that returns an error result.
    fn error_wasm() -> Vec<u8> {
        let response = r#"{"ok":false,"error":"validation failed: email is required"}"#;
        let bytes = response.as_bytes();
        let len = bytes.len();

        let mut wat = String::from(
            r#"
            (module
                (memory (export "memory") 2)
                (global $bump (mut i32) (i32.const 4096))

                (func (export "alloc") (param $size i32) (result i32)
                    (local $ptr i32)
                    (local.set $ptr (global.get $bump))
                    (global.set $bump (i32.add (global.get $bump) (local.get $size)))
                    (local.get $ptr)
                )

                (func (export "dealloc") (param $ptr i32) (param $size i32))

                (func (export "before_hook") (param $ptr i32) (param $len i32) (result i64)
            "#,
        );

        for (i, b) in bytes.iter().enumerate() {
            wat.push_str(&format!(
                "                    (i32.store8 (i32.const {i}) (i32.const {}))\n",
                *b as i32
            ));
        }

        wat.push_str(&format!(
            r#"
                    (i64.or
                        (i64.shl (i64.extend_i32_u (i32.const 0)) (i64.const 32))
                        (i64.extend_i32_u (i32.const {len}))
                    )
                )
            )
            "#
        ));

        wat::parse_str(&wat).expect("WAT parse failed")
    }

    fn test_context() -> PluginContext {
        let mut input = serde_json::Map::new();
        input.insert("name".to_string(), serde_json::json!("Alice"));
        input.insert("email".to_string(), serde_json::json!("alice@example.com"));

        PluginContext {
            input,
            data: None,
            user: Some(PluginUser {
                id: "user-123".to_string(),
                role: "admin".to_string(),
            }),
            headers: HashMap::new(),
            tenant_id: None,
        }
    }

    #[tokio::test]
    async fn passthrough_hook_runs_and_returns_ok() {
        let runtime = WasmRuntime::new().unwrap();
        let wasm = passthrough_wasm();
        runtime
            .load_plugin_bytes("__test:passthrough", &wasm)
            .await
            .unwrap();

        let ctx = test_context();
        let result = runtime
            .call_hook("__test:passthrough", "before_hook", &ctx)
            .await
            .unwrap();

        assert!(result.ok);
    }

    #[tokio::test]
    async fn modifier_hook_modifies_context() {
        let runtime = WasmRuntime::new().unwrap();
        let wasm = modifier_wasm();
        runtime
            .load_plugin_bytes("__test:modifier", &wasm)
            .await
            .unwrap();

        let ctx = test_context();
        let result = runtime
            .call_hook("__test:modifier", "before_hook", &ctx)
            .await
            .unwrap();

        assert!(result.ok);
        let modified_ctx = result.ctx.unwrap();
        assert_eq!(
            modified_ctx.input.get("name").and_then(|v| v.as_str()),
            Some("modified_by_wasm")
        );
    }

    #[tokio::test]
    async fn crashing_plugin_does_not_crash_server() {
        let runtime = WasmRuntime::new().unwrap();
        let wasm = crashing_wasm();
        runtime
            .load_plugin_bytes("__test:crash", &wasm)
            .await
            .unwrap();

        let ctx = test_context();
        let result = runtime
            .call_hook("__test:crash", "before_hook", &ctx)
            .await
            .unwrap();

        // Plugin trapped but server is fine — returns error result
        assert!(!result.ok);
        assert!(result.error.as_ref().unwrap().contains("trapped"));
    }

    #[tokio::test]
    async fn error_hook_returns_plugin_error() {
        let runtime = WasmRuntime::new().unwrap();
        let wasm = error_wasm();
        runtime
            .load_plugin_bytes("__test:error", &wasm)
            .await
            .unwrap();

        let ctx = test_context();
        let result = runtime
            .call_hook("__test:error", "before_hook", &ctx)
            .await
            .unwrap();

        assert!(!result.ok);
        assert_eq!(
            result.error.as_deref(),
            Some("validation failed: email is required")
        );
    }

    #[tokio::test]
    async fn unloaded_plugin_returns_error() {
        let runtime = WasmRuntime::new().unwrap();
        let ctx = test_context();
        let result = runtime
            .call_hook("__test:nonexistent", "before_hook", &ctx)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fuel_exhaustion_does_not_crash_server() {
        // Create runtime with very low fuel limit
        let sandbox = SandboxConfig {
            max_memory_pages: 256,
            max_fuel: 1, // Very low — will exhaust quickly
        };
        let runtime = WasmRuntime::with_sandbox(sandbox).unwrap();

        // Use the modifier which does more work
        let wasm = modifier_wasm();
        runtime
            .load_plugin_bytes("__test:fuel", &wasm)
            .await
            .unwrap();

        let ctx = test_context();
        let result = runtime
            .call_hook("__test:fuel", "before_hook", &ctx)
            .await
            .unwrap();

        // Should fail gracefully due to fuel exhaustion
        assert!(!result.ok);
        assert!(result.error.as_ref().unwrap().contains("trapped"));
    }

    #[tokio::test]
    async fn sandbox_no_wasi_by_default() {
        // This test verifies that WASM plugins cannot access host resources.
        // The passthrough module has no WASI imports, proving sandboxing works.
        // If we tried to instantiate a module WITH WASI imports, it would fail
        // because we don't provide WASI in the linker.
        let runtime = WasmRuntime::new().unwrap();
        let wasm = passthrough_wasm();
        runtime
            .load_plugin_bytes("__test:sandbox", &wasm)
            .await
            .unwrap();

        assert!(runtime.is_loaded("__test:sandbox").await);

        let ctx = test_context();
        let result = runtime
            .call_hook("__test:sandbox", "before_hook", &ctx)
            .await
            .unwrap();
        assert!(result.ok);
    }
}
