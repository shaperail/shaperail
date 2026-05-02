use std::collections::HashMap;

use actix_web::HttpRequest;
use shaperail_core::{EndpointSpec, FieldError, PaginationStyle, ShaperailError};

use crate::db::{FilterSet, PageRequest, SearchParam, SortParam};

/// Parsed query parameters from an HTTP request for a list endpoint.
pub struct ListParams {
    pub filters: FilterSet,
    pub search: Option<SearchParam>,
    pub sort: SortParam,
    pub page: PageRequest,
    pub fields: Vec<String>,
    pub include: Vec<String>,
}

/// Parsed query parameters for get/create/update endpoints.
pub struct ItemParams {
    pub fields: Vec<String>,
    pub include: Vec<String>,
}

/// Extracts all query parameters from the request as a HashMap (public for cache key building).
pub fn query_map_public(req: &HttpRequest) -> HashMap<String, String> {
    query_map(req)
}

/// Extracts all query parameters from the request as a HashMap.
fn query_map(req: &HttpRequest) -> HashMap<String, String> {
    let query_string = req.query_string();
    let mut map = HashMap::new();
    for pair in query_string.split('&') {
        if pair.is_empty() {
            continue;
        }
        if let Some((key, value)) = pair.split_once('=') {
            let key = urldecode(key);
            let value = urldecode(value);
            map.insert(key, value);
        }
    }
    map
}

/// Simple URL decoding (percent-decode and '+' → space).
fn urldecode(s: &str) -> String {
    let s = s.replace('+', " ");
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Parses query parameters for a list endpoint based on the endpoint spec.
pub fn parse_list_params(req: &HttpRequest, endpoint: &EndpointSpec) -> ListParams {
    let params = query_map(req);

    let allowed_filters = endpoint.filters.as_deref().unwrap_or(&[]);
    let filters = FilterSet::from_query_params(&params, allowed_filters);

    let search = params.get("search").and_then(|term| {
        let search_fields = endpoint.search.as_deref().unwrap_or(&[]);
        SearchParam::new(term, search_fields)
    });

    let sort_fields = endpoint.sort.as_deref().unwrap_or(&[]);
    // If no sort fields declared, allow sorting by any field in filters + schema
    let all_schema_fields: Vec<String> = sort_fields.to_vec();
    let sort = params
        .get("sort")
        .map(|raw| SortParam::parse(raw, &all_schema_fields))
        .unwrap_or_default();

    let page = match endpoint
        .pagination
        .as_ref()
        .unwrap_or(&PaginationStyle::Cursor)
    {
        PaginationStyle::Cursor => {
            let after = params.get("after").cloned();
            let limit = params.get("limit").and_then(|s| s.parse::<i64>().ok());
            PageRequest::Cursor {
                after,
                limit: PageRequest::clamped_limit(limit),
            }
        }
        PaginationStyle::Offset => {
            let offset = params
                .get("offset")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0)
                .max(0);
            let limit = params.get("limit").and_then(|s| s.parse::<i64>().ok());
            PageRequest::Offset {
                offset,
                limit: PageRequest::clamped_limit(limit),
            }
        }
    };

    let fields = parse_csv_param(&params, "fields");
    let include = parse_csv_param(&params, "include");

    ListParams {
        filters,
        search,
        sort,
        page,
        fields,
        include,
    }
}

/// Rejects malformed filter query params on list endpoints.
///
/// The runtime convention is `?filter[<field>]=<value>`, and only fields
/// declared in the endpoint's `filters:` list are honored. Two failure modes
/// are surfaced as 422 errors so callers learn immediately when their URL
/// will return unfiltered results:
///
/// 1. **`INVALID_FILTER_FORM`** — bare `?<field>=<value>` where `<field>`
///    matches a declared filter. Hint: "did you mean `?filter[<field>]=...`?".
/// 2. **`UNDECLARED_FILTER`** — bracket-form `?filter[<field>]=<value>`
///    where `<field>` is not in the declared list (or no filters are
///    declared at all). Message lists the available filters or notes that
///    the endpoint declares none.
///
/// Bare params that do not match any declared filter are left alone — they
/// may be application-defined or reserved (`sort`, `after`, `limit`, etc.).
/// Multiple offending keys accumulate into a single 422 response.
pub fn validate_filter_param_form(
    req: &HttpRequest,
    endpoint: &EndpointSpec,
) -> Result<(), ShaperailError> {
    let allowed = endpoint.filters.as_deref().unwrap_or(&[]);
    let params = query_map(req);
    let mut bad: Vec<FieldError> = Vec::new();
    for key in params.keys() {
        // Bare-field matching a declared filter — Issue G (0.11.3).
        if allowed.iter().any(|f| f == key) {
            bad.push(FieldError {
                field: key.clone(),
                message: format!(
                    "filter params use bracket notation; did you mean `?filter[{key}]=...`?"
                ),
                code: "INVALID_FILTER_FORM".to_string(),
            });
            continue;
        }
        // Bracket-notation on an undeclared field — Issue H.
        if let Some(field) = key
            .strip_prefix("filter[")
            .and_then(|s| s.strip_suffix(']'))
        {
            if !allowed.iter().any(|f| f == field) {
                let message = if allowed.is_empty() {
                    format!("this endpoint declares no filters; '{field}' is not accepted")
                } else {
                    format!(
                        "'{field}' is not a declared filter on this endpoint; available filters: {}",
                        allowed.join(", ")
                    )
                };
                bad.push(FieldError {
                    field: format!("filter[{field}]"),
                    message,
                    code: "UNDECLARED_FILTER".to_string(),
                });
            }
        }
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(ShaperailError::Validation(bad))
    }
}

/// Parses query parameters for item endpoints (get/create/update).
pub fn parse_item_params(req: &HttpRequest) -> ItemParams {
    let params = query_map(req);
    ItemParams {
        fields: parse_csv_param(&params, "fields"),
        include: parse_csv_param(&params, "include"),
    }
}

/// Parses a comma-separated query parameter into a Vec<String>.
fn parse_csv_param(params: &HashMap<String, String>, key: &str) -> Vec<String> {
    params
        .get(key)
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urldecode_basic() {
        assert_eq!(urldecode("hello%20world"), "hello world");
        assert_eq!(urldecode("foo+bar"), "foo bar");
        assert_eq!(urldecode("a%3Db"), "a=b");
    }

    #[test]
    fn parse_csv_param_splits() {
        let mut params = HashMap::new();
        params.insert("fields".to_string(), "name,email,role".to_string());
        let result = parse_csv_param(&params, "fields");
        assert_eq!(result, vec!["name", "email", "role"]);
    }

    #[test]
    fn parse_csv_param_empty() {
        let params = HashMap::new();
        let result = parse_csv_param(&params, "fields");
        assert!(result.is_empty());
    }

    fn endpoint_with_filters(filters: &[&str]) -> EndpointSpec {
        EndpointSpec {
            filters: Some(filters.iter().map(|s| s.to_string()).collect()),
            ..Default::default()
        }
    }

    #[test]
    fn validate_filter_param_form_accepts_bracket_notation() {
        let req = actix_web::test::TestRequest::default()
            .uri("/v1/items?filter[role]=admin&sort=-created_at")
            .to_http_request();
        let ep = endpoint_with_filters(&["role", "org_id"]);
        assert!(validate_filter_param_form(&req, &ep).is_ok());
    }

    #[test]
    fn validate_filter_param_form_rejects_bare_field_matching_declared_filter() {
        let req = actix_web::test::TestRequest::default()
            .uri("/v1/items?role=admin")
            .to_http_request();
        let ep = endpoint_with_filters(&["role", "org_id"]);
        let err = validate_filter_param_form(&req, &ep).unwrap_err();
        match err {
            ShaperailError::Validation(field_errors) => {
                assert_eq!(field_errors.len(), 1);
                assert_eq!(field_errors[0].field, "role");
                assert_eq!(field_errors[0].code, "INVALID_FILTER_FORM");
                assert!(field_errors[0].message.contains("filter[role]"));
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn validate_filter_param_form_ignores_unrelated_bare_params() {
        let req = actix_web::test::TestRequest::default()
            .uri("/v1/items?some_other_param=value&sort=name")
            .to_http_request();
        let ep = endpoint_with_filters(&["role"]);
        assert!(validate_filter_param_form(&req, &ep).is_ok());
    }

    #[test]
    fn validate_filter_param_form_is_noop_when_no_filters_declared() {
        let req = actix_web::test::TestRequest::default()
            .uri("/v1/items?role=admin")
            .to_http_request();
        let ep = endpoint_with_filters(&[]);
        assert!(validate_filter_param_form(&req, &ep).is_ok());
    }

    #[test]
    fn validate_filter_param_form_rejects_bracket_notation_on_undeclared_field() {
        let req = actix_web::test::TestRequest::default()
            .uri("/v1/items?filter[org_id]=abc-123")
            .to_http_request();
        let ep = endpoint_with_filters(&["posting_date", "source"]);
        let err = validate_filter_param_form(&req, &ep).unwrap_err();
        match err {
            ShaperailError::Validation(field_errors) => {
                assert_eq!(field_errors.len(), 1);
                assert_eq!(field_errors[0].field, "filter[org_id]");
                assert_eq!(field_errors[0].code, "UNDECLARED_FILTER");
                assert!(
                    field_errors[0].message.contains("'org_id'"),
                    "message should name the offending field: {}",
                    field_errors[0].message
                );
                assert!(
                    field_errors[0].message.contains("posting_date"),
                    "message should list available filters: {}",
                    field_errors[0].message
                );
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn validate_filter_param_form_rejects_bracket_notation_when_no_filters_declared() {
        let req = actix_web::test::TestRequest::default()
            .uri("/v1/items?filter[anything]=value")
            .to_http_request();
        let ep = endpoint_with_filters(&[]);
        let err = validate_filter_param_form(&req, &ep).unwrap_err();
        match err {
            ShaperailError::Validation(field_errors) => {
                assert_eq!(field_errors.len(), 1);
                assert_eq!(field_errors[0].field, "filter[anything]");
                assert_eq!(field_errors[0].code, "UNDECLARED_FILTER");
                assert!(
                    field_errors[0].message.contains("declares no filters"),
                    "no-filters-declared message should call that out: {}",
                    field_errors[0].message
                );
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn validate_filter_param_form_accumulates_multiple_errors() {
        // role: bare-field match → INVALID_FILTER_FORM.
        // filter[org_id]: bracket on undeclared field → UNDECLARED_FILTER.
        let req = actix_web::test::TestRequest::default()
            .uri("/v1/items?role=admin&filter[org_id]=abc")
            .to_http_request();
        let ep = endpoint_with_filters(&["role"]);
        let err = validate_filter_param_form(&req, &ep).unwrap_err();
        match err {
            ShaperailError::Validation(field_errors) => {
                assert_eq!(field_errors.len(), 2);
                let codes: std::collections::HashSet<&str> =
                    field_errors.iter().map(|e| e.code.as_str()).collect();
                assert!(codes.contains("INVALID_FILTER_FORM"));
                assert!(codes.contains("UNDECLARED_FILTER"));
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }
}
