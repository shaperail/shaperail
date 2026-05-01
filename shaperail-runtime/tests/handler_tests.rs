// Handler integration tests using actix-web's test utilities.
// These tests verify response shapes, status codes, and validation behavior
// without a real database. They test the handler logic + serialization layer.

#[cfg(test)]
mod handler_unit_tests {
    use indexmap::IndexMap;
    use shaperail_core::*;
    use shaperail_runtime::handlers::response;

    fn test_resource() -> ResourceDefinition {
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
        schema.insert(
            "name".to_string(),
            FieldSchema {
                field_type: FieldType::String,
                primary: false,
                generated: false,
                required: true,
                unique: false,
                nullable: false,
                reference: None,
                min: Some(serde_json::json!(1)),
                max: Some(serde_json::json!(200)),
                format: None,
                values: None,
                default: None,
                sensitive: false,
                search: false,
                items: None,
                transient: false,
            },
        );
        schema.insert(
            "email".to_string(),
            FieldSchema {
                field_type: FieldType::String,
                primary: false,
                generated: false,
                required: true,
                unique: true,
                nullable: false,
                reference: None,
                min: None,
                max: None,
                format: Some("email".to_string()),
                values: None,
                default: None,
                sensitive: false,
                search: false,
                items: None,
                transient: false,
            },
        );
        schema.insert(
            "role".to_string(),
            FieldSchema {
                field_type: FieldType::Enum,
                primary: false,
                generated: false,
                required: false,
                unique: false,
                nullable: false,
                reference: None,
                min: None,
                max: None,
                format: None,
                values: Some(vec![
                    "admin".to_string(),
                    "member".to_string(),
                    "viewer".to_string(),
                ]),
                default: Some(serde_json::json!("member")),
                sensitive: false,
                search: false,
                items: None,
                transient: false,
            },
        );
        schema.insert(
            "org_id".to_string(),
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
            },
        );
        schema.insert(
            "created_at".to_string(),
            FieldSchema {
                field_type: FieldType::Timestamp,
                primary: false,
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
        schema.insert(
            "updated_at".to_string(),
            FieldSchema {
                field_type: FieldType::Timestamp,
                primary: false,
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

        let mut endpoints = IndexMap::new();
        endpoints.insert(
            "list".to_string(),
            EndpointSpec {
                method: Some(HttpMethod::Get),
                path: Some("/users".to_string()),
                auth: Some(AuthRule::Roles(vec![
                    "member".to_string(),
                    "admin".to_string(),
                ])),
                input: None,
                filters: Some(vec!["role".to_string(), "org_id".to_string()]),
                search: Some(vec!["name".to_string(), "email".to_string()]),
                pagination: Some(PaginationStyle::Cursor),
                sort: None,
                cache: Some(CacheSpec {
                    ttl: 60,
                    invalidate_on: None,
                }),
                controller: None,
                events: None,
                jobs: None,
                subscribers: None,
                handler: None,
                upload: None,
                rate_limit: None,
                soft_delete: false,
            },
        );
        endpoints.insert(
            "create".to_string(),
            EndpointSpec {
                method: Some(HttpMethod::Post),
                path: Some("/users".to_string()),
                auth: Some(AuthRule::Roles(vec!["admin".to_string()])),
                input: Some(vec![
                    "email".to_string(),
                    "name".to_string(),
                    "role".to_string(),
                    "org_id".to_string(),
                ]),
                filters: None,
                search: None,
                pagination: None,
                sort: None,
                cache: None,
                controller: None,
                events: None,
                jobs: None,
                subscribers: None,
                handler: None,
                upload: None,
                rate_limit: None,
                soft_delete: false,
            },
        );
        endpoints.insert(
            "update".to_string(),
            EndpointSpec {
                method: Some(HttpMethod::Patch),
                path: Some("/users/:id".to_string()),
                auth: Some(AuthRule::Roles(vec![
                    "admin".to_string(),
                    "owner".to_string(),
                ])),
                input: Some(vec!["name".to_string(), "role".to_string()]),
                filters: None,
                search: None,
                pagination: None,
                sort: None,
                cache: None,
                controller: None,
                events: None,
                jobs: None,
                subscribers: None,
                handler: None,
                upload: None,
                rate_limit: None,
                soft_delete: false,
            },
        );
        endpoints.insert(
            "delete".to_string(),
            EndpointSpec {
                method: Some(HttpMethod::Delete),
                path: Some("/users/:id".to_string()),
                auth: Some(AuthRule::Roles(vec!["admin".to_string()])),
                input: None,
                filters: None,
                search: None,
                pagination: None,
                sort: None,
                cache: None,
                controller: None,
                events: None,
                jobs: None,
                subscribers: None,
                handler: None,
                upload: None,
                rate_limit: None,
                soft_delete: true,
            },
        );

        ResourceDefinition {
            resource: "users".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
            schema,
            endpoints: Some(endpoints),
            relations: None,
            indexes: None,
        }
    }

    // -- Response envelope tests --

    #[test]
    fn single_response_envelope() {
        let data = serde_json::json!({"id": "abc-123", "name": "Alice"});
        let resp = response::single(data.clone());
        assert_eq!(resp.status(), 200);
    }

    #[test]
    fn list_response_envelope() {
        let data = vec![
            serde_json::json!({"id": "1", "name": "Alice"}),
            serde_json::json!({"id": "2", "name": "Bob"}),
        ];
        let meta = serde_json::json!({"cursor": null, "has_more": false});
        let resp = response::list(data, meta);
        assert_eq!(resp.status(), 200);
    }

    #[test]
    fn created_response_returns_201() {
        let data = serde_json::json!({"id": "abc-123", "name": "Alice"});
        let resp = response::created(data);
        assert_eq!(resp.status(), 201);
    }

    #[test]
    fn no_content_response_returns_204() {
        let resp = response::no_content();
        assert_eq!(resp.status(), 204);
    }

    #[test]
    fn bulk_response_envelope() {
        let data = vec![
            serde_json::json!({"id": "1"}),
            serde_json::json!({"id": "2"}),
        ];
        let resp = response::bulk(data);
        assert_eq!(resp.status(), 200);
    }

    // -- Validation tests (422 cases) --

    #[test]
    fn validation_missing_required_returns_422() {
        let resource = test_resource();
        let mut data = serde_json::Map::new();
        data.insert("email".to_string(), serde_json::json!("alice@example.com"));
        data.insert(
            "org_id".to_string(),
            serde_json::json!("550e8400-e29b-41d4-a716-446655440000"),
        );
        // name is missing

        let result = shaperail_runtime::handlers::validate::validate_input(&data, &resource);
        assert!(result.is_err());
        if let Err(ShaperailError::Validation(errors)) = result {
            assert!(errors.iter().any(|e| e.field == "name"));
            // ShaperailError::Validation maps to 422
            assert_eq!(
                ShaperailError::Validation(errors).status(),
                actix_web::http::StatusCode::UNPROCESSABLE_ENTITY
            );
        }
    }

    #[test]
    fn validation_invalid_enum_returns_422() {
        let resource = test_resource();
        let mut data = serde_json::Map::new();
        data.insert("name".to_string(), serde_json::json!("Alice"));
        data.insert("email".to_string(), serde_json::json!("alice@example.com"));
        data.insert(
            "org_id".to_string(),
            serde_json::json!("550e8400-e29b-41d4-a716-446655440000"),
        );
        data.insert("role".to_string(), serde_json::json!("superadmin"));

        let result = shaperail_runtime::handlers::validate::validate_input(&data, &resource);
        assert!(result.is_err());
        if let Err(ShaperailError::Validation(errors)) = result {
            assert!(errors
                .iter()
                .any(|e| e.field == "role" && e.code == "invalid_enum"));
        }
    }

    #[test]
    fn validation_invalid_email_returns_422() {
        let resource = test_resource();
        let mut data = serde_json::Map::new();
        data.insert("name".to_string(), serde_json::json!("Alice"));
        data.insert("email".to_string(), serde_json::json!("not-email"));
        data.insert(
            "org_id".to_string(),
            serde_json::json!("550e8400-e29b-41d4-a716-446655440000"),
        );

        let result = shaperail_runtime::handlers::validate::validate_input(&data, &resource);
        assert!(result.is_err());
    }

    #[test]
    fn validation_string_too_short_returns_422() {
        let resource = test_resource();
        let mut data = serde_json::Map::new();
        data.insert("name".to_string(), serde_json::json!(""));
        data.insert("email".to_string(), serde_json::json!("alice@example.com"));
        data.insert(
            "org_id".to_string(),
            serde_json::json!("550e8400-e29b-41d4-a716-446655440000"),
        );

        let result = shaperail_runtime::handlers::validate::validate_input(&data, &resource);
        assert!(result.is_err());
        if let Err(ShaperailError::Validation(errors)) = result {
            assert!(errors
                .iter()
                .any(|e| e.field == "name" && e.code == "too_short"));
        }
    }

    #[test]
    fn validation_valid_input_passes() {
        let resource = test_resource();
        let mut data = serde_json::Map::new();
        data.insert("name".to_string(), serde_json::json!("Alice"));
        data.insert("email".to_string(), serde_json::json!("alice@example.com"));
        data.insert(
            "org_id".to_string(),
            serde_json::json!("550e8400-e29b-41d4-a716-446655440000"),
        );
        data.insert("role".to_string(), serde_json::json!("admin"));

        let result = shaperail_runtime::handlers::validate::validate_input(&data, &resource);
        assert!(result.is_ok());
    }

    // -- Error response shape tests (401, 403, 404) --

    #[test]
    fn error_401_shape() {
        let err = ShaperailError::Unauthorized;
        assert_eq!(err.status(), actix_web::http::StatusCode::UNAUTHORIZED);
        let body = err.to_error_body("req-001");
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
        assert_eq!(body["error"]["status"], 401);
    }

    #[test]
    fn error_403_shape() {
        let err = ShaperailError::Forbidden;
        assert_eq!(err.status(), actix_web::http::StatusCode::FORBIDDEN);
        let body = err.to_error_body("req-002");
        assert_eq!(body["error"]["code"], "FORBIDDEN");
        assert_eq!(body["error"]["status"], 403);
    }

    #[test]
    fn error_404_shape() {
        let err = ShaperailError::NotFound;
        assert_eq!(err.status(), actix_web::http::StatusCode::NOT_FOUND);
        let body = err.to_error_body("req-003");
        assert_eq!(body["error"]["code"], "NOT_FOUND");
        assert_eq!(body["error"]["status"], 404);
    }

    #[test]
    fn error_422_shape_with_details() {
        let errors = vec![
            FieldError {
                field: "email".to_string(),
                message: "is required".to_string(),
                code: "required".to_string(),
            },
            FieldError {
                field: "name".to_string(),
                message: "too short".to_string(),
                code: "too_short".to_string(),
            },
        ];
        let err = ShaperailError::Validation(errors);
        assert_eq!(
            err.status(),
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY
        );
        let body = err.to_error_body("req-004");
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
        assert_eq!(body["error"]["status"], 422);
        assert!(body["error"]["details"].is_array());
        assert_eq!(
            body["error"]["details"].as_array().map(|a| a.len()),
            Some(2)
        );
    }

    // -- Field selection tests --

    #[test]
    fn field_selection_trims_response() {
        let data = serde_json::json!({
            "id": "abc-123",
            "name": "Alice",
            "email": "alice@example.com",
            "role": "admin",
            "created_at": "2024-01-01T00:00:00Z"
        });

        let fields = vec!["name".to_string(), "email".to_string()];
        let result = response::select_fields(&data, &fields);

        assert!(result.get("name").is_some());
        assert!(result.get("email").is_some());
        assert!(result.get("id").is_none());
        assert!(result.get("role").is_none());
        assert!(result.get("created_at").is_none());
    }

    #[test]
    fn field_selection_empty_returns_all() {
        let data = serde_json::json!({
            "id": "abc-123",
            "name": "Alice"
        });

        let result = response::select_fields(&data, &[]);
        assert_eq!(result.as_object().map(|o| o.len()), Some(2));
    }

    // -- UUID validation tests --

    #[test]
    fn invalid_uuid_in_path_returns_422() {
        let err = ShaperailError::Validation(vec![FieldError {
            field: "id".to_string(),
            message: "Invalid UUID: not-a-uuid".to_string(),
            code: "invalid_uuid".to_string(),
        }]);
        assert_eq!(
            err.status(),
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn valid_uuid_in_path_parses() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let result = uuid::Uuid::parse_str(uuid_str);
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod auth_tests {
    use shaperail_core::{AuthRule, ShaperailError};
    use shaperail_runtime::auth::extractor::AuthenticatedUser;
    use shaperail_runtime::auth::jwt::JwtConfig;
    use shaperail_runtime::auth::rbac;

    fn jwt_config() -> JwtConfig {
        JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400)
    }

    fn admin_user() -> AuthenticatedUser {
        AuthenticatedUser {
            id: "user-1".to_string(),
            role: "admin".to_string(),
            tenant_id: None,
        }
    }

    fn member_user() -> AuthenticatedUser {
        AuthenticatedUser {
            id: "user-2".to_string(),
            role: "member".to_string(),
            tenant_id: None,
        }
    }

    fn viewer_user() -> AuthenticatedUser {
        AuthenticatedUser {
            id: "user-3".to_string(),
            role: "viewer".to_string(),
            tenant_id: None,
        }
    }

    // -- JWT tests --

    #[test]
    fn jwt_encode_decode_roundtrip() {
        let cfg = jwt_config();
        let token = cfg.encode_access("user-1", "admin").unwrap();
        let claims = cfg.decode(&token).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.role, "admin");
        assert_eq!(claims.token_type, "access");
    }

    #[test]
    fn jwt_refresh_token_roundtrip() {
        let cfg = jwt_config();
        let token = cfg.encode_refresh("user-1", "admin").unwrap();
        let claims = cfg.decode(&token).unwrap();
        assert_eq!(claims.token_type, "refresh");
    }

    #[test]
    fn jwt_invalid_token_rejected() {
        let cfg = jwt_config();
        assert!(cfg.decode("garbage.token.value").is_err());
    }

    #[test]
    fn jwt_wrong_secret_rejected() {
        let cfg1 = jwt_config();
        let cfg2 = JwtConfig::new("different-secret-key-also-long-enough!", 3600, 86400);
        let token = cfg1.encode_access("user-1", "admin").unwrap();
        assert!(cfg2.decode(&token).is_err());
    }

    #[test]
    fn jwt_expired_token_rejected() {
        let cfg = JwtConfig::new("test-secret-key-at-least-32-bytes-long!", -120, -120);
        let token = cfg.encode_access("user-1", "admin").unwrap();
        assert!(cfg.decode(&token).is_err());
    }

    // -- RBAC tests --

    #[test]
    fn rbac_401_no_token() {
        let rule = AuthRule::Roles(vec!["admin".to_string()]);
        let result = rbac::enforce(Some(&rule), None);
        assert!(matches!(result, Err(ShaperailError::Unauthorized)));
    }

    #[test]
    fn rbac_403_wrong_role() {
        let rule = AuthRule::Roles(vec!["admin".to_string()]);
        let result = rbac::enforce(Some(&rule), Some(&viewer_user()));
        assert!(matches!(result, Err(ShaperailError::Forbidden)));
    }

    #[test]
    fn rbac_200_correct_role() {
        let rule = AuthRule::Roles(vec!["admin".to_string(), "member".to_string()]);
        assert!(rbac::enforce(Some(&rule), Some(&admin_user())).is_ok());
        assert!(rbac::enforce(Some(&rule), Some(&member_user())).is_ok());
    }

    #[test]
    fn rbac_owner_allows_own() {
        let user = AuthenticatedUser {
            id: "user-1".to_string(),
            role: "member".to_string(),
            tenant_id: None,
        };
        let record = serde_json::json!({"id": "rec-1", "created_by": "user-1"});
        assert!(rbac::check_owner(&user, &record).is_ok());
    }

    #[test]
    fn rbac_owner_blocks_other() {
        let user = AuthenticatedUser {
            id: "user-1".to_string(),
            role: "member".to_string(),
            tenant_id: None,
        };
        let record = serde_json::json!({"id": "rec-1", "created_by": "user-999"});
        assert!(matches!(
            rbac::check_owner(&user, &record),
            Err(ShaperailError::Forbidden)
        ));
    }

    #[test]
    fn rbac_public_allows_unauthenticated() {
        assert!(rbac::enforce(Some(&AuthRule::Public), None).is_ok());
    }

    #[test]
    fn rbac_owner_requires_auth() {
        let result = rbac::enforce(Some(&AuthRule::Owner), None);
        assert!(matches!(result, Err(ShaperailError::Unauthorized)));
    }

    #[test]
    fn rbac_roles_with_owner_passes_for_any_authenticated() {
        let rule = AuthRule::Roles(vec!["admin".to_string(), "owner".to_string()]);
        // Admin matches directly
        assert!(rbac::enforce(Some(&rule), Some(&admin_user())).is_ok());
        // Viewer doesn't match admin, but "owner" in list allows through for ownership check
        assert!(rbac::enforce(Some(&rule), Some(&viewer_user())).is_ok());
    }

    #[test]
    fn rbac_needs_owner_check_identifies_correctly() {
        // Owner rule always needs check
        assert!(rbac::needs_owner_check(
            Some(&AuthRule::Owner),
            Some(&admin_user())
        ));

        // Roles with "owner" — admin is in the list, no check needed
        let rule = AuthRule::Roles(vec!["admin".to_string(), "owner".to_string()]);
        assert!(!rbac::needs_owner_check(Some(&rule), Some(&admin_user())));

        // Viewer is NOT in roles list, needs owner check
        assert!(rbac::needs_owner_check(Some(&rule), Some(&viewer_user())));

        // Public never needs check
        assert!(!rbac::needs_owner_check(Some(&AuthRule::Public), None));
    }

    // -- API Key tests --

    #[test]
    fn api_key_store_insert_and_lookup() {
        let mut store = shaperail_runtime::auth::api_key::ApiKeyStore::new();
        store.insert(
            "sk-test-key".to_string(),
            "user-1".to_string(),
            "admin".to_string(),
        );
        let user = store.lookup("sk-test-key").unwrap();
        assert_eq!(user.id, "user-1");
        assert_eq!(user.role, "admin");
    }

    #[test]
    fn api_key_store_invalid_key() {
        let store = shaperail_runtime::auth::api_key::ApiKeyStore::new();
        assert!(store.lookup("invalid-key").is_none());
    }

    // -- Token pair tests --

    #[test]
    fn token_pair_serializes() {
        let pair = shaperail_runtime::auth::TokenPair {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
        };
        let json = serde_json::to_value(&pair).unwrap();
        assert_eq!(json["token_type"], "Bearer");
        assert_eq!(json["expires_in"], 3600);
    }

    // -- Rate limit config tests --

    #[test]
    fn rate_limit_config_defaults() {
        let cfg = shaperail_runtime::auth::RateLimitConfig::default();
        assert_eq!(cfg.max_requests, 100);
        assert_eq!(cfg.window_secs, 60);
    }

    #[test]
    fn rate_limit_key_generation() {
        let key_ip = shaperail_runtime::auth::RateLimiter::key_for("1.2.3.4", None);
        assert_eq!(key_ip, "ip:1.2.3.4");

        let key_user = shaperail_runtime::auth::RateLimiter::key_for("1.2.3.4", Some("u1"));
        assert_eq!(key_user, "user:u1");
    }
}

#[cfg(test)]
mod cache_tests {
    use std::collections::HashMap;

    use shaperail_runtime::cache::RedisCache;

    #[test]
    fn cache_key_format_matches_spec() {
        let mut params = HashMap::new();
        params.insert("filter[role]".to_string(), "admin".to_string());
        let key = RedisCache::build_key("users", "list", &params, "member");

        // Format: shaperail:<resource>:<endpoint>:<hash>:<tenant_id>:<role>
        let parts: Vec<&str> = key.split(':').collect();
        assert_eq!(parts[0], "shaperail");
        assert_eq!(parts[1], "users");
        assert_eq!(parts[2], "list");
        // parts[3] is the query hash
        assert_eq!(parts[4], "_"); // default tenant placeholder (M18)
        assert_eq!(parts[5], "member");
    }

    #[test]
    fn cache_key_different_params_different_hash() {
        let mut p1 = HashMap::new();
        p1.insert("filter[role]".to_string(), "admin".to_string());

        let mut p2 = HashMap::new();
        p2.insert("filter[role]".to_string(), "member".to_string());

        let k1 = RedisCache::build_key("users", "list", &p1, "member");
        let k2 = RedisCache::build_key("users", "list", &p2, "member");

        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_different_roles_different_key() {
        let params = HashMap::new();
        let k1 = RedisCache::build_key("users", "list", &params, "admin");
        let k2 = RedisCache::build_key("users", "list", &params, "member");
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_same_params_same_hash() {
        let mut p1 = HashMap::new();
        p1.insert("a".to_string(), "1".to_string());
        p1.insert("b".to_string(), "2".to_string());

        let mut p2 = HashMap::new();
        p2.insert("b".to_string(), "2".to_string());
        p2.insert("a".to_string(), "1".to_string());

        let k1 = RedisCache::build_key("users", "list", &p1, "admin");
        let k2 = RedisCache::build_key("users", "list", &p2, "admin");
        assert_eq!(k1, k2);
    }

    #[test]
    fn cache_key_empty_params() {
        let params = HashMap::new();
        let key = RedisCache::build_key("orders", "get", &params, "anonymous");
        assert!(key.starts_with("shaperail:orders:get:"));
        assert!(key.ends_with(":anonymous"));
    }

    #[test]
    fn cache_pool_creation() {
        let pool = shaperail_runtime::cache::create_redis_pool("redis://localhost:6379");
        assert!(pool.is_ok());
    }
}
