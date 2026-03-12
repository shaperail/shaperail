//! Criterion benchmarks for shaperail-runtime hot paths.
//!
//! These are pure CPU benchmarks — no database or Redis connections required.
//! They validate the framework's ability to meet PRD performance targets:
//!   - Simple JSON response: 150,000+ req/s
//!   - Idle memory: <= 60 MB
//!   - Cold start: < 100ms

use std::collections::HashMap;

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use indexmap::IndexMap;
use shaperail_core::{FieldSchema, FieldType, IndexSpec, ResourceDefinition};
use shaperail_runtime::cache::RedisCache;
use shaperail_runtime::db::{build_create_table_sql, FilterParam, FilterSet, SortParam};
use shaperail_runtime::handlers::response::{self, BulkResponse, ListResponse, SingleResponse};
use shaperail_runtime::handlers::validate::validate_input;

// ---------------------------------------------------------------------------
// Shared test fixtures
// ---------------------------------------------------------------------------

fn user_resource() -> ResourceDefinition {
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

    ResourceDefinition {
        resource: "users".to_string(),
        version: 1,
        schema,
        endpoints: None,
        relations: None,
        indexes: Some(vec![
            IndexSpec {
                fields: vec!["org_id".to_string(), "role".to_string()],
                unique: false,
                order: None,
            },
            IndexSpec {
                fields: vec!["created_at".to_string()],
                unique: false,
                order: Some("desc".to_string()),
            },
        ]),
    }
}

fn sample_user_json() -> serde_json::Value {
    serde_json::json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "email": "alice@example.com",
        "name": "Alice Johnson",
        "role": "admin",
        "org_id": "660e8400-e29b-41d4-a716-446655440001",
        "created_at": "2025-01-15T10:30:00Z",
        "updated_at": "2025-06-01T14:22:00Z"
    })
}

fn sample_user_list(n: usize) -> Vec<serde_json::Value> {
    (0..n)
        .map(|i| {
            serde_json::json!({
                "id": format!("550e8400-e29b-41d4-a716-44665544{:04}", i),
                "email": format!("user{}@example.com", i),
                "name": format!("User {}", i),
                "role": "member",
                "org_id": "660e8400-e29b-41d4-a716-446655440001",
                "created_at": "2025-01-15T10:30:00Z",
                "updated_at": "2025-06-01T14:22:00Z"
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. JSON Response Serialization
// ---------------------------------------------------------------------------

fn bench_json_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_serialization");

    // Single response envelope — the core "simple JSON response" path
    let single_data = sample_user_json();
    group.throughput(Throughput::Elements(1));
    group.bench_function("single_response_to_bytes", |b| {
        b.iter(|| {
            let envelope = SingleResponse {
                data: black_box(single_data.clone()),
            };
            let bytes = serde_json::to_vec(&envelope).unwrap();
            black_box(bytes);
        });
    });

    // List response envelope — 20 items (typical page)
    let list_data = sample_user_list(20);
    let meta = serde_json::json!({"cursor": "abc123", "has_more": true});
    group.throughput(Throughput::Elements(20));
    group.bench_function("list_response_20_items_to_bytes", |b| {
        b.iter(|| {
            let envelope = ListResponse {
                data: black_box(list_data.clone()),
                meta: black_box(meta.clone()),
            };
            let bytes = serde_json::to_vec(&envelope).unwrap();
            black_box(bytes);
        });
    });

    // List response envelope — 100 items (large page)
    let list_data_100 = sample_user_list(100);
    let meta_100 = serde_json::json!({"cursor": "xyz789", "has_more": true});
    group.throughput(Throughput::Elements(100));
    group.bench_function("list_response_100_items_to_bytes", |b| {
        b.iter(|| {
            let envelope = ListResponse {
                data: black_box(list_data_100.clone()),
                meta: black_box(meta_100.clone()),
            };
            let bytes = serde_json::to_vec(&envelope).unwrap();
            black_box(bytes);
        });
    });

    // Bulk response envelope
    let bulk_data = sample_user_list(10);
    group.throughput(Throughput::Elements(10));
    group.bench_function("bulk_response_10_items_to_bytes", |b| {
        b.iter(|| {
            let total = bulk_data.len();
            let envelope = BulkResponse {
                data: black_box(bulk_data.clone()),
                meta: response::BulkMeta { total },
            };
            let bytes = serde_json::to_vec(&envelope).unwrap();
            black_box(bytes);
        });
    });

    // select_fields — filter 3 fields from a 7-field object
    let full_obj = sample_user_json();
    let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];
    group.throughput(Throughput::Elements(1));
    group.bench_function("select_fields_3_of_7", |b| {
        b.iter(|| {
            let filtered = response::select_fields(black_box(&full_obj), black_box(&fields));
            black_box(filtered);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 2. Validation Throughput
// ---------------------------------------------------------------------------

fn bench_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation");
    let resource = user_resource();

    // Valid payload — happy path
    let mut valid_data = serde_json::Map::new();
    valid_data.insert("name".to_string(), serde_json::json!("Alice Johnson"));
    valid_data.insert("email".to_string(), serde_json::json!("alice@example.com"));
    valid_data.insert(
        "org_id".to_string(),
        serde_json::json!("660e8400-e29b-41d4-a716-446655440001"),
    );
    valid_data.insert("role".to_string(), serde_json::json!("admin"));

    group.throughput(Throughput::Elements(1));
    group.bench_function("valid_payload", |b| {
        b.iter(|| {
            let result = validate_input(black_box(&valid_data), black_box(&resource));
            black_box(result).unwrap();
        });
    });

    // Invalid payload — multiple errors (missing required, bad enum, bad email)
    let mut invalid_data = serde_json::Map::new();
    // name is missing (required)
    invalid_data.insert("email".to_string(), serde_json::json!("not-an-email"));
    invalid_data.insert("org_id".to_string(), serde_json::json!("not-a-uuid"));
    invalid_data.insert("role".to_string(), serde_json::json!("superuser"));

    group.throughput(Throughput::Elements(1));
    group.bench_function("invalid_payload_multi_error", |b| {
        b.iter(|| {
            let result = validate_input(black_box(&invalid_data), black_box(&resource));
            black_box(result).unwrap_err();
        });
    });

    // Minimal payload — just required fields
    let mut minimal_data = serde_json::Map::new();
    minimal_data.insert("name".to_string(), serde_json::json!("A"));
    minimal_data.insert("email".to_string(), serde_json::json!("a@b.co"));
    minimal_data.insert(
        "org_id".to_string(),
        serde_json::json!("550e8400-e29b-41d4-a716-446655440000"),
    );

    group.throughput(Throughput::Elements(1));
    group.bench_function("minimal_valid_payload", |b| {
        b.iter(|| {
            let result = validate_input(black_box(&minimal_data), black_box(&resource));
            black_box(result).unwrap();
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Query Building (SQL string generation, no DB)
// ---------------------------------------------------------------------------

fn bench_query_building(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_building");
    let resource = user_resource();

    // CREATE TABLE generation
    group.throughput(Throughput::Elements(1));
    group.bench_function("build_create_table_sql", |b| {
        b.iter(|| {
            let sql = build_create_table_sql(black_box(&resource));
            black_box(sql);
        });
    });

    // Filter SQL generation — two filters
    let filter_set = FilterSet {
        filters: vec![
            FilterParam {
                field: "role".to_string(),
                value: "admin".to_string(),
            },
            FilterParam {
                field: "org_id".to_string(),
                value: "660e8400-e29b-41d4-a716-446655440001".to_string(),
            },
        ],
    };

    group.throughput(Throughput::Elements(1));
    group.bench_function("filter_apply_to_sql_2_filters", |b| {
        b.iter(|| {
            let mut sql = String::from("SELECT * FROM \"users\"");
            filter_set.apply_to_sql(black_box(&mut sql), false, 1);
            black_box(sql);
        });
    });

    // Sort SQL generation — two fields
    let sort_param = SortParam {
        fields: vec![
            shaperail_runtime::db::SortField {
                field: "created_at".to_string(),
                direction: shaperail_runtime::db::SortDirection::Desc,
            },
            shaperail_runtime::db::SortField {
                field: "name".to_string(),
                direction: shaperail_runtime::db::SortDirection::Asc,
            },
        ],
    };

    group.throughput(Throughput::Elements(1));
    group.bench_function("sort_apply_to_sql_2_fields", |b| {
        b.iter(|| {
            let mut sql = String::from("SELECT * FROM \"users\"");
            sort_param.apply_to_sql(black_box(&mut sql));
            black_box(sql);
        });
    });

    // Combined filter + sort + search SQL
    let search_fields = vec!["name".to_string(), "email".to_string()];
    group.throughput(Throughput::Elements(1));
    group.bench_function("combined_filter_sort_search_sql", |b| {
        b.iter(|| {
            let mut sql = String::from("SELECT * FROM \"users\"");
            let offset = filter_set.apply_to_sql(&mut sql, false, 1);
            let search = shaperail_runtime::db::SearchParam {
                term: "alice".to_string(),
                fields: search_fields.clone(),
            };
            let offset = search.apply_to_sql(&mut sql, true, offset);
            sort_param.apply_to_sql(&mut sql);
            let _ = offset;
            black_box(sql);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 4. Cache Key Generation
// ---------------------------------------------------------------------------

fn bench_cache_key(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_key");

    // Empty params
    let empty_params: HashMap<String, String> = HashMap::new();
    group.throughput(Throughput::Elements(1));
    group.bench_function("build_key_empty_params", |b| {
        b.iter(|| {
            let key = RedisCache::build_key(
                black_box("users"),
                black_box("list"),
                black_box(&empty_params),
                black_box("member"),
            );
            black_box(key);
        });
    });

    // Typical params — 3 filter keys
    let mut typical_params: HashMap<String, String> = HashMap::new();
    typical_params.insert("filter[role]".to_string(), "admin".to_string());
    typical_params.insert(
        "filter[org_id]".to_string(),
        "660e8400-e29b-41d4-a716-446655440001".to_string(),
    );
    typical_params.insert("sort".to_string(), "-created_at".to_string());

    group.throughput(Throughput::Elements(1));
    group.bench_function("build_key_3_params", |b| {
        b.iter(|| {
            let key = RedisCache::build_key(
                black_box("users"),
                black_box("list"),
                black_box(&typical_params),
                black_box("admin"),
            );
            black_box(key);
        });
    });

    // Many params — 10 keys (stress test)
    let mut many_params: HashMap<String, String> = HashMap::new();
    for i in 0..10 {
        many_params.insert(format!("param_{i}"), format!("value_{i}"));
    }

    group.throughput(Throughput::Elements(1));
    group.bench_function("build_key_10_params", |b| {
        b.iter(|| {
            let key = RedisCache::build_key(
                black_box("users"),
                black_box("list"),
                black_box(&many_params),
                black_box("viewer"),
            );
            black_box(key);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 5. Filter/Sort/Search Parsing
// ---------------------------------------------------------------------------

fn bench_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("parsing");

    // Filter parsing from query params
    let mut raw_params: HashMap<String, String> = HashMap::new();
    raw_params.insert("filter[role]".to_string(), "admin".to_string());
    raw_params.insert(
        "filter[org_id]".to_string(),
        "660e8400-e29b-41d4-a716-446655440001".to_string(),
    );
    raw_params.insert("filter[disallowed]".to_string(), "ignored".to_string());
    raw_params.insert("other".to_string(), "param".to_string());

    let allowed_filters = vec!["role".to_string(), "org_id".to_string()];

    group.throughput(Throughput::Elements(1));
    group.bench_function("filter_set_from_query_params", |b| {
        b.iter(|| {
            let fs =
                FilterSet::from_query_params(black_box(&raw_params), black_box(&allowed_filters));
            black_box(fs);
        });
    });

    // Sort parsing
    let allowed_sort = vec![
        "created_at".to_string(),
        "name".to_string(),
        "email".to_string(),
    ];

    group.throughput(Throughput::Elements(1));
    group.bench_function("sort_param_parse", |b| {
        b.iter(|| {
            let sp = SortParam::parse(
                black_box("-created_at,name,email"),
                black_box(&allowed_sort),
            );
            black_box(sp);
        });
    });

    // Search param creation
    let search_fields = vec!["name".to_string(), "email".to_string()];
    group.throughput(Throughput::Elements(1));
    group.bench_function("search_param_new", |b| {
        b.iter(|| {
            let sp = shaperail_runtime::db::SearchParam::new(
                black_box("alice johnson"),
                black_box(&search_fields),
            );
            black_box(sp);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_json_serialization,
    bench_validation,
    bench_query_building,
    bench_cache_key,
    bench_parsing,
);
criterion_main!(benches);
