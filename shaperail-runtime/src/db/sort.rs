/// Sort direction for a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl std::fmt::Display for SortDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Asc => write!(f, "ASC"),
            Self::Desc => write!(f, "DESC"),
        }
    }
}

/// A single field + direction sort instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortField {
    pub field: String,
    pub direction: SortDirection,
}

/// Parsed sort parameters from `?sort=-created_at,name`.
///
/// A leading `-` means descending; no prefix means ascending.
#[derive(Debug, Clone, Default)]
pub struct SortParam {
    pub fields: Vec<SortField>,
}

impl SortParam {
    /// Parses a sort query string value like `-created_at,name`.
    ///
    /// Only includes fields present in `allowed_fields`.
    pub fn parse(raw: &str, allowed_fields: &[String]) -> Self {
        let mut fields = Vec::new();
        for part in raw.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (direction, field_name) = if let Some(name) = part.strip_prefix('-') {
                (SortDirection::Desc, name)
            } else {
                (SortDirection::Asc, part)
            };
            if allowed_fields.iter().any(|f| f == field_name) {
                fields.push(SortField {
                    field: field_name.to_string(),
                    direction,
                });
            }
        }
        SortParam { fields }
    }

    /// Returns true if there are no sort fields.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Appends an ORDER BY clause to the given SQL string.
    pub fn apply_to_sql(&self, sql: &mut String) {
        if self.fields.is_empty() {
            return;
        }
        sql.push_str(" ORDER BY ");
        for (i, sf) in self.fields.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("\"{}\" {}", sf.field, sf.direction));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sort_params() {
        let allowed = vec![
            "created_at".to_string(),
            "name".to_string(),
            "email".to_string(),
        ];
        let sp = SortParam::parse("-created_at,name", &allowed);

        assert_eq!(sp.fields.len(), 2);
        assert_eq!(sp.fields[0].field, "created_at");
        assert_eq!(sp.fields[0].direction, SortDirection::Desc);
        assert_eq!(sp.fields[1].field, "name");
        assert_eq!(sp.fields[1].direction, SortDirection::Asc);
    }

    #[test]
    fn sort_disallowed_fields_ignored() {
        let allowed = vec!["name".to_string()];
        let sp = SortParam::parse("-secret,name", &allowed);

        assert_eq!(sp.fields.len(), 1);
        assert_eq!(sp.fields[0].field, "name");
    }

    #[test]
    fn sort_apply_to_sql() {
        let sp = SortParam {
            fields: vec![
                SortField {
                    field: "created_at".to_string(),
                    direction: SortDirection::Desc,
                },
                SortField {
                    field: "name".to_string(),
                    direction: SortDirection::Asc,
                },
            ],
        };

        let mut sql = "SELECT * FROM users".to_string();
        sp.apply_to_sql(&mut sql);

        assert_eq!(
            sql,
            "SELECT * FROM users ORDER BY \"created_at\" DESC, \"name\" ASC"
        );
    }

    #[test]
    fn empty_sort_no_clause() {
        let sp = SortParam::default();
        assert!(sp.is_empty());

        let mut sql = "SELECT * FROM users".to_string();
        sp.apply_to_sql(&mut sql);
        assert_eq!(sql, "SELECT * FROM users");
    }

    #[test]
    fn sort_direction_display() {
        assert_eq!(SortDirection::Asc.to_string(), "ASC");
        assert_eq!(SortDirection::Desc.to_string(), "DESC");
    }
}
