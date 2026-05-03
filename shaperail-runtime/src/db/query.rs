use serde::{Deserialize, Serialize};
use shaperail_core::{DatabaseEngine, FieldSchema, FieldType, ResourceDefinition, ShaperailError};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use super::filter::FilterSet;
use super::pagination::{decode_cursor, encode_cursor, PageRequest};
use super::search::SearchParam;
use super::sort::SortParam;

/// A single row returned from a resource query, represented as a JSON object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRow(pub serde_json::Value);

/// Dynamic query executor for a resource defined by a `ResourceDefinition`.
///
/// Generates and executes parameterized SQL queries against a PgPool,
/// returning results as `ResourceRow` (JSON objects).
///
/// # SQL injection safety
///
/// All user-controllable input (filter values, search term, cursor, pagination
/// offset/limit, insert/update body) is passed only as bound parameters via
/// `BindValue` and `query.bind()`. Table and column names in the SQL string
/// come solely from `ResourceDefinition` (trusted schema). Filter/sort field
/// names are allow-listed (FilterSet::from_query_params, SortParam::parse).
/// See db_integration tests `test_sql_injection_*` for verification.
pub struct ResourceQuery<'a> {
    pub resource: &'a ResourceDefinition,
    pub pool: &'a PgPool,
}

impl<'a> ResourceQuery<'a> {
    pub fn new(resource: &'a ResourceDefinition, pool: &'a PgPool) -> Self {
        Self { resource, pool }
    }

    /// Returns the table name (same as resource name).
    fn table(&self) -> &str {
        &self.resource.resource
    }

    /// Returns all column names from the schema.
    fn columns(&self) -> Vec<&str> {
        self.resource.schema.keys().map(|k| k.as_str()).collect()
    }

    /// Returns the primary key field name.
    fn primary_key(&self) -> &str {
        self.resource
            .schema
            .iter()
            .find(|(_, fs)| fs.primary)
            .map(|(name, _)| name.as_str())
            .unwrap_or("id")
    }

    /// Builds a SELECT column list.
    fn select_columns(&self) -> String {
        self.columns()
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Converts a `PgRow` to a `serde_json::Value` object based on schema field types.
    fn row_to_json(&self, row: &PgRow) -> Result<serde_json::Value, ShaperailError> {
        let mut obj = serde_json::Map::new();
        for (name, field) in &self.resource.schema {
            if field.transient {
                continue;
            }
            let value = extract_column_value(row, name, field)?;
            obj.insert(name.clone(), value);
        }
        Ok(serde_json::Value::Object(obj))
    }

    /// Returns `true` if any endpoint on this resource has `soft_delete: true`.
    fn has_soft_delete(&self) -> bool {
        self.resource
            .endpoints
            .as_ref()
            .map(|eps| eps.values().any(|ep| ep.soft_delete))
            .unwrap_or(false)
    }

    // -- Query methods --

    /// Find a single record by its primary key.
    pub async fn find_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let pk = self.primary_key();
        let soft_delete_clause = if self.has_soft_delete() {
            " AND \"deleted_at\" IS NULL"
        } else {
            ""
        };
        let sql = format!(
            "SELECT {} FROM \"{}\" WHERE \"{}\" = $1{soft_delete_clause}",
            self.select_columns(),
            self.table(),
            pk,
        );

        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or(ShaperailError::NotFound)?;

        let json = self.row_to_json(&row)?;
        Ok(ResourceRow(json))
    }

    /// Find all records with filtering, searching, sorting, and pagination.
    ///
    /// Returns `(rows, cursor_page)` for cursor pagination or `(rows, offset_page)` for offset.
    pub async fn find_all(
        &self,
        filters: &FilterSet,
        search: Option<&SearchParam>,
        sort: &SortParam,
        page: &PageRequest,
    ) -> Result<(Vec<ResourceRow>, serde_json::Value), ShaperailError> {
        let mut sql = format!("SELECT {} FROM \"{}\"", self.select_columns(), self.table());
        let mut has_where = false;
        let mut param_offset: usize = 1;
        let mut bind_values: Vec<BindValue> = Vec::new();

        // Exclude soft-deleted rows
        if self.has_soft_delete() {
            sql.push_str(" WHERE \"deleted_at\" IS NULL");
            has_where = true;
        }

        // Apply filters
        if !filters.is_empty() {
            param_offset = filters.apply_to_sql(&mut sql, has_where, param_offset);
            has_where = true;
            for f in &filters.filters {
                bind_values.push(self.coerce_filter_value(&f.field, &f.value));
            }
        }

        // Apply search
        if let Some(sp) = search {
            param_offset = sp.apply_to_sql(&mut sql, has_where, param_offset);
            has_where = true;
            bind_values.push(BindValue::Text(sp.term.clone()));
        }

        // Apply cursor/offset pagination
        match page {
            PageRequest::Cursor { after, limit } => {
                let decoded_cursor = if let Some(cursor_str) = after {
                    let id_str = decode_cursor(cursor_str)?;
                    let id = uuid::Uuid::parse_str(&id_str).map_err(|_| {
                        ShaperailError::Validation(vec![shaperail_core::FieldError {
                            field: "cursor".to_string(),
                            message: "Invalid cursor value".to_string(),
                            code: "invalid_cursor".to_string(),
                        }])
                    })?;
                    Some(id)
                } else {
                    None
                };

                if decoded_cursor.is_some() {
                    if has_where {
                        sql.push_str(" AND ");
                    } else {
                        sql.push_str(" WHERE ");
                    }
                    sql.push_str(&format!("\"id\" > ${param_offset}"));
                    bind_values.push(BindValue::Uuid(decoded_cursor.unwrap_or_default()));
                }

                // Apply sort or default to id ASC for cursor pagination
                if sort.is_empty() {
                    sql.push_str(" ORDER BY \"id\" ASC");
                } else {
                    sort.apply_to_sql(&mut sql);
                }
                sql.push_str(&format!(" LIMIT {}", limit + 1));

                let rows = self.execute_query(&sql, &bind_values).await?;

                let has_more = rows.len() as i64 > *limit;
                let result_rows: Vec<ResourceRow> =
                    rows.into_iter().take(*limit as usize).collect();

                let cursor = if has_more {
                    result_rows
                        .last()
                        .and_then(|r| r.0.get("id"))
                        .and_then(|v| v.as_str())
                        .map(encode_cursor)
                } else {
                    None
                };

                let meta = serde_json::json!({
                    "cursor": cursor,
                    "has_more": has_more,
                });
                Ok((result_rows, meta))
            }
            PageRequest::Offset { offset, limit } => {
                // For offset pagination, get total count
                let mut count_sql = format!("SELECT COUNT(*) FROM \"{}\"", self.table());
                let mut count_has_where = false;
                let mut count_offset: usize = 1;
                let mut count_binds: Vec<BindValue> = Vec::new();

                // Exclude soft-deleted rows
                if self.has_soft_delete() {
                    count_sql.push_str(" WHERE \"deleted_at\" IS NULL");
                    count_has_where = true;
                }

                if !filters.is_empty() {
                    count_offset =
                        filters.apply_to_sql(&mut count_sql, count_has_where, count_offset);
                    count_has_where = true;
                    for f in &filters.filters {
                        count_binds.push(self.coerce_filter_value(&f.field, &f.value));
                    }
                }
                if let Some(sp) = search {
                    sp.apply_to_sql(&mut count_sql, count_has_where, count_offset);
                    count_binds.push(BindValue::Text(sp.term.clone()));
                }

                let total = self.execute_count(&count_sql, &count_binds).await?;

                // Apply sort
                if !sort.is_empty() {
                    sort.apply_to_sql(&mut sql);
                }
                sql.push_str(&format!(" LIMIT {limit} OFFSET {offset}"));

                let rows = self.execute_query(&sql, &bind_values).await?;

                let meta = serde_json::json!({
                    "offset": offset,
                    "limit": limit,
                    "total": total,
                });
                Ok((rows, meta))
            }
        }
    }

    /// Insert a new record. Returns the inserted row.
    pub async fn insert(
        &self,
        data: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<ResourceRow, ShaperailError> {
        let mut columns = Vec::new();
        let mut placeholders = Vec::new();
        let mut bind_values = Vec::new();
        let mut idx = 1usize;

        // Add generated fields
        for (name, field) in &self.resource.schema {
            if field.transient {
                continue;
            }
            if field.generated {
                match field.field_type {
                    FieldType::Uuid => {
                        columns.push(format!("\"{name}\""));
                        placeholders.push(format!("${idx}"));
                        bind_values.push(BindValue::Uuid(uuid::Uuid::new_v4()));
                        idx += 1;
                    }
                    FieldType::Timestamp => {
                        columns.push(format!("\"{name}\""));
                        placeholders.push(format!("${idx}"));
                        bind_values.push(BindValue::Timestamp(chrono::Utc::now()));
                        idx += 1;
                    }
                    _ => {}
                }
                continue;
            }

            if let Some(value) = data.get(name) {
                columns.push(format!("\"{name}\""));
                placeholders.push(format!("${idx}"));
                bind_values.push(json_to_bind(value, field));
                idx += 1;
            } else if let Some(default) = &field.default {
                columns.push(format!("\"{name}\""));
                placeholders.push(format!("${idx}"));
                bind_values.push(json_to_bind(default, field));
                idx += 1;
            }
        }

        let sql = format!(
            "INSERT INTO \"{}\" ({}) VALUES ({}) RETURNING {}",
            self.table(),
            columns.join(", "),
            placeholders.join(", "),
            self.select_columns(),
        );

        let rows = self.execute_query(&sql, &bind_values).await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| ShaperailError::Internal("Insert returned no rows".to_string()))
    }

    /// Update a record by primary key. Returns the updated row.
    pub async fn update_by_id(
        &self,
        id: &uuid::Uuid,
        data: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<ResourceRow, ShaperailError> {
        let mut set_clauses = Vec::new();
        let mut bind_values = Vec::new();
        let mut idx = 1usize;

        for (name, value) in data {
            if let Some(field) = self.resource.schema.get(name) {
                if field.primary || field.generated {
                    continue;
                }
                set_clauses.push(format!("\"{name}\" = ${idx}"));
                bind_values.push(json_to_bind(value, field));
                idx += 1;
            }
        }

        // Auto-update updated_at if it exists and is generated
        if let Some(field) = self.resource.schema.get("updated_at") {
            if field.generated && field.field_type == FieldType::Timestamp {
                set_clauses.push(format!("\"updated_at\" = ${idx}"));
                bind_values.push(BindValue::Timestamp(chrono::Utc::now()));
                idx += 1;
            }
        }

        if set_clauses.is_empty() {
            return Err(ShaperailError::Validation(vec![
                shaperail_core::FieldError {
                    field: "body".to_string(),
                    message: "No valid fields to update".to_string(),
                    code: "empty_update".to_string(),
                },
            ]));
        }

        let pk = self.primary_key();
        let soft_delete_clause = if self.has_soft_delete() {
            " AND \"deleted_at\" IS NULL"
        } else {
            ""
        };
        let sql = format!(
            "UPDATE \"{}\" SET {} WHERE \"{}\" = ${}{soft_delete_clause} RETURNING {}",
            self.table(),
            set_clauses.join(", "),
            pk,
            idx,
            self.select_columns(),
        );
        bind_values.push(BindValue::Uuid(*id));

        let rows = self.execute_query(&sql, &bind_values).await?;
        rows.into_iter().next().ok_or(ShaperailError::NotFound)
    }

    /// Soft-delete a record by setting `deleted_at` to now.
    pub async fn soft_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let pk = self.primary_key();
        let sql = format!(
            "UPDATE \"{}\" SET \"deleted_at\" = $1 WHERE \"{}\" = $2 AND \"deleted_at\" IS NULL RETURNING {}",
            self.table(),
            pk,
            self.select_columns(),
        );

        let bind_values = vec![
            BindValue::Timestamp(chrono::Utc::now()),
            BindValue::Uuid(*id),
        ];

        let rows = self.execute_query(&sql, &bind_values).await?;
        rows.into_iter().next().ok_or(ShaperailError::NotFound)
    }

    /// Hard-delete a record permanently.
    pub async fn hard_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let pk = self.primary_key();
        let sql = format!(
            "DELETE FROM \"{}\" WHERE \"{}\" = $1 RETURNING {}",
            self.table(),
            pk,
            self.select_columns(),
        );

        let bind_values = vec![BindValue::Uuid(*id)];
        let rows = self.execute_query(&sql, &bind_values).await?;
        rows.into_iter().next().ok_or(ShaperailError::NotFound)
    }

    // -- Internal helpers --

    /// Coerces a filter string value to the correct `BindValue` based on the field's schema type.
    fn coerce_filter_value(&self, field_name: &str, value: &str) -> BindValue {
        if let Some(field) = self.resource.schema.get(field_name) {
            match field.field_type {
                FieldType::Uuid => {
                    if let Ok(u) = uuid::Uuid::parse_str(value) {
                        return BindValue::Uuid(u);
                    }
                }
                FieldType::Integer => {
                    if let Ok(n) = value.parse::<i64>() {
                        return BindValue::Bigint(n);
                    }
                }
                FieldType::Number => {
                    if let Ok(n) = value.parse::<f64>() {
                        return BindValue::Float(n);
                    }
                }
                FieldType::Boolean => {
                    if let Ok(b) = value.parse::<bool>() {
                        return BindValue::Bool(b);
                    }
                }
                _ => {}
            }
        }
        BindValue::Text(value.to_string())
    }

    async fn execute_query(
        &self,
        sql: &str,
        binds: &[BindValue],
    ) -> Result<Vec<ResourceRow>, ShaperailError> {
        let span = crate::observability::telemetry::db_span("query", self.table(), sql);
        let _enter = span.enter();
        let start = std::time::Instant::now();

        // Dynamic queries use sqlx::query() with bind params (not string interpolation).
        // The query_as! macro requires compile-time SQL; generated code (M04+) will use it.
        let mut query = sqlx::query(sql);
        for bind in binds {
            query = match bind {
                BindValue::Text(v) => query.bind(v),
                BindValue::Bigint(v) => query.bind(v),
                BindValue::Float(v) => query.bind(v),
                BindValue::Bool(v) => query.bind(v),
                BindValue::Uuid(v) => query.bind(v),
                BindValue::Timestamp(v) => query.bind(v),
                BindValue::Date(v) => query.bind(v),
                BindValue::Json(v) => query.bind(v),
                BindValue::Null => query.bind(None::<String>),
            };
        }

        let pg_rows = query.fetch_all(self.pool).await?;
        let duration_ms = start.elapsed().as_millis() as u64;
        log_slow_query(sql, duration_ms);

        let mut results = Vec::with_capacity(pg_rows.len());
        for row in &pg_rows {
            results.push(ResourceRow(self.row_to_json(row)?));
        }
        Ok(results)
    }

    async fn execute_count(&self, sql: &str, binds: &[BindValue]) -> Result<i64, ShaperailError> {
        let span = crate::observability::telemetry::db_span("count", self.table(), sql);
        let _enter = span.enter();
        let start = std::time::Instant::now();

        let mut query = sqlx::query_scalar::<_, i64>(sql);
        for bind in binds {
            query = match bind {
                BindValue::Text(v) => query.bind(v),
                BindValue::Bigint(v) => query.bind(v),
                BindValue::Float(v) => query.bind(v),
                BindValue::Bool(v) => query.bind(v),
                BindValue::Uuid(v) => query.bind(v),
                BindValue::Timestamp(v) => query.bind(v),
                BindValue::Date(v) => query.bind(v),
                BindValue::Json(v) => query.bind(v),
                BindValue::Null => query.bind(None::<String>),
            };
        }
        let count = query.fetch_one(self.pool).await?;
        let duration_ms = start.elapsed().as_millis() as u64;
        log_slow_query(sql, duration_ms);

        Ok(count)
    }
}

/// Internal enum for type-safe query parameter binding.
#[derive(Debug, Clone)]
enum BindValue {
    Text(String),
    Bigint(i64),
    Float(f64),
    Bool(bool),
    Uuid(uuid::Uuid),
    Timestamp(chrono::DateTime<chrono::Utc>),
    Date(chrono::NaiveDate),
    Json(serde_json::Value),
    Null,
}

/// Converts a JSON value to the appropriate `BindValue` based on the field schema.
fn json_to_bind(value: &serde_json::Value, field: &FieldSchema) -> BindValue {
    if value.is_null() {
        return BindValue::Null;
    }
    match field.field_type {
        FieldType::Uuid => {
            if let Some(s) = value.as_str() {
                if let Ok(u) = uuid::Uuid::parse_str(s) {
                    return BindValue::Uuid(u);
                }
            }
            BindValue::Text(value.to_string().trim_matches('"').to_string())
        }
        FieldType::String | FieldType::Enum | FieldType::File => {
            BindValue::Text(value.as_str().unwrap_or(&value.to_string()).to_string())
        }
        FieldType::Integer => BindValue::Bigint(value.as_i64().unwrap_or(0)),
        FieldType::Number => BindValue::Float(value.as_f64().unwrap_or(0.0)),
        FieldType::Boolean => BindValue::Bool(value.as_bool().unwrap_or(false)),
        FieldType::Timestamp => {
            if let Some(s) = value.as_str() {
                if let Ok(dt) = s.parse::<chrono::DateTime<chrono::Utc>>() {
                    return BindValue::Timestamp(dt);
                }
            }
            BindValue::Timestamp(chrono::Utc::now())
        }
        FieldType::Date => {
            if let Some(s) = value.as_str() {
                if let Ok(d) = s.parse::<chrono::NaiveDate>() {
                    return BindValue::Date(d);
                }
            }
            BindValue::Date(chrono::Utc::now().date_naive())
        }
        FieldType::Json | FieldType::Array => BindValue::Json(value.clone()),
    }
}

/// Extracts a column value from a `PgRow` as a `serde_json::Value`.
fn extract_column_value(
    row: &PgRow,
    name: &str,
    field: &FieldSchema,
) -> Result<serde_json::Value, ShaperailError> {
    // Try to get the column; if it doesn't exist, return null
    let map_err = |e: sqlx::Error| ShaperailError::Internal(format!("Column '{name}' error: {e}"));

    match field.field_type {
        FieldType::Uuid => {
            let v: Option<uuid::Uuid> = row.try_get(name).map_err(map_err)?;
            Ok(v.map(|u| serde_json::Value::String(u.to_string()))
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::String | FieldType::Enum | FieldType::File => {
            let v: Option<String> = row.try_get(name).map_err(map_err)?;
            Ok(v.map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Integer => {
            let v: Option<i64> = row.try_get(name).map_err(map_err)?;
            Ok(v.map(|n| serde_json::Value::Number(n.into()))
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Number => {
            let v: Option<f64> = row.try_get(name).map_err(map_err)?;
            Ok(
                v.and_then(|n| serde_json::Number::from_f64(n).map(serde_json::Value::Number))
                    .unwrap_or(serde_json::Value::Null),
            )
        }
        FieldType::Boolean => {
            let v: Option<bool> = row.try_get(name).map_err(map_err)?;
            Ok(v.map(serde_json::Value::Bool)
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Timestamp => {
            let v: Option<chrono::DateTime<chrono::Utc>> = row.try_get(name).map_err(map_err)?;
            Ok(v.map(|dt| serde_json::Value::String(dt.to_rfc3339()))
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Date => {
            let v: Option<chrono::NaiveDate> = row.try_get(name).map_err(map_err)?;
            Ok(v.map(|d| serde_json::Value::String(d.to_string()))
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Json | FieldType::Array => {
            let v: Option<serde_json::Value> = row.try_get(name).map_err(map_err)?;
            Ok(v.unwrap_or(serde_json::Value::Null))
        }
    }
}

/// Logs a warning if a query exceeds the slow query threshold.
///
/// The threshold is read from `SHAPERAIL_SLOW_QUERY_MS` env var (default: 100ms).
fn log_slow_query(sql: &str, duration_ms: u64) {
    let threshold: u64 = std::env::var("SHAPERAIL_SLOW_QUERY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    if duration_ms >= threshold {
        tracing::warn!(
            duration_ms = duration_ms,
            sql = %sql,
            threshold_ms = threshold,
            "Slow query detected"
        );
    }
}

/// Builds a SQL `CREATE TABLE` statement for the given engine (M14 multi-DB).
///
/// Returns engine-specific SQL (Postgres, MySQL, or SQLite). MongoDB is not supported for CREATE TABLE.
pub fn build_create_table_sql_for_engine(
    engine: DatabaseEngine,
    resource: &ResourceDefinition,
) -> String {
    match engine {
        DatabaseEngine::Postgres => build_create_table_sql_postgres(resource),
        DatabaseEngine::MySQL => build_create_table_sql_mysql(resource),
        DatabaseEngine::SQLite => build_create_table_sql_sqlite(resource),
        DatabaseEngine::MongoDB => {
            // MongoDB uses collections, not CREATE TABLE
            String::new()
        }
    }
}

/// Builds a SQL `CREATE TABLE` statement from a `ResourceDefinition` (Postgres).
///
/// Used by the migration generator to produce initial table creation SQL.
pub fn build_create_table_sql(resource: &ResourceDefinition) -> String {
    build_create_table_sql_postgres(resource)
}

fn build_create_table_sql_postgres(resource: &ResourceDefinition) -> String {
    let mut columns = Vec::new();
    let mut constraints = Vec::new();
    let has_soft_delete = resource
        .endpoints
        .as_ref()
        .map(|eps| eps.values().any(|ep| ep.soft_delete))
        .unwrap_or(false);

    for (name, field) in &resource.schema {
        if field.transient {
            continue;
        }
        let mut col = format!(
            "\"{}\" {}",
            name,
            field_type_to_sql(&field.field_type, field)
        );

        if field.primary {
            col.push_str(" PRIMARY KEY");
        }
        if field.required && !field.primary && !field.nullable {
            col.push_str(" NOT NULL");
        }
        if field.unique && !field.primary {
            col.push_str(" UNIQUE");
        }
        if let Some(default) = &field.default {
            col.push_str(&format!(" DEFAULT {}", sql_default_value(default, field)));
        }
        if field.field_type == FieldType::Uuid && field.generated {
            col.push_str(" DEFAULT gen_random_uuid()");
        }
        if field.field_type == FieldType::Timestamp && field.generated {
            col.push_str(" DEFAULT NOW()");
        }
        if field.field_type == FieldType::Date && field.generated {
            col.push_str(" DEFAULT CURRENT_DATE");
        }

        // Enum CHECK constraint
        if field.field_type == FieldType::Enum {
            if let Some(values) = &field.values {
                let vals = values
                    .iter()
                    .map(|v| format!("'{v}'"))
                    .collect::<Vec<_>>()
                    .join(", ");
                constraints.push(format!(
                    "CONSTRAINT \"chk_{table}_{name}\" CHECK (\"{name}\" IN ({vals}))",
                    table = resource.resource,
                ));
            }
        }

        // Foreign key constraint
        if let Some(reference) = &field.reference {
            if let Some((ref_table, ref_col)) = reference.split_once('.') {
                constraints.push(format!(
                    "CONSTRAINT \"fk_{table}_{name}\" FOREIGN KEY (\"{name}\") REFERENCES \"{ref_table}\"(\"{ref_col}\")",
                    table = resource.resource,
                ));
            }
        }

        columns.push(col);
    }

    if has_soft_delete && !resource.schema.contains_key("deleted_at") {
        columns.push("\"deleted_at\" TIMESTAMPTZ".to_string());
    }

    let mut sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" (\n  {}",
        resource.resource,
        columns.join(",\n  ")
    );

    if !constraints.is_empty() {
        sql.push_str(",\n  ");
        sql.push_str(&constraints.join(",\n  "));
    }
    sql.push_str("\n)");

    // Add indexes
    if let Some(indexes) = &resource.indexes {
        for (i, idx) in indexes.iter().enumerate() {
            let idx_cols = idx
                .fields
                .iter()
                .map(|f| format!("\"{f}\""))
                .collect::<Vec<_>>()
                .join(", ");
            let unique = if idx.unique { "UNIQUE " } else { "" };
            let order = idx
                .order
                .as_deref()
                .map(|o| format!(" {}", o.to_uppercase()))
                .unwrap_or_default();
            sql.push_str(&format!(
                ";\nCREATE {unique}INDEX IF NOT EXISTS \"idx_{}_{i}\" ON \"{}\" ({idx_cols}{order})",
                resource.resource, resource.resource,
            ));
        }
    }

    sql
}

fn build_create_table_sql_mysql(resource: &ResourceDefinition) -> String {
    let q = |s: &str| format!("`{s}`");
    let mut columns = Vec::new();
    let mut constraints = Vec::new();
    let has_soft_delete = resource
        .endpoints
        .as_ref()
        .map(|eps| eps.values().any(|ep| ep.soft_delete))
        .unwrap_or(false);

    for (name, field) in &resource.schema {
        if field.transient {
            continue;
        }
        let mut col = format!(
            "{} {}",
            q(name),
            field_type_to_sql_mysql(&field.field_type, field)
        );
        if field.primary {
            col.push_str(" PRIMARY KEY");
        }
        if field.required && !field.primary && !field.nullable {
            col.push_str(" NOT NULL");
        }
        if field.unique && !field.primary {
            col.push_str(" UNIQUE");
        }
        if let Some(default) = &field.default {
            col.push_str(&format!(" DEFAULT {}", sql_default_value(default, field)));
        }
        if field.field_type == FieldType::Uuid && field.generated {
            col.push_str(" DEFAULT (UUID())");
        }
        if field.field_type == FieldType::Timestamp && field.generated {
            col.push_str(" DEFAULT (CURRENT_TIMESTAMP)");
        }
        if field.field_type == FieldType::Date && field.generated {
            col.push_str(" DEFAULT (CURDATE())");
        }
        if field.field_type == FieldType::Enum {
            if let Some(values) = &field.values {
                let vals = values
                    .iter()
                    .map(|v| format!("'{v}'"))
                    .collect::<Vec<_>>()
                    .join(", ");
                constraints.push(format!(
                    "CONSTRAINT chk_{}_{} CHECK ({} IN ({vals}))",
                    resource.resource,
                    name,
                    q(name),
                ));
            }
        }
        if let Some(reference) = &field.reference {
            if let Some((ref_table, ref_col)) = reference.split_once('.') {
                constraints.push(format!(
                    "CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {}({})",
                    resource.resource,
                    name,
                    q(name),
                    q(ref_table),
                    q(ref_col),
                ));
            }
        }
        columns.push(col);
    }
    if has_soft_delete && !resource.schema.contains_key("deleted_at") {
        columns.push(format!("{} DATETIME", q("deleted_at")));
    }
    let mut sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (\n  {}",
        q(&resource.resource),
        columns.join(",\n  ")
    );
    if !constraints.is_empty() {
        sql.push_str(",\n  ");
        sql.push_str(&constraints.join(",\n  "));
    }
    sql.push_str("\n)");
    if let Some(indexes) = &resource.indexes {
        for (i, idx) in indexes.iter().enumerate() {
            let idx_cols = idx
                .fields
                .iter()
                .map(|f| q(f))
                .collect::<Vec<_>>()
                .join(", ");
            let unique = if idx.unique { "UNIQUE " } else { "" };
            let order = idx
                .order
                .as_deref()
                .map(|o| format!(" {}", o.to_uppercase()))
                .unwrap_or_default();
            sql.push_str(&format!(
                ";\nCREATE {unique}INDEX idx_{resource}_{i} ON {tbl} ({idx_cols}{order})",
                unique = unique,
                resource = resource.resource,
                i = i,
                tbl = q(&resource.resource),
                idx_cols = idx_cols,
                order = order,
            ));
        }
    }
    sql
}

fn build_create_table_sql_sqlite(resource: &ResourceDefinition) -> String {
    let q = |s: &str| format!("\"{s}\"");
    let mut columns = Vec::new();
    let mut constraints = Vec::new();
    let has_soft_delete = resource
        .endpoints
        .as_ref()
        .map(|eps| eps.values().any(|ep| ep.soft_delete))
        .unwrap_or(false);

    for (name, field) in &resource.schema {
        if field.transient {
            continue;
        }
        let mut col = format!(
            "{} {}",
            q(name),
            field_type_to_sql_sqlite(&field.field_type, field)
        );
        if field.primary {
            col.push_str(" PRIMARY KEY");
        }
        if field.required && !field.primary && !field.nullable {
            col.push_str(" NOT NULL");
        }
        if field.unique && !field.primary {
            col.push_str(" UNIQUE");
        }
        if let Some(default) = &field.default {
            col.push_str(&format!(" DEFAULT {}", sql_default_value(default, field)));
        }
        if field.field_type == FieldType::Uuid && field.generated {
            col.push_str(" DEFAULT (lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || substr('89ab',abs(random()) % 4 + 1, 1) || lower(hex(randomblob(2))) || '-' || lower(hex(randomblob(6))))");
        }
        if field.field_type == FieldType::Timestamp && field.generated {
            col.push_str(" DEFAULT (datetime('now'))");
        }
        if field.field_type == FieldType::Date && field.generated {
            col.push_str(" DEFAULT (date('now'))");
        }
        if field.field_type == FieldType::Enum {
            if let Some(values) = &field.values {
                let vals = values
                    .iter()
                    .map(|v| format!("'{v}'"))
                    .collect::<Vec<_>>()
                    .join(", ");
                constraints.push(format!(
                    "CONSTRAINT chk_{}_{} CHECK ({} IN ({vals}))",
                    resource.resource,
                    name,
                    q(name),
                ));
            }
        }
        if let Some(reference) = &field.reference {
            if let Some((ref_table, ref_col)) = reference.split_once('.') {
                constraints.push(format!(
                    "CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {}({})",
                    resource.resource,
                    name,
                    q(name),
                    q(ref_table),
                    q(ref_col),
                ));
            }
        }
        columns.push(col);
    }
    if has_soft_delete && !resource.schema.contains_key("deleted_at") {
        columns.push(format!("{} TEXT", q("deleted_at")));
    }
    let mut sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (\n  {}",
        q(&resource.resource),
        columns.join(",\n  ")
    );
    if !constraints.is_empty() {
        sql.push_str(",\n  ");
        sql.push_str(&constraints.join(",\n  "));
    }
    sql.push_str("\n)");
    if let Some(indexes) = &resource.indexes {
        for (i, idx) in indexes.iter().enumerate() {
            let idx_cols = idx
                .fields
                .iter()
                .map(|f| q(f))
                .collect::<Vec<_>>()
                .join(", ");
            let unique = if idx.unique { "UNIQUE " } else { "" };
            let order = idx
                .order
                .as_deref()
                .map(|o| format!(" {}", o.to_uppercase()))
                .unwrap_or_default();
            sql.push_str(&format!(
                ";\nCREATE {unique}INDEX IF NOT EXISTS idx_{resource}_{i} ON {tbl} ({idx_cols}{order})",
                unique = unique,
                resource = resource.resource,
                i = i,
                tbl = q(&resource.resource),
                idx_cols = idx_cols,
                order = order,
            ));
        }
    }
    sql
}

/// Maps a `FieldType` to its PostgreSQL SQL type string.
fn field_type_to_sql(ft: &FieldType, field: &FieldSchema) -> String {
    match ft {
        FieldType::Uuid => "UUID".to_string(),
        FieldType::String => {
            if let Some(max) = &field.max {
                if let Some(n) = max.as_u64() {
                    return format!("VARCHAR({n})");
                }
            }
            "TEXT".to_string()
        }
        FieldType::Integer => "BIGINT".to_string(),
        FieldType::Number => "NUMERIC".to_string(),
        FieldType::Boolean => "BOOLEAN".to_string(),
        FieldType::Timestamp => "TIMESTAMPTZ".to_string(),
        FieldType::Date => "DATE".to_string(),
        FieldType::Enum => "TEXT".to_string(),
        FieldType::Json => "JSONB".to_string(),
        FieldType::Array => {
            if let Some(items) = &field.items {
                let item_sql = match items.field_type {
                    FieldType::String | FieldType::Enum => "TEXT",
                    FieldType::Integer => "BIGINT",
                    FieldType::Number => "DOUBLE PRECISION",
                    FieldType::Boolean => "BOOLEAN",
                    FieldType::Timestamp => "TIMESTAMPTZ",
                    FieldType::Date => "DATE",
                    FieldType::Uuid => "UUID",
                    _ => "TEXT",
                };
                format!("{item_sql}[]")
            } else {
                "TEXT[]".to_string()
            }
        }
        FieldType::File => "TEXT".to_string(),
    }
}

fn field_type_to_sql_mysql(ft: &FieldType, field: &FieldSchema) -> String {
    match ft {
        FieldType::Uuid => "CHAR(36)".to_string(),
        FieldType::String => {
            if let Some(max) = &field.max {
                if let Some(n) = max.as_u64() {
                    return format!("VARCHAR({n})");
                }
            }
            "TEXT".to_string()
        }
        FieldType::Integer => "BIGINT".to_string(),
        FieldType::Number => "DECIMAL(65,30)".to_string(),
        FieldType::Boolean => "BOOLEAN".to_string(),
        FieldType::Timestamp => "DATETIME(6)".to_string(),
        FieldType::Date => "DATE".to_string(),
        FieldType::Enum => "VARCHAR(255)".to_string(),
        FieldType::Json => "JSON".to_string(),
        FieldType::Array => "JSON".to_string(),
        FieldType::File => "TEXT".to_string(),
    }
}

fn field_type_to_sql_sqlite(ft: &FieldType, field: &FieldSchema) -> String {
    match ft {
        FieldType::Uuid => "TEXT".to_string(),
        FieldType::String => {
            if let Some(max) = &field.max {
                if let Some(n) = max.as_u64() {
                    return format!("VARCHAR({n})");
                }
            }
            "TEXT".to_string()
        }
        FieldType::Integer => "INTEGER".to_string(),
        FieldType::Number => "REAL".to_string(),
        FieldType::Boolean => "INTEGER".to_string(),
        FieldType::Timestamp => "TEXT".to_string(),
        FieldType::Date => "TEXT".to_string(),
        FieldType::Enum => "TEXT".to_string(),
        FieldType::Json => "TEXT".to_string(),
        FieldType::Array => "TEXT".to_string(),
        FieldType::File => "TEXT".to_string(),
    }
}

/// Converts a JSON default value to a SQL literal.
fn sql_default_value(value: &serde_json::Value, _field: &FieldSchema) -> String {
    match value {
        serde_json::Value::String(s) => format!("'{s}'"),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string().to_uppercase(),
        serde_json::Value::Null => "NULL".to_string(),
        other => format!("'{}'", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use shaperail_core::IndexSpec;

    fn test_resource() -> ResourceDefinition {
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
                transient: false,
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
                transient: false,
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
                transient: false,
            },
        );
        schema.insert(
            "role".to_string(),
            FieldSchema {
                field_type: FieldType::Enum,
                primary: false,
                generated: false,
                required: true,
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
                transient: false,
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
                transient: false,
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
                transient: false,
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
                transient: false,
            },
        );

        ResourceDefinition {
            resource: "users".to_string(),
            version: 1,
            db: None,
            tenant_key: None,
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

    #[test]
    fn create_table_sql_basic() {
        let resource = test_resource();
        let sql = build_create_table_sql(&resource);

        assert!(sql.contains("CREATE TABLE IF NOT EXISTS \"users\""));
        assert!(sql.contains("\"id\" UUID PRIMARY KEY DEFAULT gen_random_uuid()"));
        assert!(sql.contains("\"email\" VARCHAR(255) NOT NULL UNIQUE"));
        assert!(sql.contains("\"name\" VARCHAR(200) NOT NULL"));
        assert!(sql.contains("\"role\" TEXT NOT NULL DEFAULT 'member'"));
        assert!(sql.contains("\"org_id\" UUID NOT NULL"));
        assert!(sql.contains("\"created_at\" TIMESTAMPTZ DEFAULT NOW()"));
        assert!(sql.contains("\"updated_at\" TIMESTAMPTZ DEFAULT NOW()"));
    }

    #[test]
    fn create_table_sql_constraints() {
        let resource = test_resource();
        let sql = build_create_table_sql(&resource);

        assert!(sql.contains("CONSTRAINT \"chk_users_role\" CHECK"));
        assert!(sql.contains("'admin', 'member', 'viewer'"));
        assert!(sql.contains("CONSTRAINT \"fk_users_org_id\" FOREIGN KEY"));
        assert!(sql.contains("REFERENCES \"organizations\"(\"id\")"));
    }

    #[test]
    fn create_table_sql_indexes() {
        let resource = test_resource();
        let sql = build_create_table_sql(&resource);

        assert!(sql.contains(
            "CREATE INDEX IF NOT EXISTS \"idx_users_0\" ON \"users\" (\"org_id\", \"role\")"
        ));
        assert!(sql.contains(
            "CREATE INDEX IF NOT EXISTS \"idx_users_1\" ON \"users\" (\"created_at\" DESC)"
        ));
    }

    #[test]
    fn create_table_sql_for_engine_mysql() {
        let resource = test_resource();
        let sql = build_create_table_sql_for_engine(DatabaseEngine::MySQL, &resource);
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS `users`"));
        assert!(sql.contains("CHAR(36)"));
        assert!(sql.contains("DEFAULT (UUID())"));
        assert!(sql.contains("DEFAULT (CURRENT_TIMESTAMP)"));
    }

    #[test]
    fn create_table_sql_for_engine_sqlite() {
        let resource = test_resource();
        let sql = build_create_table_sql_for_engine(DatabaseEngine::SQLite, &resource);
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS \"users\""));
        assert!(sql.contains("DEFAULT (datetime('now'))"));
    }

    #[test]
    fn create_table_sql_adds_deleted_at_for_soft_delete() {
        let mut resource = test_resource();
        resource.endpoints = Some(indexmap::IndexMap::from([(
            "delete".to_string(),
            shaperail_core::EndpointSpec {
                method: Some(shaperail_core::HttpMethod::Delete),
                path: Some("/users/:id".to_string()),
                soft_delete: true,
                ..Default::default()
            },
        )]));

        let sql = build_create_table_sql(&resource);
        assert!(sql.contains("\"deleted_at\" TIMESTAMPTZ"));
    }

    #[test]
    fn field_type_to_sql_mapping() {
        let default_field = FieldSchema {
            field_type: FieldType::String,
            primary: false,
            generated: false,
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
            transient: false,
        };

        assert_eq!(field_type_to_sql(&FieldType::Uuid, &default_field), "UUID");
        assert_eq!(
            field_type_to_sql(&FieldType::String, &default_field),
            "TEXT"
        );
        assert_eq!(
            field_type_to_sql(&FieldType::Integer, &default_field),
            "BIGINT"
        );
        assert_eq!(
            field_type_to_sql(&FieldType::Number, &default_field),
            "NUMERIC"
        );
        assert_eq!(
            field_type_to_sql(&FieldType::Boolean, &default_field),
            "BOOLEAN"
        );
        assert_eq!(
            field_type_to_sql(&FieldType::Timestamp, &default_field),
            "TIMESTAMPTZ"
        );
        assert_eq!(field_type_to_sql(&FieldType::Date, &default_field), "DATE");
        assert_eq!(field_type_to_sql(&FieldType::Enum, &default_field), "TEXT");
        assert_eq!(field_type_to_sql(&FieldType::Json, &default_field), "JSONB");
        assert_eq!(field_type_to_sql(&FieldType::File, &default_field), "TEXT");
    }

    #[test]
    fn field_type_to_sql_varchar() {
        let field = FieldSchema {
            field_type: FieldType::String,
            primary: false,
            generated: false,
            required: false,
            unique: false,
            nullable: false,
            reference: None,
            min: None,
            max: Some(serde_json::json!(100)),
            format: None,
            values: None,
            default: None,
            sensitive: false,
            search: false,
            items: None,
            transient: false,
        };
        assert_eq!(
            field_type_to_sql(&FieldType::String, &field),
            "VARCHAR(100)"
        );
    }

    #[test]
    fn field_type_to_sql_array() {
        let field = FieldSchema {
            field_type: FieldType::Array,
            primary: false,
            generated: false,
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
            items: Some(shaperail_core::ItemsSpec::of(
                shaperail_core::FieldType::String,
            )),
            transient: false,
        };
        assert_eq!(field_type_to_sql(&FieldType::Array, &field), "TEXT[]");
    }

    #[test]
    fn json_to_bind_types() {
        let str_field = FieldSchema {
            field_type: FieldType::String,
            primary: false,
            generated: false,
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
            transient: false,
        };

        let bind = json_to_bind(&serde_json::json!("hello"), &str_field);
        assert!(matches!(bind, BindValue::Text(s) if s == "hello"));

        let bind = json_to_bind(&serde_json::Value::Null, &str_field);
        assert!(matches!(bind, BindValue::Null));
    }

    fn default_field() -> FieldSchema {
        FieldSchema {
            field_type: FieldType::String,
            primary: false,
            generated: false,
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
            transient: false,
        }
    }

    #[test]
    fn integer_field_emits_bigint() {
        use shaperail_core::FieldType;
        let f = default_field();
        assert_eq!(field_type_to_sql(&FieldType::Integer, &f), "BIGINT");
    }
}
