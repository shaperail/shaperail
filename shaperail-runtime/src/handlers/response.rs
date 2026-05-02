use actix_web::HttpResponse;
use serde::Serialize;

/// Response envelope for a single record.
///
/// Shape: `{ "data": { ... } }`
#[derive(Debug, Serialize)]
pub struct SingleResponse {
    pub data: serde_json::Value,
}

/// Response envelope for a list of records with pagination metadata.
///
/// Shape: `{ "data": [...], "meta": { "cursor", "has_more", "total" } }`
#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub data: Vec<serde_json::Value>,
    pub meta: serde_json::Value,
}

/// Response envelope for a bulk operation.
///
/// Shape: `{ "data": [...], "meta": { "total" } }`
#[derive(Debug, Serialize)]
pub struct BulkResponse {
    pub data: Vec<serde_json::Value>,
    pub meta: BulkMeta,
}

#[derive(Debug, Serialize)]
pub struct BulkMeta {
    pub total: usize,
}

/// Builds an HTTP 200 response with a single record envelope.
pub fn single(data: serde_json::Value) -> HttpResponse {
    HttpResponse::Ok().json(SingleResponse { data })
}

/// Builds an HTTP 200 response with a single record envelope (data already wrapped).
pub fn single_value(data: serde_json::Value) -> HttpResponse {
    HttpResponse::Ok().json(SingleResponse { data })
}

/// Builds an HTTP 200 response with a list envelope.
pub fn list(data: Vec<serde_json::Value>, meta: serde_json::Value) -> HttpResponse {
    HttpResponse::Ok().json(ListResponse { data, meta })
}

/// Builds an HTTP 201 response with a single record envelope.
pub fn created(data: serde_json::Value) -> HttpResponse {
    HttpResponse::Created().json(SingleResponse { data })
}

/// Builds an HTTP 200 response with a bulk operation envelope.
pub fn bulk(data: Vec<serde_json::Value>) -> HttpResponse {
    let total = data.len();
    HttpResponse::Ok().json(BulkResponse {
        data,
        meta: BulkMeta { total },
    })
}

/// Builds an HTTP 204 No Content response (for hard deletes).
pub fn no_content() -> HttpResponse {
    HttpResponse::NoContent().finish()
}

/// Filters a JSON object to only include the requested fields.
///
/// If `fields` is empty, returns the original value unchanged.
pub fn select_fields(value: &serde_json::Value, fields: &[String]) -> serde_json::Value {
    if fields.is_empty() {
        return value.clone();
    }
    if let Some(obj) = value.as_object() {
        let filtered: serde_json::Map<String, serde_json::Value> = obj
            .iter()
            .filter(|(k, _)| fields.iter().any(|f| f == k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        serde_json::Value::Object(filtered)
    } else {
        value.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_fields_filters_correctly() {
        let value = serde_json::json!({
            "id": "abc",
            "name": "Alice",
            "email": "alice@example.com",
            "role": "admin"
        });

        let fields = vec!["name".to_string(), "email".to_string()];
        let result = select_fields(&value, &fields);

        assert_eq!(result.as_object().map(|o| o.len()), Some(2));
        assert_eq!(result["name"], "Alice");
        assert_eq!(result["email"], "alice@example.com");
        assert!(result.get("id").is_none());
    }

    #[test]
    fn select_fields_empty_returns_all() {
        let value = serde_json::json!({"id": "abc", "name": "Alice"});
        let result = select_fields(&value, &[]);
        assert_eq!(result, value);
    }

    #[test]
    fn single_value_returns_data_wrapper() {
        let data = serde_json::json!({"id": "abc", "name": "Alice"});
        let resp = single_value(data.clone());
        assert_eq!(resp.status(), 200);
    }

    #[test]
    fn select_fields_non_object_returns_unchanged() {
        let value = serde_json::json!([1, 2, 3]);
        let fields = vec!["x".to_string()];
        let result = select_fields(&value, &fields);
        assert_eq!(result, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn select_fields_unknown_field_name_excluded() {
        let value = serde_json::json!({"id": "abc", "name": "Alice"});
        let fields = vec!["id".to_string(), "missing".to_string()];
        let result = select_fields(&value, &fields);
        // Only "id" is in the object — "missing" is silently absent
        assert_eq!(result.as_object().map(|o| o.len()), Some(1));
        assert_eq!(result["id"], "abc");
    }
}
