//! Integration tests for the steel-runtime database layer.
//!
//! Uses `#[sqlx::test]` macro for auto-rollback and isolated DB per test.
//! Requires a running PostgreSQL instance.
//! Set DATABASE_URL env var or run `docker compose up -d` first.
//!
//! Run with: cargo test -p steel-runtime --test db_integration

use indexmap::IndexMap;
use steel_core::{FieldSchema, FieldType, ResourceDefinition};
use steel_runtime::db::{
    health_check, FilterParam, FilterSet, PageRequest, ResourceQuery, SortParam,
};

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
        schema,
        endpoints: None,
        relations: None,
        indexes: None,
    }
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
    let resource = test_resource();
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
