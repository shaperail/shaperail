use std::sync::Arc;

use actix_web::web;
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

            // Convert PRD path (/users/:id) to Actix path (/users/{id})
            let actix_path = endpoint.path.replace(":id", "{id}");

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
                "update" => {
                    let ep = ep_arc.clone();
                    let r = res.clone();
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
                _ => {
                    // Custom endpoint names — map by HTTP method
                    match endpoint.method {
                        HttpMethod::Get if actix_path.contains("{id}") => {
                            let ep = ep_arc.clone();
                            let r = res.clone();
                            cfg.route(
                                &actix_path,
                                web::get().to(
                                    move |req,
                                          state: web::Data<Arc<AppState>>,
                                          path: web::Path<String>| {
                                        let ep = web::Data::new(ep.clone());
                                        let r = web::Data::new(r.clone());
                                        crud::handle_get(req, state, r, ep, path)
                                    },
                                ),
                            );
                        }
                        HttpMethod::Get => {
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
                        HttpMethod::Post => {
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
                                        crud::handle_create(req, state, r, ep, body)
                                    },
                                ),
                            );
                        }
                        HttpMethod::Patch | HttpMethod::Put => {
                            let ep = ep_arc.clone();
                            let r = res.clone();
                            cfg.route(
                                &actix_path,
                                web::method(match endpoint.method {
                                    HttpMethod::Patch => actix_web::http::Method::PATCH,
                                    _ => actix_web::http::Method::PUT,
                                })
                                .to(
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
                        HttpMethod::Delete => {
                            let ep = ep_arc.clone();
                            let r = res.clone();
                            cfg.route(
                                &actix_path,
                                web::delete().to(
                                    move |req,
                                          state: web::Data<Arc<AppState>>,
                                          path: web::Path<String>| {
                                        let ep = web::Data::new(ep.clone());
                                        let r = web::Data::new(r.clone());
                                        crud::handle_delete(req, state, r, ep, path)
                                    },
                                ),
                            );
                        }
                    }
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
