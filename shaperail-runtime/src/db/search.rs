/// Full-text search parameter using PostgreSQL `to_tsvector`.
///
/// When `?search=term` is provided and the endpoint declares `search: [field1, field2]`,
/// a `WHERE to_tsvector('english', field1 || ' ' || field2) @@ plainto_tsquery('english', $N)`
/// clause is appended.
#[derive(Debug, Clone)]
pub struct SearchParam {
    /// The search term from the query string.
    pub term: String,
    /// The fields to search across (from endpoint spec `search` list).
    pub fields: Vec<String>,
}

impl SearchParam {
    /// Creates a new search parameter if both term and fields are non-empty.
    pub fn new(term: &str, fields: &[String]) -> Option<Self> {
        let term = term.trim();
        if term.is_empty() || fields.is_empty() {
            return None;
        }
        Some(SearchParam {
            term: term.to_string(),
            fields: fields.to_vec(),
        })
    }

    /// Appends a full-text search WHERE clause to the SQL string.
    ///
    /// Uses `to_tsvector('english', ...)` and `plainto_tsquery('english', $N)`.
    /// `param_offset` is the `$N` parameter index for the search term.
    /// Returns the new parameter offset after appending.
    pub fn apply_to_sql(&self, sql: &mut String, has_where: bool, param_offset: usize) -> usize {
        if has_where {
            sql.push_str(" AND ");
        } else {
            sql.push_str(" WHERE ");
        }

        // Build the concatenated text vector: coalesce(field1,'') || ' ' || coalesce(field2,'')
        let tsvector_expr = self
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                if i == 0 {
                    format!("COALESCE(\"{f}\", '')")
                } else {
                    format!(" || ' ' || COALESCE(\"{f}\", '')")
                }
            })
            .collect::<String>();

        sql.push_str(&format!(
            "to_tsvector('english', {tsvector_expr}) @@ plainto_tsquery('english', ${param_offset})"
        ));

        param_offset + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_param_new_valid() {
        let fields = vec!["name".to_string(), "email".to_string()];
        let sp = SearchParam::new("john", &fields);
        assert!(sp.is_some());
        let sp = sp.unwrap();
        assert_eq!(sp.term, "john");
        assert_eq!(sp.fields.len(), 2);
    }

    #[test]
    fn search_param_new_empty_term() {
        let fields = vec!["name".to_string()];
        assert!(SearchParam::new("", &fields).is_none());
        assert!(SearchParam::new("  ", &fields).is_none());
    }

    #[test]
    fn search_param_new_empty_fields() {
        assert!(SearchParam::new("john", &[]).is_none());
    }

    #[test]
    fn apply_search_single_field() {
        let sp = SearchParam {
            term: "john".to_string(),
            fields: vec!["name".to_string()],
        };

        let mut sql = "SELECT * FROM users".to_string();
        let offset = sp.apply_to_sql(&mut sql, false, 1);

        assert_eq!(
            sql,
            "SELECT * FROM users WHERE to_tsvector('english', COALESCE(\"name\", '')) @@ plainto_tsquery('english', $1)"
        );
        assert_eq!(offset, 2);
    }

    #[test]
    fn apply_search_multiple_fields() {
        let sp = SearchParam {
            term: "john".to_string(),
            fields: vec!["name".to_string(), "email".to_string()],
        };

        let mut sql = "SELECT * FROM users".to_string();
        let offset = sp.apply_to_sql(&mut sql, false, 1);

        assert_eq!(
            sql,
            "SELECT * FROM users WHERE to_tsvector('english', COALESCE(\"name\", '') || ' ' || COALESCE(\"email\", '')) @@ plainto_tsquery('english', $1)"
        );
        assert_eq!(offset, 2);
    }

    #[test]
    fn apply_search_with_existing_where() {
        let sp = SearchParam {
            term: "john".to_string(),
            fields: vec!["name".to_string()],
        };

        let mut sql = "SELECT * FROM users WHERE \"role\" = $1".to_string();
        let offset = sp.apply_to_sql(&mut sql, true, 2);

        assert!(sql.contains("AND to_tsvector"));
        assert_eq!(offset, 3);
    }
}
