use std::collections::HashMap;
use std::sync::Arc;

use actix_multipart::Multipart;
use actix_web::{web, HttpMessage, HttpRequest};
use shaperail_core::{HttpMethod, ResourceDefinition};

use super::crud::{self, AppState};

/// Registers all declared endpoints for a resource into an Actix `ServiceConfig`.
///
/// For each endpoint in `resource.endpoints`, this function maps the HTTP method
/// and path to the appropriate handler. If no endpoints are declared, it
/// registers no HTTP routes for that resource.
pub fn register_resource(
    cfg: &mut web::ServiceConfig,
    resource: &ResourceDefinition,
    _state: Arc<AppState>,
) {
    let resource_arc = Arc::new(resource.clone());

    if let Some(endpoints) = &resource.endpoints {
        for (action, endpoint) in endpoints {
            let ep_arc = Arc::new(endpoint.clone());
            let res = resource_arc.clone();

            // Convert PRD path (/users/:id) to Actix path (/v1/users/{id})
            let actix_path = format!(
                "/v{}{}",
                resource.version,
                endpoint.path().replace(":id", "{id}")
            );

            match action.as_str() {
                "list" => {
                    let ep = ep_arc.clone();
                    let r = res.clone();
                    cfg.route(
                        &actix_path,
                        web::get().to(move |req, state: web::Data<Arc<AppState>>| {
                            let ep = web::Data::new(ep.clone());
                            let r = web::Data::new(r.clone());
                            crud::handle_list(req, state, r, ep)
                        }),
                    );
                }
                "get" => {
                    let ep = ep_arc.clone();
                    let r = res.clone();
                    cfg.route(
                        &actix_path,
                        web::get().to(
                            move |req, state: web::Data<Arc<AppState>>, path: web::Path<String>| {
                                let ep = web::Data::new(ep.clone());
                                let r = web::Data::new(r.clone());
                                crud::handle_get(req, state, r, ep, path)
                            },
                        ),
                    );
                }
                "create" => {
                    let ep = ep_arc.clone();
                    let r = res.clone();
                    if endpoint.upload.is_some() {
                        cfg.route(
                            &actix_path,
                            web::post().to(
                                move |req, state: web::Data<Arc<AppState>>, payload: Multipart| {
                                    let ep = web::Data::new(ep.clone());
                                    let r = web::Data::new(r.clone());
                                    crud::handle_create_upload(req, state, r, ep, payload)
                                },
                            ),
                        );
                    } else {
                        cfg.route(
                            &actix_path,
                            web::post().to(
                                move |req,
                                      state: web::Data<Arc<AppState>>,
                                      body: web::Json<serde_json::Value>| {
                                    let ep = web::Data::new(ep.clone());
                                    let r = web::Data::new(r.clone());
                                    crud::handle_create(req, state, r, ep, body)
                                },
                            ),
                        );
                    }
                }
                "update" => {
                    let ep = ep_arc.clone();
                    let r = res.clone();
                    if endpoint.upload.is_some() {
                        cfg.route(
                            &actix_path,
                            web::patch().to(
                                move |req,
                                      state: web::Data<Arc<AppState>>,
                                      path: web::Path<String>,
                                      payload: Multipart| {
                                    let ep = web::Data::new(ep.clone());
                                    let r = web::Data::new(r.clone());
                                    crud::handle_update_upload(req, state, r, ep, path, payload)
                                },
                            ),
                        );
                    } else {
                        cfg.route(
                            &actix_path,
                            web::patch().to(
                                move |req,
                                      state: web::Data<Arc<AppState>>,
                                      path: web::Path<String>,
                                      body: web::Json<serde_json::Value>| {
                                    let ep = web::Data::new(ep.clone());
                                    let r = web::Data::new(r.clone());
                                    crud::handle_update(req, state, r, ep, path, body)
                                },
                            ),
                        );
                    }
                }
                "delete" => {
                    let ep = ep_arc.clone();
                    let r = res.clone();
                    cfg.route(
                        &actix_path,
                        web::delete().to(
                            move |req, state: web::Data<Arc<AppState>>, path: web::Path<String>| {
                                let ep = web::Data::new(ep.clone());
                                let r = web::Data::new(r.clone());
                                crud::handle_delete(req, state, r, ep, path)
                            },
                        ),
                    );
                }
                "bulk_create" => {
                    let ep = ep_arc.clone();
                    let r = res.clone();
                    cfg.route(
                        &actix_path,
                        web::post().to(
                            move |req,
                                  state: web::Data<Arc<AppState>>,
                                  body: web::Json<serde_json::Value>| {
                                let ep = web::Data::new(ep.clone());
                                let r = web::Data::new(r.clone());
                                crud::handle_bulk_create(req, state, r, ep, body)
                            },
                        ),
                    );
                }
                "bulk_delete" => {
                    let ep = ep_arc.clone();
                    let r = res.clone();
                    cfg.route(
                        &actix_path,
                        web::delete().to(
                            move |req,
                                  state: web::Data<Arc<AppState>>,
                                  body: web::Json<serde_json::Value>| {
                                let ep = web::Data::new(ep.clone());
                                let r = web::Data::new(r.clone());
                                crud::handle_bulk_delete(req, state, r, ep, body)
                            },
                        ),
                    );
                }
                action_name => {
                    // Non-convention endpoint: dispatch to registered custom handler.
                    let Some(method) = endpoint.method.clone() else {
                        // apply_endpoint_defaults was not called; skip registration
                        // rather than panicking. The validator should have caught this.
                        tracing::warn!(
                            resource = %resource.resource,
                            action = %action_name,
                            "custom endpoint has no method set; skipping route registration"
                        );
                        continue;
                    };
                    let ep = ep_arc.clone();
                    let r = res.clone();
                    let action_owned = action_name.to_string();
                    let route = match method {
                        HttpMethod::Get => web::get(),
                        HttpMethod::Post => web::post(),
                        HttpMethod::Patch => web::patch(),
                        HttpMethod::Put => web::put(),
                        HttpMethod::Delete => web::delete(),
                    };
                    cfg.route(
                        &actix_path,
                        route.to(move |req: HttpRequest, state: web::Data<Arc<AppState>>| {
                            let ep = ep.clone();
                            let r = r.clone();
                            let action = action_owned.clone();
                            async move {
                                // If the endpoint declares a before:-controller, build a Context,
                                // run the hook, and stash the result in req.extensions_mut() so
                                // the custom handler can read it via
                                // req.extensions().get::<Context>().
                                let before_name = ep
                                    .controller
                                    .as_ref()
                                    .and_then(|c| c.before.as_deref())
                                    .map(|s| s.to_string());
                                if let Some(before_name) = before_name {
                                    let user = crate::auth::extractor::try_extract_auth(&req);
                                    let headers: HashMap<String, String> = req
                                        .headers()
                                        .iter()
                                        .map(|(k, v)| {
                                            (
                                                k.to_string(),
                                                v.to_str().unwrap_or("").to_string(),
                                            )
                                        })
                                        .collect();
                                    let tenant_id =
                                        crud::resolve_tenant_id(&r, user.as_ref());
                                    let mut ctx = super::controller::Context {
                                        input: serde_json::Map::new(),
                                        data: None,
                                        user: user.clone(),
                                        pool: state.pool.clone(),
                                        headers,
                                        response_headers: vec![],
                                        tenant_id,
                                        session: serde_json::Map::new(),
                                        response_extras: serde_json::Map::new(),
                                    };
                                    #[cfg(feature = "wasm-plugins")]
                                    let wasm_rt = state.wasm_runtime.as_ref();
                                    #[cfg(not(feature = "wasm-plugins"))]
                                    let wasm_rt: Option<&()> = None;
                                    if let Err(e) = super::controller::dispatch_controller(
                                        &before_name,
                                        &r.resource,
                                        &mut ctx,
                                        state.controllers.as_ref(),
                                        wasm_rt,
                                    )
                                    .await
                                    {
                                        use actix_web::ResponseError;
                                        return e.error_response();
                                    }
                                    req.extensions_mut()
                                        .insert(ctx);
                                }

                                let resource_name = r.resource.clone();
                                let key = super::custom::handler_key(&resource_name, &action);
                                let handler = state
                                    .custom_handlers
                                    .as_ref()
                                    .and_then(|m| m.get(&key))
                                    .cloned();
                                match handler {
                                    Some(f) => f(req, state.get_ref().clone(), r, ep).await,
                                    None => actix_web::HttpResponse::NotImplemented()
                                        .json(serde_json::json!({
                                            "error": format!(
                                                "Custom handler '{}' not registered for {resource_name}:{action}",
                                                ep.handler.as_deref().unwrap_or("(none)")
                                            )
                                        })),
                                }
                            }
                        }),
                    );
                }
            }
        }
    }
}

/// Registers all resource routes from a list of resource definitions.
pub fn register_all_resources(
    cfg: &mut web::ServiceConfig,
    resources: &[ResourceDefinition],
    state: Arc<AppState>,
) {
    for resource in resources {
        register_resource(cfg, resource, state.clone());
    }
}
