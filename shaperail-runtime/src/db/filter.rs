use std::collections::HashMap;

/// A single filter parameter parsed from `?filter[field]=value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterParam {
    /// The field name to filter on.
    pub field: String,
    /// The value to match against.
    pub value: String,
}

/// A set of filter parameters for a query.
#[derive(Debug, Clone, Default)]
pub struct FilterSet {
    pub filters: Vec<FilterParam>,
}

impl FilterSet {
    /// Parses filter parameters from a query string map.
    ///
    /// Expects keys in the format `filter[field_name]`.
    /// Only includes filters for fields that are in the `allowed_fields` list.
    pub fn from_query_params(params: &HashMap<String, String>, allowed_fields: &[String]) -> Self {
        let mut filters = Vec::new();
        for (key, value) in params {
            if let Some(field) = key
                .strip_prefix("filter[")
                .and_then(|s| s.strip_suffix(']'))
            {
                if allowed_fields.iter().any(|f| f == field) {
                    filters.push(FilterParam {
                        field: field.to_string(),
                        value: value.clone(),
                    });
                }
            }
        }
        FilterSet { filters }
    }

    /// Returns true if there are no filters.
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    /// Appends WHERE clauses to the given SQL string.
    ///
    /// `param_offset` is the starting `$N` parameter index.
    /// Returns the new parameter offset after appending.
    pub fn apply_to_sql(&self, sql: &mut String, has_where: bool, param_offset: usize) -> usize {
        let mut offset = param_offset;
        for (i, filter) in self.filters.iter().enumerate() {
            if i == 0 && !has_where {
                sql.push_str(" WHERE ");
            } else {
                sql.push_str(" AND ");
            }
            sql.push_str(&format!("\"{}\" = ${}", filter.field, offset));
            offset += 1;
        }
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_filter_params() {
        let mut params = HashMap::new();
        params.insert("filter[role]".to_string(), "admin".to_string());
        params.insert("filter[org_id]".to_string(), "abc-123".to_string());
        params.insert("other_param".to_string(), "ignored".to_string());
        params.insert("filter[secret]".to_string(), "blocked".to_string());

        let allowed = vec!["role".to_string(), "org_id".to_string()];
        let fs = FilterSet::from_query_params(&params, &allowed);

        assert_eq!(fs.filters.len(), 2);
        assert!(fs
            .filters
            .iter()
            .any(|f| f.field == "role" && f.value == "admin"));
        assert!(fs
            .filters
            .iter()
            .any(|f| f.field == "org_id" && f.value == "abc-123"));
    }

    #[test]
    fn filter_disallowed_fields_ignored() {
        let mut params = HashMap::new();
        params.insert("filter[secret]".to_string(), "value".to_string());

        let allowed: Vec<String> = vec!["role".to_string()];
        let fs = FilterSet::from_query_params(&params, &allowed);

        assert!(fs.is_empty());
    }

    #[test]
    fn apply_to_sql_no_existing_where() {
        let fs = FilterSet {
            filters: vec![
                FilterParam {
                    field: "role".to_string(),
                    value: "admin".to_string(),
                },
                FilterParam {
                    field: "org_id".to_string(),
                    value: "abc".to_string(),
                },
            ],
        };

        let mut sql = "SELECT * FROM users".to_string();
        let offset = fs.apply_to_sql(&mut sql, false, 1);

        assert_eq!(
            sql,
            "SELECT * FROM users WHERE \"role\" = $1 AND \"org_id\" = $2"
        );
        assert_eq!(offset, 3);
    }

    #[test]
    fn apply_to_sql_with_existing_where() {
        let fs = FilterSet {
            filters: vec![FilterParam {
                field: "role".to_string(),
                value: "admin".to_string(),
            }],
        };

        let mut sql = "SELECT * FROM users WHERE \"deleted_at\" IS NULL".to_string();
        let offset = fs.apply_to_sql(&mut sql, true, 1);

        assert_eq!(
            sql,
            "SELECT * FROM users WHERE \"deleted_at\" IS NULL AND \"role\" = $1"
        );
        assert_eq!(offset, 2);
    }

    #[test]
    fn empty_filter_set() {
        let fs = FilterSet::default();
        assert!(fs.is_empty());

        let mut sql = "SELECT * FROM users".to_string();
        let offset = fs.apply_to_sql(&mut sql, false, 1);
        assert_eq!(sql, "SELECT * FROM users");
        assert_eq!(offset, 1);
    }
}
