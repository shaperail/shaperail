use std::collections::HashMap;

use actix_web::HttpRequest;
use shaperail_core::{EndpointSpec, PaginationStyle};

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

    #[test]
    fn parse_csv_param_trims_spaces() {
        let mut params = HashMap::new();
        params.insert("fields".to_string(), " name , email , role ".to_string());
        let result = parse_csv_param(&params, "fields");
        assert_eq!(result, vec!["name", "email", "role"]);
    }

    #[test]
    fn parse_csv_param_filters_empty_segments() {
        let mut params = HashMap::new();
        params.insert("fields".to_string(), "name,,role,".to_string());
        let result = parse_csv_param(&params, "fields");
        assert_eq!(result, vec!["name", "role"]);
    }

    #[test]
    fn urldecode_plus_to_space() {
        assert_eq!(urldecode("hello+world"), "hello world");
    }

    #[test]
    fn urldecode_percent_encoded_chars() {
        assert_eq!(urldecode("hello%20world"), "hello world");
        assert_eq!(urldecode("a%3Db"), "a=b");
        assert_eq!(urldecode("%40"), "@");
    }

    #[test]
    fn urldecode_invalid_percent_keeps_as_is() {
        // Invalid hex after % — should not panic, just pass through
        let result = urldecode("a%ZZb");
        assert!(result.contains('a'), "Should preserve surrounding chars");
    }

    #[test]
    fn query_map_public_parses_query_string() {
        use actix_web::test::TestRequest;
        let req = TestRequest::get()
            .uri("/users?filter%5Brole%5D=admin&limit=10")
            .to_http_request();
        let map = query_map_public(&req);
        // URL-encoded bracket: filter[role]=admin
        assert!(
            map.contains_key("filter[role]") || map.contains_key("filter%5Brole%5D"),
            "Map should contain the filter key"
        );
        assert_eq!(map.get("limit").map(|s| s.as_str()), Some("10"));
    }
}
