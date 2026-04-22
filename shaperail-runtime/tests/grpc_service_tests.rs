//! Integration tests for gRPC service handlers.
//! Run with: cargo test -p shaperail-runtime --test grpc_service_tests

use std::sync::Arc;

use indexmap::IndexMap;
use prost::bytes::BytesMut;
use shaperail_core::{EndpointSpec, FieldSchema, FieldType, HttpMethod, ResourceDefinition};
use shaperail_runtime::grpc::service;
use shaperail_runtime::handlers::crud::AppState;
use shaperail_runtime::observability::MetricsState;

fn test_resource() -> ResourceDefinition {
    let mut schema = IndexMap::new();
    schema.insert(
        "id".to_string(),
        FieldSchema {
            field_type: FieldType::String,
            primary: true,
            generated: true,
            required: true,
            unique: true,
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
        "name".to_string(),
        FieldSchema {
            field_type: FieldType::String,
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
    let mut endpoints = IndexMap::new();
    endpoints.insert(
        "update".to_string(),
        EndpointSpec {
            method: Some(HttpMethod::Patch),
            path: Some("/grpc_test_items/:id".to_string()),
            auth: None,
            input: Some(vec!["name".to_string()]),
            ..Default::default()
        },
    );
    ResourceDefinition {
        resource: "grpc_test_items".to_string(),
        version: 1,
        db: None,
        tenant_key: None,
        schema,
        endpoints: Some(endpoints),
        relations: None,
        indexes: None,
    }
}

fn make_state(pool: sqlx::PgPool) -> Arc<AppState> {
    Arc::new(AppState {
        pool,
        resources: vec![],
        stores: None,
        controllers: None,
        jwt_config: None,
        cache: None,
        event_emitter: None,
        job_queue: None,
        rate_limiter: None,
        metrics: Some(MetricsState::new().expect("metrics")),
        wasm_runtime: None,
        event_bus: tokio::sync::broadcast::channel(16).0,
    })
}

/// Encode a gRPC update request: field 1 = id (string), field 2 = name (string).
fn encode_update_request(id: &str, name: &str) -> prost::bytes::Bytes {
    let mut buf = BytesMut::new();
    // Field 1: id
    prost::encoding::encode_key(1, prost::encoding::WireType::LengthDelimited, &mut buf);
    prost::encoding::encode_varint(id.len() as u64, &mut buf);
    buf.extend_from_slice(id.as_bytes());
    // Field 2: name
    prost::encoding::encode_key(2, prost::encoding::WireType::LengthDelimited, &mut buf);
    prost::encoding::encode_varint(name.len() as u64, &mut buf);
    buf.extend_from_slice(name.as_bytes());
    buf.freeze()
}

#[sqlx::test]
async fn grpc_handle_update_changes_record(pool: sqlx::PgPool) {
    sqlx::query("CREATE TABLE grpc_test_items (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO grpc_test_items (id, name) VALUES ('item-1', 'original')")
        .execute(&pool)
        .await
        .unwrap();

    let state = make_state(pool.clone());
    let resource = test_resource();
    let request_bytes = encode_update_request("item-1", "updated");

    let result: Result<prost::bytes::Bytes, tonic::Status> =
        service::handle_update(state, &resource, None, &request_bytes).await;
    assert!(result.is_ok(), "handle_update failed: {:?}", result.err());

    let row: (String,) = sqlx::query_as("SELECT name FROM grpc_test_items WHERE id = 'item-1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, "updated");
}

#[sqlx::test]
async fn grpc_handle_update_missing_id_returns_error(pool: sqlx::PgPool) {
    let state = make_state(pool);
    let resource = test_resource();

    let result: Result<prost::bytes::Bytes, tonic::Status> =
        service::handle_update(state, &resource, None, &[]).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}
