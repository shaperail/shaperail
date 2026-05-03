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
                        route.to(move |req: HttpRequest, body: web::Bytes, state: web::Data<Arc<AppState>>| {
                            let ep = ep.clone();
                            let r = r.clone();
                            let action = action_owned.clone();
                            // Stash the buffered body in request extensions so the custom handler
                            // can read it via `req.extensions().get::<web::Bytes>().cloned()`.
                            // actix-web only extracts the request payload when an extractor is
                            // declared in the closure's argument list — without `body: web::Bytes`
                            // here, ServiceRequest.payload is dropped and `req.take_payload()`
                            // returns Payload::None unconditionally.
                            req.extensions_mut().insert(body);
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use actix_web::HttpMessage;
    use indexmap::IndexMap;
    use shaperail_core::{FieldSchema, FieldType, ResourceDefinition};

    use super::super::controller::Context;
    use super::super::crud::resolve_tenant_id;
    use crate::auth::extractor::AuthenticatedUser;

    fn test_pool() -> sqlx::PgPool {
        sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap()
    }

    fn user_with_tenant(tenant_id: &str) -> AuthenticatedUser {
        AuthenticatedUser {
            sub: "user-1".to_string(),
            role: "member".to_string(),
            tenant_id: Some(tenant_id.to_string()),
        }
    }

    fn uuid_field() -> FieldSchema {
        FieldSchema {
            field_type: FieldType::Uuid,
            primary: true,
            generated: true,
            required: false,
            unique: false,
            nullable: false,
            reference: None,
            min: None,
            max: None,
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: false,
            items: None,
            transient: false,
        }
    }

    fn fk_field() -> FieldSchema {
        FieldSchema {
            field_type: FieldType::Uuid,
            primary: false,
            generated: false,
            required: true,
            unique: false,
            nullable: false,
            reference: Some("organizations.id".to_string()),
            min: None,
            max: None,
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: false,
            items: None,
            transient: false,
        }
    }

    fn resource_with_tenant_key(tenant_key: &str) -> ResourceDefinition {
        let mut schema = IndexMap::new();
        schema.insert("id".to_string(), uuid_field());
        schema.insert(tenant_key.to_string(), fk_field());
        ResourceDefinition {
            resource: "agents".to_string(),
            version: 1,
            db: None,
            tenant_key: Some(tenant_key.to_string()),
            schema,
            endpoints: None,
            relations: None,
            indexes: None,
        }
    }

    fn resource_without_tenant_key() -> ResourceDefinition {
        let mut schema = IndexMap::new();
        schema.insert("id".to_string(), uuid_field());
        ResourceDefinition {
            resource: "agents".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: None,
            relations: None,
            indexes: None,
        }
    }

    // --- resolve_tenant_id behavior ---

    #[test]
    fn resolve_tenant_id_populates_from_user_when_resource_has_tenant_key() {
        let resource = resource_with_tenant_key("org_id");
        let user = user_with_tenant("org-abc");
        let tenant_id = resolve_tenant_id(&resource, Some(&user));
        assert_eq!(tenant_id, Some("org-abc".to_string()));
    }

    #[test]
    fn resolve_tenant_id_is_none_when_resource_has_no_tenant_key() {
        let resource = resource_without_tenant_key();
        let user = user_with_tenant("org-abc");
        // Even though the user carries a tenant claim, no tenant_key → no scoping.
        let tenant_id = resolve_tenant_id(&resource, Some(&user));
        assert_eq!(tenant_id, None);
    }

    #[test]
    fn resolve_tenant_id_is_none_when_user_has_no_tenant_claim() {
        let resource = resource_with_tenant_key("org_id");
        let user = AuthenticatedUser {
            sub: "user-1".to_string(),
            role: "super_admin".to_string(),
            tenant_id: None,
        };
        let tenant_id = resolve_tenant_id(&resource, Some(&user));
        assert_eq!(tenant_id, None);
    }

    // --- Context: Clone + actix extensions round-trip ---

    #[tokio::test]
    async fn context_clone_round_trips_through_extensions() {
        // Build a Context with tenant_id and a session entry.
        let mut ctx = Context {
            input: serde_json::Map::new(),
            data: None,
            user: Some(user_with_tenant("org-1")),
            pool: test_pool(),
            headers: HashMap::new(),
            response_headers: vec![],
            tenant_id: Some("org-1".to_string()),
            session: serde_json::Map::new(),
            response_extras: serde_json::Map::new(),
        };
        ctx.session
            .insert("ran".to_string(), serde_json::json!(true));

        // Simulate what routes.rs does: stash the context in the extensions of a
        // minimal actix HttpRequest built from scratch.
        let req = actix_web::test::TestRequest::post()
            .uri("/agents/1/regenerate_secret")
            .to_http_request();
        req.extensions_mut().insert(ctx);

        // Now simulate what the custom handler does: pull it back out.
        let retrieved = req.extensions().get::<Context>().cloned();
        assert!(
            retrieved.is_some(),
            "Context should be retrievable from extensions"
        );
        let ctx_out = retrieved.unwrap();
        assert_eq!(ctx_out.tenant_id, Some("org-1".to_string()));
        assert_eq!(ctx_out.session["ran"], serde_json::json!(true));
        assert!(ctx_out.user.is_some());
        assert_eq!(
            ctx_out.user.as_ref().unwrap().tenant_id.as_deref(),
            Some("org-1")
        );
    }

    #[test]
    fn context_without_before_controller_produces_no_extension() {
        // When no before-controller is declared the runtime never inserts a Context.
        // Verify that extensions().get::<Context>() returns None in that case.
        let req = actix_web::test::TestRequest::get()
            .uri("/agents")
            .to_http_request();
        let ctx = req.extensions().get::<Context>().cloned();
        assert!(
            ctx.is_none(),
            "no Context should be present when no before-controller ran"
        );
    }

    // --- Custom-handler body extraction (regression test for v0.11.2) ---

    #[actix_web::test]
    async fn custom_handler_can_read_request_body() {
        // Regression test for the v0.11.1 bug: actix-web only extracts the
        // request payload when an extractor is declared in the closure
        // argument list. v0.11.1's dispatch closure had only (req, state),
        // so ServiceRequest.payload was dropped and req.take_payload()
        // returned Payload::None unconditionally — making POST/PUT custom
        // handlers unable to read their bodies.
        //
        // The fix adds `body: web::Bytes` to the closure and stashes it in
        // req.extensions_mut() so the handler can read it via
        // req.extensions().get::<web::Bytes>().cloned().
        use actix_web::{
            test::{call_service, init_service, TestRequest},
            web, App,
        };
        use indexmap::IndexMap;
        use shaperail_core::{
            EndpointSpec, FieldSchema, FieldType, HttpMethod, ResourceDefinition,
        };
        use std::sync::Arc;

        // Resource with a single custom POST endpoint named "echo".
        let mut endpoints = IndexMap::new();
        endpoints.insert(
            "echo".to_string(),
            EndpointSpec {
                method: Some(HttpMethod::Post),
                path: Some("/journal_entries".to_string()),
                handler: Some("echo_body".to_string()),
                ..Default::default()
            },
        );
        let mut schema = IndexMap::new();
        schema.insert(
            "id".to_string(),
            FieldSchema {
                field_type: FieldType::Uuid,
                primary: true,
                generated: true,
                required: false,
                unique: false,
                nullable: false,
                reference: None,
                min: None,
                max: None,
                format: None,
                values: None,
                default: None,
                sensitive: false,
                search: false,
                items: None,
                transient: false,
            },
        );
        let resource = ResourceDefinition {
            resource: "journal".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: Some(endpoints),
            relations: None,
            indexes: None,
        };

        // Custom handler reads the body bytes from req.extensions() and
        // echoes back its length + the content. If the bug were still
        // present, body_len would be 0 and content would be empty.
        let handler: super::super::custom::CustomHandlerFn = Arc::new(|req, _state, _r, _ep| {
            // Extract from req synchronously — HttpRequest contains Rc internally
            // and is therefore !Send, so it cannot cross an async-await boundary.
            // Move only owned data (the Bytes) into the async block.
            let body = req
                .extensions()
                .get::<web::Bytes>()
                .cloned()
                .unwrap_or_default();
            Box::pin(async move {
                let body_len = body.len();
                let content = String::from_utf8_lossy(&body).to_string();
                actix_web::HttpResponse::Ok().json(serde_json::json!({
                    "body_len": body_len,
                    "content": content,
                }))
            })
        });
        let mut custom_handlers: super::super::custom::CustomHandlerMap =
            std::collections::HashMap::new();
        custom_handlers.insert(
            super::super::custom::handler_key("journal", "echo"),
            handler,
        );

        let mut state = super::super::crud::AppState::new(test_pool(), vec![resource.clone()]);
        state.custom_handlers = Some(custom_handlers);
        let state = Arc::new(state);

        // Spin up the actix App with the routes registered.
        let app = init_service(
            App::new()
                .app_data(web::Data::new(state.clone()))
                .configure(|cfg| {
                    super::super::routes::register_resource(cfg, &resource, state.clone());
                }),
        )
        .await;

        // POST a non-trivial JSON body. v0.11.1 would echo body_len: 0;
        // v0.11.2 must echo the actual posted bytes.
        let payload = serde_json::json!({"description": "ate lunch", "amount": 12.50});
        let payload_bytes = serde_json::to_vec(&payload).unwrap();
        let req = TestRequest::post()
            .uri("/v1/journal_entries")
            .insert_header(("content-type", "application/json"))
            .set_payload(payload_bytes.clone())
            .to_request();
        let resp = call_service(&app, req).await;
        assert!(
            resp.status().is_success(),
            "expected 2xx, got {:?}",
            resp.status()
        );

        let body = actix_web::test::read_body(resp).await;
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let body_len = parsed["body_len"].as_u64().unwrap_or(0);
        assert_eq!(
            body_len as usize,
            payload_bytes.len(),
            "custom handler must see the full request body, got {} bytes",
            body_len
        );
        assert!(
            parsed["content"]
                .as_str()
                .unwrap_or("")
                .contains("ate lunch"),
            "body content should round-trip; got {parsed:?}"
        );
    }
}
