//! Custom endpoint handler dispatch.
//!
//! Users declare non-CRUD endpoints in resource YAML with a `handler:` field.
//! The framework enforces auth, validation, and rate limiting; user code provides
//! only the business logic function.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse};
use shaperail_core::{EndpointSpec, ResourceDefinition};

use super::crud::AppState;

/// A custom handler function: receives request context, returns HTTP response.
pub type CustomHandlerFn = Arc<
    dyn Fn(
            HttpRequest,
            Arc<AppState>,
            Arc<ResourceDefinition>,
            Arc<EndpointSpec>,
        ) -> Pin<Box<dyn Future<Output = HttpResponse> + Send>>
        + Send
        + Sync,
>;

/// Registry mapping "{resource}:{action}" to a custom handler function.
pub type CustomHandlerMap = HashMap<String, CustomHandlerFn>;

/// Build the registry key for a custom handler.
pub fn handler_key(resource: &str, action: &str) -> String {
    format!("{resource}:{action}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_key_format() {
        assert_eq!(handler_key("users", "invite"), "users:invite");
        assert_eq!(handler_key("orders", "cancel"), "orders:cancel");
    }

    #[test]
    fn custom_handler_map_lookup() {
        let mut map: CustomHandlerMap = HashMap::new();
        let key = handler_key("users", "invite");
        let handler: CustomHandlerFn =
            Arc::new(|_req, _state, _res, _ep| Box::pin(async { HttpResponse::Ok().finish() }));
        map.insert(key.clone(), handler);
        assert!(map.contains_key(&key));
        assert!(!map.contains_key("users:ban"));
    }
}
