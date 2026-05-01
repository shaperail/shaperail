//! ORM-backed resource query (M14). Builds dialect-aware SQL and executes via SeaORM.
//!
//! Supports Postgres, MySQL, and SQLite. Uses each connection's engine to determine
//! identifier quoting (`"` vs `` ` ``), parameter syntax (`$N` vs `?`), and backend.

use shaperail_core::{FieldSchema, FieldType, ResourceDefinition, ShaperailError};

use super::filter::FilterSet;
use super::pagination::{decode_cursor, encode_cursor, PageRequest};
use super::search::SearchParam;
use super::sort::SortParam;
use super::{ResourceRow, SqlConnection};
use crate::observability::telemetry;
use sea_orm::ConnectionTrait;

/// Converts a SeaORM QueryResult row to a JSON object using the resource schema.
fn query_result_to_json(
    row: &sea_orm::QueryResult,
    resource: &ResourceDefinition,
) -> Result<serde_json::Value, ShaperailError> {
    let mut obj = serde_json::Map::new();
    for (name, field) in &resource.schema {
        if field.transient {
            continue;
        }
        let value = get_column_value(row, name, field)?;
        obj.insert(name.clone(), value);
    }
    Ok(serde_json::Value::Object(obj))
}

fn get_column_value(
    row: &sea_orm::QueryResult,
    name: &str,
    field: &FieldSchema,
) -> Result<serde_json::Value, ShaperailError> {
    let map_err =
        |e: sea_orm::DbErr| ShaperailError::Internal(format!("Column '{name}' error: {e}"));
    match field.field_type {
        FieldType::Uuid => {
            // MySQL/SQLite store UUIDs as strings; Postgres as native UUID.
            let v: Option<String> = row.try_get("", name).map_err(map_err)?;
            Ok(v.map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::String | FieldType::Enum | FieldType::File => {
            let v: Option<String> = row.try_get("", name).map_err(map_err)?;
            Ok(v.map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Integer => {
            let v: Option<i32> = row.try_get("", name).map_err(map_err)?;
            Ok(v.map(|n| serde_json::Value::Number(n.into()))
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Bigint => {
            let v: Option<i64> = row.try_get("", name).map_err(map_err)?;
            Ok(v.map(|n| serde_json::Value::Number(n.into()))
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Number => {
            let v: Option<f64> = row.try_get("", name).map_err(map_err)?;
            Ok(
                v.and_then(|n| serde_json::Number::from_f64(n).map(serde_json::Value::Number))
                    .unwrap_or(serde_json::Value::Null),
            )
        }
        FieldType::Boolean => {
            let v: Option<bool> = row.try_get("", name).map_err(map_err)?;
            Ok(v.map(serde_json::Value::Bool)
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Timestamp => {
            let v: Option<String> = row.try_get("", name).map_err(map_err)?;
            Ok(v.map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Date => {
            let v: Option<String> = row.try_get("", name).map_err(map_err)?;
            Ok(v.map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Json | FieldType::Array => {
            let v: Option<serde_json::Value> = row.try_get("", name).map_err(map_err)?;
            Ok(v.unwrap_or(serde_json::Value::Null))
        }
    }
}

fn json_to_sea_value(value: &serde_json::Value, field: &FieldSchema) -> sea_query::Value {
    if value.is_null() {
        return sea_query::Value::String(None);
    }
    match field.field_type {
        FieldType::Uuid => {
            // Store as string for cross-engine compat (MySQL/SQLite use CHAR/TEXT).
            sea_query::Value::String(Some(Box::new(
                value.as_str().unwrap_or(&value.to_string()).to_string(),
            )))
        }
        FieldType::String | FieldType::Enum | FieldType::File => sea_query::Value::String(Some(
            Box::new(value.as_str().unwrap_or(&value.to_string()).to_string()),
        )),
        FieldType::Integer => sea_query::Value::Int(Some(value.as_i64().unwrap_or(0) as i32)),
        FieldType::Bigint => sea_query::Value::BigInt(Some(value.as_i64().unwrap_or(0))),
        FieldType::Number => sea_query::Value::Double(Some(value.as_f64().unwrap_or(0.0))),
        FieldType::Boolean => sea_query::Value::Bool(Some(value.as_bool().unwrap_or(false))),
        FieldType::Timestamp => sea_query::Value::String(Some(Box::new(
            value.as_str().unwrap_or(&value.to_string()).to_string(),
        ))),
        FieldType::Date => sea_query::Value::String(Some(Box::new(
            value.as_str().unwrap_or(&value.to_string()).to_string(),
        ))),
        FieldType::Json | FieldType::Array => sea_query::Value::Json(Some(Box::new(value.clone()))),
    }
}

fn coerce_filter_to_sea_value(value: &str, field: &FieldSchema) -> sea_query::Value {
    match field.field_type {
        FieldType::Uuid => {
            // Store as string for cross-engine compat.
            sea_query::Value::String(Some(Box::new(value.to_string())))
        }
        FieldType::Integer => value
            .parse::<i32>()
            .ok()
            .map(|n| sea_query::Value::Int(Some(n)))
            .unwrap_or_else(|| sea_query::Value::String(Some(Box::new(value.to_string())))),
        FieldType::Bigint => value
            .parse::<i64>()
            .ok()
            .map(|n| sea_query::Value::BigInt(Some(n)))
            .unwrap_or_else(|| sea_query::Value::String(Some(Box::new(value.to_string())))),
        FieldType::Number => value
            .parse::<f64>()
            .ok()
            .map(|n| sea_query::Value::Double(Some(n)))
            .unwrap_or_else(|| sea_query::Value::String(Some(Box::new(value.to_string())))),
        FieldType::Boolean => value
            .parse::<bool>()
            .ok()
            .map(|b| sea_query::Value::Bool(Some(b)))
            .unwrap_or_else(|| sea_query::Value::String(Some(Box::new(value.to_string())))),
        _ => sea_query::Value::String(Some(Box::new(value.to_string()))),
    }
}

/// ORM-backed resource query. Executes dialect-aware SQL via SeaORM.
/// Supports Postgres, MySQL, and SQLite backends.
pub struct OrmResourceQuery<'a> {
    pub resource: &'a ResourceDefinition,
    pub connection: &'a SqlConnection,
}

impl<'a> OrmResourceQuery<'a> {
    pub fn new(resource: &'a ResourceDefinition, connection: &'a SqlConnection) -> Self {
        Self {
            resource,
            connection,
        }
    }

    fn table(&self) -> String {
        self.connection.quote_ident(&self.resource.resource)
    }

    fn select_columns(&self) -> String {
        self.resource
            .schema
            .keys()
            .map(|c| self.connection.quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn primary_key(&self) -> &str {
        self.resource
            .schema
            .iter()
            .find(|(_, fs)| fs.primary)
            .map(|(name, _)| name.as_str())
            .unwrap_or("id")
    }

    fn has_soft_delete(&self) -> bool {
        self.resource
            .endpoints
            .as_ref()
            .map(|eps| eps.values().any(|ep| ep.soft_delete))
            .unwrap_or(false)
    }

    fn backend(&self) -> sea_orm::DatabaseBackend {
        self.connection.backend()
    }

    fn param(&self, index: usize) -> String {
        self.connection.param(index)
    }

    fn qi(&self, name: &str) -> String {
        self.connection.quote_ident(name)
    }

    /// Whether this engine supports RETURNING clause (Postgres, SQLite 3.35+).
    fn supports_returning(&self) -> bool {
        matches!(
            self.connection.engine,
            shaperail_core::DatabaseEngine::Postgres | shaperail_core::DatabaseEngine::SQLite
        )
    }

    /// Find a single record by primary key.
    pub async fn find_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let pk = self.primary_key();
        let soft = if self.has_soft_delete() {
            format!(" AND {} IS NULL", self.qi("deleted_at"))
        } else {
            String::new()
        };
        let p1 = self.param(1);
        let sql = format!(
            "SELECT {} FROM {} WHERE {} = {p1}{soft}",
            self.select_columns(),
            self.table(),
            self.qi(pk),
        );
        let values = sea_query::Values(vec![sea_query::Value::String(Some(Box::new(
            id.to_string(),
        )))]);
        let stmt = sea_orm::Statement::from_sql_and_values(self.backend(), sql, values);
        let span = telemetry::db_span("orm_find_by_id", &self.resource.resource, "SELECT");
        let _enter = span.enter();
        let rows = self
            .connection
            .inner
            .query_all(stmt)
            .await
            .map_err(|e| ShaperailError::Internal(format!("ORM find_by_id failed: {e}")))?;
        let row = rows.into_iter().next().ok_or(ShaperailError::NotFound)?;
        let json = query_result_to_json(&row, self.resource)?;
        Ok(ResourceRow(json))
    }

    /// Find all with filters, sort, and pagination.
    pub async fn find_all(
        &self,
        filters: &FilterSet,
        _search: Option<&SearchParam>,
        sort: &SortParam,
        page: &PageRequest,
    ) -> Result<(Vec<ResourceRow>, serde_json::Value), ShaperailError> {
        let mut sql = format!("SELECT {} FROM {}", self.select_columns(), self.table());
        let mut values_vec = Vec::new();
        let mut param = 1usize;

        if self.has_soft_delete() {
            sql.push_str(&format!(" WHERE {} IS NULL", self.qi("deleted_at")));
        }
        for f in &filters.filters {
            if let Some(field) = self.resource.schema.get(&f.field) {
                if param == 1 && !self.has_soft_delete() {
                    sql.push_str(" WHERE ");
                } else {
                    sql.push_str(" AND ");
                }
                let p = self.param(param);
                sql.push_str(&format!("{} = {p}", self.qi(&f.field)));
                values_vec.push(coerce_filter_to_sea_value(&f.value, field));
                param += 1;
            }
        }

        if !sort.fields.is_empty() {
            sql.push_str(" ORDER BY ");
            for (i, s) in sort.fields.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                let dir = match s.direction {
                    super::sort::SortDirection::Asc => "ASC",
                    super::sort::SortDirection::Desc => "DESC",
                };
                sql.push_str(&format!("{} {dir}", self.qi(&s.field)));
            }
        }

        match page {
            PageRequest::Cursor { after, limit } => {
                if let Some(cursor) = after {
                    let id_str = decode_cursor(cursor)?;
                    if param == 1 && !self.has_soft_delete() && filters.is_empty() {
                        sql.push_str(" WHERE ");
                    } else {
                        sql.push_str(" AND ");
                    }
                    let p = self.param(param);
                    sql.push_str(&format!("{} > {p}", self.qi("id")));
                    values_vec.push(sea_query::Value::String(Some(Box::new(id_str))));
                }
                if sort.fields.is_empty() {
                    sql.push_str(&format!(" ORDER BY {} ASC", self.qi("id")));
                }
                sql.push_str(&format!(" LIMIT {}", limit + 1));
                let values = sea_query::Values(values_vec);
                let stmt = sea_orm::Statement::from_sql_and_values(self.backend(), sql, values);
                let span = telemetry::db_span("orm_find_all", &self.resource.resource, "SELECT");
                let _enter = span.enter();
                let rows =
                    self.connection.inner.query_all(stmt).await.map_err(|e| {
                        ShaperailError::Internal(format!("ORM find_all failed: {e}"))
                    })?;
                let limit_i64 = *limit;
                let has_more = rows.len() as i64 > limit_i64;
                let result_rows: Vec<ResourceRow> = rows
                    .into_iter()
                    .take(limit_i64 as usize)
                    .map(|r| query_result_to_json(&r, self.resource).map(ResourceRow))
                    .collect::<Result<Vec<_>, _>>()?;
                let cursor = result_rows
                    .last()
                    .and_then(|r| r.0.get("id"))
                    .and_then(|v| v.as_str())
                    .map(encode_cursor);
                let meta = serde_json::json!({ "cursor": cursor, "has_more": has_more });
                Ok((result_rows, meta))
            }
            PageRequest::Offset { offset, limit } => {
                sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));
                let values = sea_query::Values(values_vec);
                let stmt = sea_orm::Statement::from_sql_and_values(self.backend(), sql, values);
                let span = telemetry::db_span("orm_find_all", &self.resource.resource, "SELECT");
                let _enter = span.enter();
                let rows =
                    self.connection.inner.query_all(stmt).await.map_err(|e| {
                        ShaperailError::Internal(format!("ORM find_all failed: {e}"))
                    })?;
                let result_rows: Vec<ResourceRow> = rows
                    .into_iter()
                    .map(|r| query_result_to_json(&r, self.resource).map(ResourceRow))
                    .collect::<Result<Vec<_>, _>>()?;
                let total = result_rows.len() as i64;
                let meta = serde_json::json!({
                    "offset": offset,
                    "limit": limit,
                    "total": total
                });
                Ok((result_rows, meta))
            }
        }
    }

    /// Insert a record. Uses RETURNING for Postgres/SQLite; for MySQL does INSERT + SELECT.
    pub async fn insert(
        &self,
        data: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<ResourceRow, ShaperailError> {
        let mut columns = Vec::new();
        let mut placeholders = Vec::new();
        let mut values_vec = Vec::new();
        let mut param = 1usize;
        let mut generated_id: Option<String> = None;

        for (name, field) in &self.resource.schema {
            if field.transient {
                continue;
            }
            if field.generated {
                match field.field_type {
                    FieldType::Uuid => {
                        let id = uuid::Uuid::new_v4();
                        columns.push(self.qi(name));
                        placeholders.push(self.param(param));
                        values_vec.push(sea_query::Value::String(Some(Box::new(id.to_string()))));
                        if field.primary {
                            generated_id = Some(id.to_string());
                        }
                        param += 1;
                    }
                    FieldType::Timestamp => {
                        let now = chrono::Utc::now().to_rfc3339();
                        columns.push(self.qi(name));
                        placeholders.push(self.param(param));
                        values_vec.push(sea_query::Value::String(Some(Box::new(now))));
                        param += 1;
                    }
                    _ => {}
                }
                continue;
            }
            if let Some(value) = data.get(name) {
                columns.push(self.qi(name));
                placeholders.push(self.param(param));
                values_vec.push(json_to_sea_value(value, field));
                param += 1;
            } else if let Some(default) = &field.default {
                columns.push(self.qi(name));
                placeholders.push(self.param(param));
                values_vec.push(json_to_sea_value(default, field));
                param += 1;
            }
        }

        if self.supports_returning() {
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({}) RETURNING {}",
                self.table(),
                columns.join(", "),
                placeholders.join(", "),
                self.select_columns(),
            );
            let values = sea_query::Values(values_vec);
            let stmt = sea_orm::Statement::from_sql_and_values(self.backend(), sql, values);
            let span = telemetry::db_span("orm_insert", &self.resource.resource, "INSERT");
            let _enter = span.enter();
            let rows = self
                .connection
                .inner
                .query_all(stmt)
                .await
                .map_err(|e| ShaperailError::Internal(format!("ORM insert failed: {e}")))?;
            let row = rows
                .into_iter()
                .next()
                .ok_or_else(|| ShaperailError::Internal("Insert returned no rows".to_string()))?;
            let json = query_result_to_json(&row, self.resource)?;
            Ok(ResourceRow(json))
        } else {
            // MySQL: INSERT then SELECT back.
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                self.table(),
                columns.join(", "),
                placeholders.join(", "),
            );
            let values = sea_query::Values(values_vec);
            let stmt = sea_orm::Statement::from_sql_and_values(self.backend(), sql, values);
            let span = telemetry::db_span("orm_insert", &self.resource.resource, "INSERT");
            let _enter = span.enter();
            self.connection
                .inner
                .execute(stmt)
                .await
                .map_err(|e| ShaperailError::Internal(format!("ORM insert failed: {e}")))?;
            // Fetch back via generated ID.
            if let Some(id_str) = generated_id {
                let id = uuid::Uuid::parse_str(&id_str).map_err(|e| {
                    ShaperailError::Internal(format!("Generated UUID parse error: {e}"))
                })?;
                self.find_by_id(&id).await
            } else {
                // Fallback: use LAST_INSERT_ID for MySQL auto-increment (rare in Shaperail).
                Err(ShaperailError::Internal(
                    "MySQL insert without generated UUID not supported".to_string(),
                ))
            }
        }
    }

    /// Update by primary key.
    pub async fn update_by_id(
        &self,
        id: &uuid::Uuid,
        data: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<ResourceRow, ShaperailError> {
        let mut set_parts = Vec::new();
        let mut values_vec = Vec::new();
        let mut param = 1usize;

        for (name, value) in data {
            if let Some(field) = self.resource.schema.get(name) {
                if field.primary || field.generated {
                    continue;
                }
                let p = self.param(param);
                set_parts.push(format!("{} = {p}", self.qi(name)));
                values_vec.push(json_to_sea_value(value, field));
                param += 1;
            }
        }
        if self.resource.schema.contains_key("updated_at") {
            let p = self.param(param);
            let now = chrono::Utc::now().to_rfc3339();
            set_parts.push(format!("{} = {p}", self.qi("updated_at")));
            values_vec.push(sea_query::Value::String(Some(Box::new(now))));
            param += 1;
        }
        if set_parts.is_empty() {
            return Err(ShaperailError::Validation(vec![
                shaperail_core::FieldError {
                    field: "body".to_string(),
                    message: "No valid fields to update".to_string(),
                    code: "empty_update".to_string(),
                },
            ]));
        }
        let pk = self.primary_key();
        let soft = if self.has_soft_delete() {
            format!(" AND {} IS NULL", self.qi("deleted_at"))
        } else {
            String::new()
        };
        let p = self.param(param);
        values_vec.push(sea_query::Value::String(Some(Box::new(id.to_string()))));

        if self.supports_returning() {
            let sql = format!(
                "UPDATE {} SET {} WHERE {} = {p}{soft} RETURNING {}",
                self.table(),
                set_parts.join(", "),
                self.qi(pk),
                self.select_columns(),
            );
            let values = sea_query::Values(values_vec);
            let stmt = sea_orm::Statement::from_sql_and_values(self.backend(), sql, values);
            let span = telemetry::db_span("orm_update", &self.resource.resource, "UPDATE");
            let _enter = span.enter();
            let rows = self
                .connection
                .inner
                .query_all(stmt)
                .await
                .map_err(|e| ShaperailError::Internal(format!("ORM update failed: {e}")))?;
            rows.into_iter()
                .next()
                .ok_or(ShaperailError::NotFound)
                .and_then(|row| query_result_to_json(&row, self.resource).map(ResourceRow))
        } else {
            // MySQL: UPDATE then SELECT back.
            let sql = format!(
                "UPDATE {} SET {} WHERE {} = {p}{soft}",
                self.table(),
                set_parts.join(", "),
                self.qi(pk),
            );
            let values = sea_query::Values(values_vec);
            let stmt = sea_orm::Statement::from_sql_and_values(self.backend(), sql, values);
            let span = telemetry::db_span("orm_update", &self.resource.resource, "UPDATE");
            let _enter = span.enter();
            let result = self
                .connection
                .inner
                .execute(stmt)
                .await
                .map_err(|e| ShaperailError::Internal(format!("ORM update failed: {e}")))?;
            if result.rows_affected() == 0 {
                return Err(ShaperailError::NotFound);
            }
            self.find_by_id(id).await
        }
    }

    /// Soft-delete by setting deleted_at.
    pub async fn soft_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let pk = self.primary_key();
        let p1 = self.param(1);
        let p2 = self.param(2);
        let now = chrono::Utc::now().to_rfc3339();

        if self.supports_returning() {
            let sql = format!(
                "UPDATE {} SET {} = {p1} WHERE {} = {p2} AND {} IS NULL RETURNING {}",
                self.table(),
                self.qi("deleted_at"),
                self.qi(pk),
                self.qi("deleted_at"),
                self.select_columns(),
            );
            let values = sea_query::Values(vec![
                sea_query::Value::String(Some(Box::new(now))),
                sea_query::Value::String(Some(Box::new(id.to_string()))),
            ]);
            let stmt = sea_orm::Statement::from_sql_and_values(self.backend(), sql, values);
            let span = telemetry::db_span("orm_soft_delete", &self.resource.resource, "UPDATE");
            let _enter = span.enter();
            let rows =
                self.connection.inner.query_all(stmt).await.map_err(|e| {
                    ShaperailError::Internal(format!("ORM soft_delete failed: {e}"))
                })?;
            rows.into_iter()
                .next()
                .ok_or(ShaperailError::NotFound)
                .and_then(|row| query_result_to_json(&row, self.resource).map(ResourceRow))
        } else {
            // MySQL: UPDATE then SELECT.
            let sql = format!(
                "UPDATE {} SET {} = {p1} WHERE {} = {p2} AND {} IS NULL",
                self.table(),
                self.qi("deleted_at"),
                self.qi(pk),
                self.qi("deleted_at"),
            );
            let values = sea_query::Values(vec![
                sea_query::Value::String(Some(Box::new(now))),
                sea_query::Value::String(Some(Box::new(id.to_string()))),
            ]);
            let stmt = sea_orm::Statement::from_sql_and_values(self.backend(), sql, values);
            let span = telemetry::db_span("orm_soft_delete", &self.resource.resource, "UPDATE");
            let _enter = span.enter();
            let result =
                self.connection.inner.execute(stmt).await.map_err(|e| {
                    ShaperailError::Internal(format!("ORM soft_delete failed: {e}"))
                })?;
            if result.rows_affected() == 0 {
                return Err(ShaperailError::NotFound);
            }
            self.find_by_id(id).await
        }
    }

    /// Hard-delete by primary key.
    pub async fn hard_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let row = self.find_by_id(id).await?;
        let pk = self.primary_key();
        let p1 = self.param(1);

        if self.supports_returning() {
            let sql = format!(
                "DELETE FROM {} WHERE {} = {p1} RETURNING {}",
                self.table(),
                self.qi(pk),
                self.select_columns(),
            );
            let values = sea_query::Values(vec![sea_query::Value::String(Some(Box::new(
                id.to_string(),
            )))]);
            let stmt = sea_orm::Statement::from_sql_and_values(self.backend(), sql, values);
            let span = telemetry::db_span("orm_hard_delete", &self.resource.resource, "DELETE");
            let _enter = span.enter();
            self.connection
                .inner
                .execute(stmt)
                .await
                .map_err(|e| ShaperailError::Internal(format!("ORM hard_delete failed: {e}")))?;
        } else {
            // MySQL: just DELETE.
            let sql = format!("DELETE FROM {} WHERE {} = {p1}", self.table(), self.qi(pk),);
            let values = sea_query::Values(vec![sea_query::Value::String(Some(Box::new(
                id.to_string(),
            )))]);
            let stmt = sea_orm::Statement::from_sql_and_values(self.backend(), sql, values);
            let span = telemetry::db_span("orm_hard_delete", &self.resource.resource, "DELETE");
            let _enter = span.enter();
            self.connection
                .inner
                .execute(stmt)
                .await
                .map_err(|e| ShaperailError::Internal(format!("ORM hard_delete failed: {e}")))?;
        }
        Ok(row)
    }
}
