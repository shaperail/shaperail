//! Dynamic gRPC service implementation for Shaperail resources (M16).
//!
//! Implements a generic gRPC service that dispatches to the appropriate handler
//! based on the service/method name, using the resource schema for encoding.

use std::sync::Arc;

use prost::bytes::{Bytes, BytesMut};
use shaperail_core::{AuthRule, ResourceDefinition, ShaperailError};
use tonic::Status;

use super::codec::{decode_resource_message, encode_resource_message};
use crate::auth::extractor::AuthenticatedUser;
use crate::auth::rbac;
use crate::handlers::crud::AppState;

/// Convert a ShaperailError to a tonic Status.
fn to_status(err: ShaperailError) -> Status {
    match err {
        ShaperailError::NotFound => Status::not_found("Resource not found"),
        ShaperailError::Unauthorized => Status::unauthenticated("Authentication required"),
        ShaperailError::Forbidden => Status::permission_denied("Access denied"),
        ShaperailError::Validation(errors) => {
            let details: Vec<String> = errors
                .iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect();
            Status::invalid_argument(details.join("; "))
        }
        ShaperailError::Conflict(msg) => Status::already_exists(msg),
        ShaperailError::RateLimited => Status::resource_exhausted("Rate limit exceeded"),
        ShaperailError::Internal(msg) => Status::internal(msg),
    }
}

/// Enforce auth rules from an endpoint spec.
#[allow(clippy::result_large_err)]
fn enforce_auth(
    auth_rule: Option<&AuthRule>,
    user: Option<&AuthenticatedUser>,
) -> Result<(), Status> {
    rbac::enforce(auth_rule, user).map_err(to_status)
}

/// Handle a Get RPC: looks up a resource by ID.
pub async fn handle_get(
    state: Arc<AppState>,
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    request_data: &[u8],
) -> Result<Bytes, Status> {
    // Check auth
    let endpoint = resource.endpoints.as_ref().and_then(|e| e.get("get"));
    let auth_rule = endpoint.and_then(|e| e.auth.as_ref());
    enforce_auth(auth_rule, user)?;

    // Decode request to get ID
    let mut id_schema = indexmap::IndexMap::new();
    id_schema.insert(
        "id".to_string(),
        shaperail_core::FieldSchema {
            field_type: shaperail_core::FieldType::String,
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
    let req_json = decode_resource_message(&id_schema, request_data);
    let id = req_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Status::invalid_argument("Missing 'id' field"))?;

    // Query database
    let table = &resource.resource;
    let query = format!("SELECT row_to_json(t) FROM (SELECT * FROM {table} WHERE id = $1) t");
    let row: Option<(serde_json::Value,)> = sqlx::query_as(&query)
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    let record = row
        .map(|(v,)| v)
        .ok_or_else(|| Status::not_found("Not found"))?;

    // Encode response: wrap in a response message with field 1 = data
    let data_bytes = encode_resource_message(&resource.schema, &record);
    let mut response_buf = BytesMut::new();
    // Field 1, wire type 2 (length-delimited)
    prost::encoding::encode_key(
        1,
        prost::encoding::WireType::LengthDelimited,
        &mut response_buf,
    );
    prost::encoding::encode_varint(data_bytes.len() as u64, &mut response_buf);
    response_buf.extend_from_slice(&data_bytes);

    Ok(response_buf.freeze())
}

/// Handle a List RPC: returns all matching records.
pub async fn handle_list(
    state: Arc<AppState>,
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    _request_data: &[u8],
) -> Result<Bytes, Status> {
    let endpoint = resource.endpoints.as_ref().and_then(|e| e.get("list"));
    let auth_rule = endpoint.and_then(|e| e.auth.as_ref());
    enforce_auth(auth_rule, user)?;

    let table = &resource.resource;
    let query = format!("SELECT row_to_json(t) FROM (SELECT * FROM {table} LIMIT 100) t");
    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(&query)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    let mut response_buf = BytesMut::new();
    // Field 1 (repeated items) — each as a length-delimited sub-message
    for (record,) in &rows {
        let item_bytes = encode_resource_message(&resource.schema, record);
        prost::encoding::encode_key(
            1,
            prost::encoding::WireType::LengthDelimited,
            &mut response_buf,
        );
        prost::encoding::encode_varint(item_bytes.len() as u64, &mut response_buf);
        response_buf.extend_from_slice(&item_bytes);
    }
    // Field 3: has_more = false (varint 0)
    prost::encoding::encode_key(3, prost::encoding::WireType::Varint, &mut response_buf);
    prost::encoding::encode_varint(0, &mut response_buf);
    // Field 4: total count
    prost::encoding::encode_key(4, prost::encoding::WireType::Varint, &mut response_buf);
    prost::encoding::encode_varint(rows.len() as u64, &mut response_buf);

    Ok(response_buf.freeze())
}

/// Handle a streaming List RPC: yields one record at a time.
pub async fn handle_stream_list(
    state: Arc<AppState>,
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    _request_data: &[u8],
) -> Result<Vec<Bytes>, Status> {
    let endpoint = resource.endpoints.as_ref().and_then(|e| e.get("list"));
    let auth_rule = endpoint.and_then(|e| e.auth.as_ref());
    enforce_auth(auth_rule, user)?;

    let table = &resource.resource;
    let query = format!("SELECT row_to_json(t) FROM (SELECT * FROM {table}) t");
    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(&query)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    let mut items = Vec::with_capacity(rows.len());
    for (record,) in &rows {
        items.push(encode_resource_message(&resource.schema, record));
    }

    Ok(items)
}

/// Handle a Create RPC.
pub async fn handle_create(
    state: Arc<AppState>,
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    request_data: &[u8],
) -> Result<Bytes, Status> {
    let endpoint = resource.endpoints.as_ref().and_then(|e| e.get("create"));
    let auth_rule = endpoint.and_then(|e| e.auth.as_ref());
    enforce_auth(auth_rule, user)?;

    let input_fields = endpoint
        .and_then(|e| e.input.as_ref())
        .cloned()
        .unwrap_or_default();

    // Build a schema for just the input fields
    let mut input_schema = indexmap::IndexMap::new();
    for field_name in &input_fields {
        if let Some(field) = resource.schema.get(field_name) {
            input_schema.insert(field_name.clone(), field.clone());
        }
    }

    let input_json = decode_resource_message(&input_schema, request_data);
    let table = &resource.resource;

    // Build INSERT
    let columns: Vec<&str> = input_fields.iter().map(|s| s.as_str()).collect();
    let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${i}")).collect();
    let col_list = columns.join(", ");
    let ph_list = placeholders.join(", ");

    let query = format!(
        "INSERT INTO {table} (id, {col_list}) VALUES (gen_random_uuid(), {ph_list}) RETURNING row_to_json({table}.*)"
    );

    let mut q = sqlx::query_as::<_, (serde_json::Value,)>(&query);
    for field_name in &input_fields {
        let val = input_json
            .get(field_name)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        match val {
            serde_json::Value::String(s) => q = q.bind(s),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    q = q.bind(i.to_string());
                } else if let Some(f) = n.as_f64() {
                    q = q.bind(f.to_string());
                } else {
                    q = q.bind(n.to_string());
                }
            }
            serde_json::Value::Bool(b) => q = q.bind(b.to_string()),
            _ => q = q.bind(Option::<String>::None),
        }
    }

    let (record,) = q
        .fetch_one(&state.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    // Wrap in response
    let data_bytes = encode_resource_message(&resource.schema, &record);
    let mut response_buf = BytesMut::new();
    prost::encoding::encode_key(
        1,
        prost::encoding::WireType::LengthDelimited,
        &mut response_buf,
    );
    prost::encoding::encode_varint(data_bytes.len() as u64, &mut response_buf);
    response_buf.extend_from_slice(&data_bytes);

    Ok(response_buf.freeze())
}

/// Handle an Update RPC: updates a resource record by ID.
pub async fn handle_update(
    state: Arc<AppState>,
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    request_data: &[u8],
) -> Result<Bytes, Status> {
    let endpoint = resource.endpoints.as_ref().and_then(|e| e.get("update"));
    let auth_rule = endpoint.and_then(|e| e.auth.as_ref());
    enforce_auth(auth_rule, user)?;

    let input_fields = endpoint
        .and_then(|e| e.input.as_ref())
        .cloned()
        .unwrap_or_default();

    // Build combined decode schema: id first, then input fields
    let mut update_schema = indexmap::IndexMap::new();
    update_schema.insert(
        "id".to_string(),
        shaperail_core::FieldSchema {
            field_type: shaperail_core::FieldType::String,
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
    for field_name in &input_fields {
        if let Some(field) = resource.schema.get(field_name) {
            update_schema.insert(field_name.clone(), field.clone());
        }
    }

    let req_json = decode_resource_message(&update_schema, request_data);
    let id = req_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Status::invalid_argument("Missing 'id' field"))?;

    if input_fields.is_empty() {
        return Err(Status::invalid_argument(
            "Update endpoint has no input fields declared",
        ));
    }

    let table = &resource.resource;
    let set_clauses: Vec<String> = input_fields
        .iter()
        .enumerate()
        .map(|(i, f)| format!("{f} = ${}", i + 1))
        .collect();
    let set_clause = set_clauses.join(", ");
    let id_param = input_fields.len() + 1;
    let query = format!(
        "UPDATE {table} SET {set_clause} WHERE id = ${id_param} RETURNING row_to_json({table}.*)"
    );

    let mut q = sqlx::query_as::<_, (serde_json::Value,)>(&query);
    for field_name in &input_fields {
        let val = req_json
            .get(field_name)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        match val {
            serde_json::Value::String(s) => q = q.bind(s),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    q = q.bind(i.to_string());
                } else if let Some(f) = n.as_f64() {
                    q = q.bind(f.to_string());
                } else {
                    q = q.bind(n.to_string());
                }
            }
            serde_json::Value::Bool(b) => q = q.bind(b.to_string()),
            _ => q = q.bind(Option::<String>::None),
        }
    }
    q = q.bind(id);

    let row = q
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    let record = row
        .map(|(v,)| v)
        .ok_or_else(|| Status::not_found("Not found"))?;

    let data_bytes = encode_resource_message(&resource.schema, &record);
    let mut response_buf = BytesMut::new();
    prost::encoding::encode_key(
        1,
        prost::encoding::WireType::LengthDelimited,
        &mut response_buf,
    );
    prost::encoding::encode_varint(data_bytes.len() as u64, &mut response_buf);
    response_buf.extend_from_slice(&data_bytes);

    Ok(response_buf.freeze())
}

/// Handle a Delete RPC.
pub async fn handle_delete(
    state: Arc<AppState>,
    resource: &ResourceDefinition,
    user: Option<&AuthenticatedUser>,
    request_data: &[u8],
) -> Result<Bytes, Status> {
    let endpoint = resource.endpoints.as_ref().and_then(|e| e.get("delete"));
    let auth_rule = endpoint.and_then(|e| e.auth.as_ref());
    enforce_auth(auth_rule, user)?;

    let mut id_schema = indexmap::IndexMap::new();
    id_schema.insert(
        "id".to_string(),
        shaperail_core::FieldSchema {
            field_type: shaperail_core::FieldType::String,
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
    let req_json = decode_resource_message(&id_schema, request_data);
    let id = req_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Status::invalid_argument("Missing 'id' field"))?;

    let soft = endpoint.map(|e| e.soft_delete).unwrap_or(false);
    let table = &resource.resource;

    if soft {
        let query = format!("UPDATE {table} SET deleted_at = NOW() WHERE id = $1");
        sqlx::query(&query)
            .bind(id)
            .execute(&state.pool)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
    } else {
        let query = format!("DELETE FROM {table} WHERE id = $1");
        sqlx::query(&query)
            .bind(id)
            .execute(&state.pool)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
    }

    // Response: success = true (field 1, varint 1)
    let mut response_buf = BytesMut::new();
    prost::encoding::encode_key(1, prost::encoding::WireType::Varint, &mut response_buf);
    prost::encoding::encode_varint(1, &mut response_buf);

    Ok(response_buf.freeze())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::Code;

    #[test]
    fn shaperail_error_to_status() {
        assert_eq!(to_status(ShaperailError::NotFound).code(), Code::NotFound);
        assert_eq!(
            to_status(ShaperailError::Unauthorized).code(),
            Code::Unauthenticated
        );
        assert_eq!(
            to_status(ShaperailError::Forbidden).code(),
            Code::PermissionDenied
        );
        assert_eq!(
            to_status(ShaperailError::Conflict("dup".to_string())).code(),
            Code::AlreadyExists
        );
        assert_eq!(
            to_status(ShaperailError::RateLimited).code(),
            Code::ResourceExhausted
        );
        assert_eq!(
            to_status(ShaperailError::Internal("test".to_string())).code(),
            Code::Internal
        );
    }

    #[test]
    fn validation_errors_to_status() {
        let errors = vec![
            shaperail_core::FieldError {
                field: "email".to_string(),
                message: "invalid format".to_string(),
                code: "invalid".to_string(),
            },
            shaperail_core::FieldError {
                field: "name".to_string(),
                message: "too short".to_string(),
                code: "too_short".to_string(),
            },
        ];
        let status = to_status(ShaperailError::Validation(errors));
        assert_eq!(status.code(), Code::InvalidArgument);
        assert!(status.message().contains("email: invalid format"));
        assert!(status.message().contains("name: too short"));
    }
}
