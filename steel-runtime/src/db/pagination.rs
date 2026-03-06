use serde::{Deserialize, Serialize};

/// Request parameters for pagination.
#[derive(Debug, Clone)]
pub enum PageRequest {
    /// Cursor-based: fetch `limit` rows after the given cursor (base64-encoded id).
    Cursor { after: Option<String>, limit: i64 },
    /// Offset-based: skip `offset` rows, fetch `limit` rows.
    Offset { offset: i64, limit: i64 },
}

impl PageRequest {
    /// Default page size.
    pub const DEFAULT_LIMIT: i64 = 25;
    /// Maximum allowed page size.
    pub const MAX_LIMIT: i64 = 100;

    /// Clamps the limit to the allowed range [1, MAX_LIMIT].
    pub fn clamped_limit(limit: Option<i64>) -> i64 {
        limit
            .unwrap_or(Self::DEFAULT_LIMIT)
            .clamp(1, Self::MAX_LIMIT)
    }
}

/// Response metadata for cursor-based pagination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPage {
    /// Opaque cursor for the next page (base64-encoded id of last row).
    pub cursor: Option<String>,
    /// Whether there are more rows after this page.
    pub has_more: bool,
}

/// Response metadata for offset-based pagination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffsetPage {
    /// Current offset.
    pub offset: i64,
    /// Page size used.
    pub limit: i64,
    /// Total number of matching rows.
    pub total: i64,
}

impl PageRequest {
    /// Appends cursor pagination clauses to the SQL string.
    ///
    /// For cursor pagination, adds `WHERE "id" > $N ORDER BY "id" ASC LIMIT N+1`
    /// (fetches one extra row to determine `has_more`).
    /// Returns the new parameter offset.
    pub fn apply_cursor_to_sql(
        &self,
        sql: &mut String,
        has_where: bool,
        param_offset: usize,
    ) -> usize {
        match self {
            PageRequest::Cursor { after, limit } => {
                let mut offset = param_offset;
                if after.is_some() {
                    if has_where {
                        sql.push_str(" AND ");
                    } else {
                        sql.push_str(" WHERE ");
                    }
                    sql.push_str(&format!("\"id\" > ${offset}"));
                    offset += 1;
                }
                sql.push_str(" ORDER BY \"id\" ASC");
                // Fetch limit+1 to detect has_more
                sql.push_str(&format!(" LIMIT {}", limit + 1));
                offset
            }
            PageRequest::Offset { offset: off, limit } => {
                sql.push_str(&format!(" LIMIT {limit} OFFSET {off}"));
                param_offset
            }
        }
    }
}

/// Decodes a cursor string (base64-encoded UUID) to a UUID string.
pub fn decode_cursor(cursor: &str) -> Result<String, steel_core::SteelError> {
    use std::str;
    // We use simple base64 encoding of the UUID string
    let bytes = base64_decode(cursor).map_err(|_| {
        steel_core::SteelError::Validation(vec![steel_core::FieldError {
            field: "cursor".to_string(),
            message: "Invalid cursor format".to_string(),
            code: "invalid_cursor".to_string(),
        }])
    })?;
    let id = str::from_utf8(&bytes).map_err(|_| {
        steel_core::SteelError::Validation(vec![steel_core::FieldError {
            field: "cursor".to_string(),
            message: "Invalid cursor encoding".to_string(),
            code: "invalid_cursor".to_string(),
        }])
    })?;
    Ok(id.to_string())
}

/// Encodes a UUID string as a base64 cursor.
pub fn encode_cursor(id: &str) -> String {
    base64_encode(id.as_bytes())
}

// Simple base64 encode/decode (no external dep needed for this)
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    fn char_to_val(c: u8) -> Result<u32, &'static str> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((c - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            b'=' => Ok(0),
            _ => Err("invalid base64 character"),
        }
    }

    let bytes = input.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("invalid base64 length");
    }

    let mut result = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let a = char_to_val(chunk[0])?;
        let b = char_to_val(chunk[1])?;
        let c = char_to_val(chunk[2])?;
        let d = char_to_val(chunk[3])?;
        let triple = (a << 18) | (b << 12) | (c << 6) | d;
        result.push((triple >> 16) as u8);
        if chunk[2] != b'=' {
            result.push((triple >> 8) as u8);
        }
        if chunk[3] != b'=' {
            result.push(triple as u8);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamped_limit_default() {
        assert_eq!(PageRequest::clamped_limit(None), 25);
    }

    #[test]
    fn clamped_limit_within_range() {
        assert_eq!(PageRequest::clamped_limit(Some(10)), 10);
        assert_eq!(PageRequest::clamped_limit(Some(50)), 50);
    }

    #[test]
    fn clamped_limit_too_high() {
        assert_eq!(PageRequest::clamped_limit(Some(500)), 100);
    }

    #[test]
    fn clamped_limit_too_low() {
        assert_eq!(PageRequest::clamped_limit(Some(0)), 1);
        assert_eq!(PageRequest::clamped_limit(Some(-5)), 1);
    }

    #[test]
    fn cursor_encode_decode_roundtrip() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let encoded = encode_cursor(id);
        let decoded = decode_cursor(&encoded).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn invalid_cursor_returns_error() {
        let result = decode_cursor("!!invalid!!");
        assert!(result.is_err());
    }

    #[test]
    fn cursor_pagination_sql_no_cursor() {
        let page = PageRequest::Cursor {
            after: None,
            limit: 25,
        };
        let mut sql = "SELECT * FROM users".to_string();
        let offset = page.apply_cursor_to_sql(&mut sql, false, 1);

        assert_eq!(sql, "SELECT * FROM users ORDER BY \"id\" ASC LIMIT 26");
        assert_eq!(offset, 1);
    }

    #[test]
    fn cursor_pagination_sql_with_cursor() {
        let page = PageRequest::Cursor {
            after: Some("some-uuid".to_string()),
            limit: 10,
        };
        let mut sql = "SELECT * FROM users".to_string();
        let offset = page.apply_cursor_to_sql(&mut sql, false, 1);

        assert_eq!(
            sql,
            "SELECT * FROM users WHERE \"id\" > $1 ORDER BY \"id\" ASC LIMIT 11"
        );
        assert_eq!(offset, 2);
    }

    #[test]
    fn cursor_pagination_with_existing_where() {
        let page = PageRequest::Cursor {
            after: Some("some-uuid".to_string()),
            limit: 10,
        };
        let mut sql = "SELECT * FROM users WHERE \"role\" = $1".to_string();
        let offset = page.apply_cursor_to_sql(&mut sql, true, 2);

        assert!(sql.contains("AND \"id\" > $2"));
        assert!(sql.contains("LIMIT 11"));
        assert_eq!(offset, 3);
    }

    #[test]
    fn offset_pagination_sql() {
        let page = PageRequest::Offset {
            offset: 20,
            limit: 10,
        };
        let mut sql = "SELECT * FROM users".to_string();
        let offset = page.apply_cursor_to_sql(&mut sql, false, 1);

        assert_eq!(sql, "SELECT * FROM users LIMIT 10 OFFSET 20");
        assert_eq!(offset, 1);
    }
}
