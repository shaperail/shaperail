//! Integration tests for the shaperail-runtime database layer.
//!
//! Uses `#[sqlx::test]` macro for auto-rollback and isolated DB per test.
//! Requires a running PostgreSQL instance.
//! Set DATABASE_URL env var or run `docker compose up -d` first.
//!
//! Run with: cargo test -p shaperail-runtime --test db_integration

use std::sync::Arc;

use actix_web::{body::to_bytes, http::StatusCode, test::TestRequest, web};
use indexmap::IndexMap;
use shaperail_core::{EndpointSpec, FieldSchema, FieldType, HttpMethod, ResourceDefinition};
use shaperail_runtime::db::{
    health_check, FilterParam, FilterSet, PageRequest, ResourceQuery, SearchParam, SortParam,
};
use shaperail_runtime::handlers::crud::{handle_delete, AppState};
use shaperail_runtime::observability::MetricsState;

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
            max: Some(serde_json::json!(255)),
            format: Some("email".to_string()),
            values: None,
            default: None,
            sensitive: false,
            search: true,
            items: None,
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
            search: true,
            items: None,
        },
    );
    schema.insert(
        "role".to_string(),
        FieldSchema {
            field_type: FieldType::Enum,
            primary: false,
            generated: false,
            required: true,
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
            reference: None,
            min: None,
            max: None,
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: false,
            items: None,
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
        },
    );
    schema.insert(
        "deleted_at".to_string(),
        FieldSchema {
            field_type: FieldType::Timestamp,
            primary: false,
            generated: false,
            required: false,
            unique: false,
            nullable: true,
            reference: None,
            min: None,
            max: None,
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: false,
            items: None,
        },
    );

    ResourceDefinition {
        resource: "test_users".to_string(),
        version: 1,
        db: None,
        tenant_key: None,
        schema,
        endpoints: None,
        relations: None,
        indexes: None,
    }
}

fn test_resource_with_soft_delete_endpoint() -> ResourceDefinition {
    let mut resource = test_resource();
    let mut endpoints = IndexMap::new();
    endpoints.insert(
        "delete".to_string(),
        EndpointSpec {
            method: HttpMethod::Delete,
            path: "/test_users/:id".to_string(),
            auth: None,
            input: None,
            filters: None,
            search: None,
            pagination: None,
            sort: None,
            cache: None,
            controller: None,
            events: None,
            jobs: None,
            upload: None,
            soft_delete: true,
        },
    );
    resource.endpoints = Some(endpoints);
    resource
}

// All tests use #[sqlx::test] which provides:
// - An isolated database per test (auto-created, auto-dropped)
// - Auto-rollback on completion
// - Migrations from the `migrations` arg are applied before each test

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_health_check(pool: sqlx::PgPool) {
    health_check(&pool).await.expect("Health check failed");
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_insert_and_find_by_id(pool: sqlx::PgPool) {
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();
    let mut data = serde_json::Map::new();
    data.insert("email".to_string(), serde_json::json!("test@example.com"));
    data.insert("name".to_string(), serde_json::json!("Test User"));
    data.insert("role".to_string(), serde_json::json!("admin"));
    data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));

    let row = q.insert(&data).await.expect("Insert failed");
    let id_str = row.0["id"].as_str().expect("No id in row");
    let id = uuid::Uuid::parse_str(id_str).expect("Invalid UUID");

    let found = q.find_by_id(&id).await.expect("Find by ID failed");
    assert_eq!(found.0["email"], "test@example.com");
    assert_eq!(found.0["name"], "Test User");
    assert_eq!(found.0["role"], "admin");
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_update_by_id(pool: sqlx::PgPool) {
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();
    let mut data = serde_json::Map::new();
    data.insert("email".to_string(), serde_json::json!("update@example.com"));
    data.insert("name".to_string(), serde_json::json!("Original Name"));
    data.insert("role".to_string(), serde_json::json!("member"));
    data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));

    let row = q.insert(&data).await.expect("Insert failed");
    let id_str = row.0["id"].as_str().unwrap();
    let id = uuid::Uuid::parse_str(id_str).unwrap();

    let mut update_data = serde_json::Map::new();
    update_data.insert("name".to_string(), serde_json::json!("Updated Name"));

    let updated = q
        .update_by_id(&id, &update_data)
        .await
        .expect("Update failed");
    assert_eq!(updated.0["name"], "Updated Name");
    assert_eq!(updated.0["email"], "update@example.com");
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_soft_delete(pool: sqlx::PgPool) {
    let resource = test_resource_with_soft_delete_endpoint();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();
    let mut data = serde_json::Map::new();
    data.insert(
        "email".to_string(),
        serde_json::json!("softdel@example.com"),
    );
    data.insert("name".to_string(), serde_json::json!("Soft Delete"));
    data.insert("role".to_string(), serde_json::json!("viewer"));
    data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));

    let row = q.insert(&data).await.expect("Insert failed");
    let id = uuid::Uuid::parse_str(row.0["id"].as_str().unwrap()).unwrap();

    let deleted = q.soft_delete_by_id(&id).await.expect("Soft delete failed");
    assert!(!deleted.0["deleted_at"].is_null());

    // Double soft-delete should fail (already deleted)
    let result = q.soft_delete_by_id(&id).await;
    assert!(result.is_err());
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_update_by_id_ignores_soft_deleted_rows(pool: sqlx::PgPool) {
    let resource = test_resource_with_soft_delete_endpoint();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();
    let mut data = serde_json::Map::new();
    data.insert(
        "email".to_string(),
        serde_json::json!("softupdate@example.com"),
    );
    data.insert("name".to_string(), serde_json::json!("Soft Update"));
    data.insert("role".to_string(), serde_json::json!("viewer"));
    data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));

    let row = q.insert(&data).await.expect("Insert failed");
    let id = uuid::Uuid::parse_str(row.0["id"].as_str().unwrap()).unwrap();
    q.soft_delete_by_id(&id).await.expect("Soft delete failed");

    let mut update_data = serde_json::Map::new();
    update_data.insert("name".to_string(), serde_json::json!("Should not update"));

    let result = q.update_by_id(&id, &update_data).await;
    assert!(matches!(
        result,
        Err(shaperail_core::ShaperailError::NotFound)
    ));
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_handle_delete_soft_delete_returns_no_content(pool: sqlx::PgPool) {
    let resource = test_resource_with_soft_delete_endpoint();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();
    let mut data = serde_json::Map::new();
    data.insert(
        "email".to_string(),
        serde_json::json!("deletehandler@example.com"),
    );
    data.insert("name".to_string(), serde_json::json!("Delete Handler"));
    data.insert("role".to_string(), serde_json::json!("admin"));
    data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));

    let row = q.insert(&data).await.expect("Insert failed");
    let id = row.0["id"].as_str().unwrap().to_string();
    let endpoint = resource
        .endpoints
        .as_ref()
        .and_then(|endpoints| endpoints.get("delete"))
        .cloned()
        .expect("delete endpoint");
    let state = Arc::new(AppState {
        pool: pool.clone(),
        resources: vec![resource.clone()],
        stores: None,
        controllers: None,
        jwt_config: None,
        cache: None,
        event_emitter: None,
        job_queue: None,
        metrics: Some(MetricsState::new().expect("metrics state")),
        wasm_runtime: None,
        event_bus: tokio::sync::broadcast::channel(16).0,
    });

    let response = handle_delete(
        TestRequest::delete().to_http_request(),
        web::Data::new(state),
        web::Data::new(Arc::new(resource)),
        web::Data::new(Arc::new(endpoint)),
        web::Path::from(id),
    )
    .await
    .expect("Delete handler failed");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let body = to_bytes(response.into_body()).await.expect("Read body");
    assert!(body.is_empty());
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_hard_delete(pool: sqlx::PgPool) {
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();
    let mut data = serde_json::Map::new();
    data.insert(
        "email".to_string(),
        serde_json::json!("harddel@example.com"),
    );
    data.insert("name".to_string(), serde_json::json!("Hard Delete"));
    data.insert("role".to_string(), serde_json::json!("member"));
    data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));

    let row = q.insert(&data).await.expect("Insert failed");
    let id = uuid::Uuid::parse_str(row.0["id"].as_str().unwrap()).unwrap();

    q.hard_delete_by_id(&id).await.expect("Hard delete failed");

    // Should be gone
    let result = q.find_by_id(&id).await;
    assert!(result.is_err());
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_find_all_with_filters(pool: sqlx::PgPool) {
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();

    for (email, role) in [
        ("filter1@example.com", "admin"),
        ("filter2@example.com", "member"),
        ("filter3@example.com", "viewer"),
    ] {
        let mut data = serde_json::Map::new();
        data.insert("email".to_string(), serde_json::json!(email));
        data.insert("name".to_string(), serde_json::json!("Filter User"));
        data.insert("role".to_string(), serde_json::json!(role));
        data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));
        q.insert(&data).await.expect("Insert failed");
    }

    let filters = FilterSet {
        filters: vec![FilterParam {
            field: "role".to_string(),
            value: "admin".to_string(),
        }],
    };
    let sort = SortParam::default();
    let page = PageRequest::Cursor {
        after: None,
        limit: 25,
    };

    let (rows, meta) = q
        .find_all(&filters, None, &sort, &page)
        .await
        .expect("Find all failed");
    assert!(rows.iter().all(|r| r.0["role"] == "admin"));
    assert!(!rows.is_empty());
    assert_eq!(meta["has_more"], false);
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_find_all_with_sort(pool: sqlx::PgPool) {
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();
    for (email, name) in [
        ("sort_c@example.com", "Charlie"),
        ("sort_a@example.com", "Alice"),
        ("sort_b@example.com", "Bob"),
    ] {
        let mut data = serde_json::Map::new();
        data.insert("email".to_string(), serde_json::json!(email));
        data.insert("name".to_string(), serde_json::json!(name));
        data.insert("role".to_string(), serde_json::json!("member"));
        data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));
        q.insert(&data).await.unwrap();
    }

    let filters = FilterSet {
        filters: vec![FilterParam {
            field: "org_id".to_string(),
            value: org_id.to_string(),
        }],
    };
    let sort = SortParam::parse("name", &["name".to_string(), "email".to_string()]);
    let page = PageRequest::Cursor {
        after: None,
        limit: 25,
    };

    let (rows, _) = q
        .find_all(&filters, None, &sort, &page)
        .await
        .expect("Find all failed");

    assert_eq!(rows.len(), 3);
    let names: Vec<&str> = rows.iter().filter_map(|r| r.0["name"].as_str()).collect();
    for window in names.windows(2) {
        assert!(
            window[0] <= window[1],
            "Not sorted: {} > {}",
            window[0],
            window[1]
        );
    }
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_find_all_cursor_pagination(pool: sqlx::PgPool) {
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();
    for i in 0..5 {
        let mut data = serde_json::Map::new();
        data.insert(
            "email".to_string(),
            serde_json::json!(format!("page{i}@example.com")),
        );
        data.insert("name".to_string(), serde_json::json!(format!("User {i}")));
        data.insert("role".to_string(), serde_json::json!("member"));
        data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));
        q.insert(&data).await.unwrap();
    }

    let filters = FilterSet::default();
    let sort = SortParam::default();

    // First page: limit 2
    let page = PageRequest::Cursor {
        after: None,
        limit: 2,
    };
    let (rows, meta) = q
        .find_all(&filters, None, &sort, &page)
        .await
        .expect("First page failed");
    assert_eq!(rows.len(), 2);
    assert_eq!(meta["has_more"], true);
    let cursor = meta["cursor"].as_str().unwrap().to_string();

    // Second page
    let page2 = PageRequest::Cursor {
        after: Some(cursor),
        limit: 2,
    };
    let (rows2, meta2) = q
        .find_all(&filters, None, &sort, &page2)
        .await
        .expect("Second page failed");
    assert_eq!(rows2.len(), 2);
    assert_eq!(meta2["has_more"], true);

    // No overlap between pages
    let ids1: Vec<&str> = rows.iter().filter_map(|r| r.0["id"].as_str()).collect();
    let ids2: Vec<&str> = rows2.iter().filter_map(|r| r.0["id"].as_str()).collect();
    for id in &ids2 {
        assert!(!ids1.contains(id), "Page overlap: {id}");
    }
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_find_all_offset_pagination(pool: sqlx::PgPool) {
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();
    for i in 0..5 {
        let mut data = serde_json::Map::new();
        data.insert(
            "email".to_string(),
            serde_json::json!(format!("offset{i}@example.com")),
        );
        data.insert("name".to_string(), serde_json::json!(format!("User {i}")));
        data.insert("role".to_string(), serde_json::json!("member"));
        data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));
        q.insert(&data).await.unwrap();
    }

    let filters = FilterSet::default();
    let sort = SortParam::default();

    let page = PageRequest::Offset {
        offset: 0,
        limit: 3,
    };
    let (rows, meta) = q
        .find_all(&filters, None, &sort, &page)
        .await
        .expect("Offset page failed");
    assert_eq!(rows.len(), 3);
    assert_eq!(meta["total"], 5);
    assert_eq!(meta["offset"], 0);
    assert_eq!(meta["limit"], 3);
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_find_by_id_not_found(pool: sqlx::PgPool) {
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);

    let result = q.find_by_id(&uuid::Uuid::new_v4()).await;
    assert!(result.is_err());
}

// --- SQL injection tests: user input is always bound as parameters, never concatenated into SQL ---

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_sql_injection_filter_value(pool: sqlx::PgPool) {
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();
    let mut data = serde_json::Map::new();
    data.insert("email".to_string(), serde_json::json!("safe@example.com"));
    data.insert("name".to_string(), serde_json::json!("Safe User"));
    data.insert("role".to_string(), serde_json::json!("member"));
    data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));
    q.insert(&data).await.expect("Insert failed");

    // Count rows before
    let (rows_before, _) = q
        .find_all(
            &FilterSet::default(),
            None,
            &SortParam::default(),
            &PageRequest::Cursor {
                after: None,
                limit: 100,
            },
        )
        .await
        .expect("Find all failed");
    let count_before = rows_before.len();

    // Attempt injection via filter value: should be bound as literal, no SQL executed
    let filters = FilterSet {
        filters: vec![
            FilterParam {
                field: "role".to_string(),
                value: "'; DROP TABLE test_users; --".to_string(),
            },
            FilterParam {
                field: "org_id".to_string(),
                value: org_id.to_string(),
            },
        ],
    };
    let result = q
        .find_all(
            &filters,
            None,
            &SortParam::default(),
            &PageRequest::Cursor {
                after: None,
                limit: 25,
            },
        )
        .await;
    assert!(
        result.is_ok(),
        "find_all should not crash on injection attempt"
    );
    let (rows_after, _) = result.expect("ok");
    // No row matches role = "'; DROP TABLE test_users; --"
    assert!(rows_after.is_empty());

    // Table must still exist with same row count
    let (rows_still, _) = q
        .find_all(
            &FilterSet::default(),
            None,
            &SortParam::default(),
            &PageRequest::Cursor {
                after: None,
                limit: 100,
            },
        )
        .await
        .expect("Find all still works");
    assert_eq!(
        rows_still.len(),
        count_before,
        "Table must be unchanged after filter injection attempt"
    );
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_sql_injection_search_term(pool: sqlx::PgPool) {
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);

    let org_id = uuid::Uuid::new_v4();
    let mut data = serde_json::Map::new();
    data.insert("email".to_string(), serde_json::json!("search@example.com"));
    data.insert("name".to_string(), serde_json::json!("Search User"));
    data.insert("role".to_string(), serde_json::json!("member"));
    data.insert("org_id".to_string(), serde_json::json!(org_id.to_string()));
    q.insert(&data).await.expect("Insert failed");

    let search = SearchParam {
        term: "1'; DELETE FROM test_users WHERE '1'='1".to_string(),
        fields: vec!["name".to_string(), "email".to_string()],
    };
    let result = q
        .find_all(
            &FilterSet::default(),
            Some(&search),
            &SortParam::default(),
            &PageRequest::Cursor {
                after: None,
                limit: 25,
            },
        )
        .await;
    assert!(
        result.is_ok(),
        "find_all with malicious search term should not crash"
    );
    let (rows, _) = result.expect("ok");
    // Search term is bound; no literal match, so may be empty
    assert!(rows.len() <= 1, "No extra rows from injection");

    // Table must still have our row
    let (all_rows, _) = q
        .find_all(
            &FilterSet::default(),
            None,
            &SortParam::default(),
            &PageRequest::Cursor {
                after: None,
                limit: 100,
            },
        )
        .await
        .expect("Find all still works");
    assert!(
        all_rows
            .iter()
            .any(|r| r.0["email"] == "search@example.com"),
        "Row must still exist after search injection attempt"
    );
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_sql_injection_sort_field(pool: sqlx::PgPool) {
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);

    // SortParam::parse uses allow-list; malicious field names are dropped
    let allowed = vec!["id".to_string(), "name".to_string(), "email".to_string()];
    let sort = SortParam::parse("id; DROP TABLE test_users; --", &allowed);
    assert!(
        sort.fields.is_empty() || sort.fields.iter().all(|f| allowed.contains(&f.field)),
        "Malicious sort field must not appear"
    );

    let result = q
        .find_all(
            &FilterSet::default(),
            None,
            &sort,
            &PageRequest::Cursor {
                after: None,
                limit: 25,
            },
        )
        .await;
    assert!(
        result.is_ok(),
        "find_all with parsed sort must not execute injected SQL"
    );
}

#[sqlx::test(migrations = "tests/fixtures/migrations")]
async fn test_sql_injection_cursor_invalid(pool: sqlx::PgPool) {
    use shaperail_runtime::db::decode_cursor;

    // Invalid base64 cursor must not be used in SQL
    let result = decode_cursor("'; DROP TABLE test_users; --");
    assert!(result.is_err(), "Invalid cursor must return error");

    // find_all with malicious cursor string must fail with Validation, not execute raw SQL
    let resource = test_resource();
    let q = ResourceQuery::new(&resource, &pool);
    let page = PageRequest::Cursor {
        after: Some("'; DROP TABLE test_users; --".to_string()),
        limit: 10,
    };
    let result = q
        .find_all(&FilterSet::default(), None, &SortParam::default(), &page)
        .await;
    assert!(
        result.is_err(),
        "Malicious cursor must yield error, not execute SQL"
    );
}
