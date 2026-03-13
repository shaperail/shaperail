//! ORM-backed resource query (M14). Builds SQL and executes via SeaORM (Postgres).
//!
//! Uses the same SQL shape as ResourceQuery but runs through SeaORM's connection
//! for pooling and async. Full dialect support (MySQL/SQLite) can use SeaQuery later.

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
            let v: Option<uuid::Uuid> = row.try_get("", name).map_err(map_err)?;
            Ok(v.map(|u| serde_json::Value::String(u.to_string()))
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
            let v: Option<chrono::DateTime<chrono::Utc>> =
                row.try_get("", name).map_err(map_err)?;
            Ok(v.map(|dt| serde_json::Value::String(dt.to_rfc3339()))
                .unwrap_or(serde_json::Value::Null))
        }
        FieldType::Date => {
            let v: Option<chrono::NaiveDate> = row.try_get("", name).map_err(map_err)?;
            Ok(v.map(|d| serde_json::Value::String(d.to_string()))
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
        FieldType::Uuid => value
            .as_str()
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .map(|u| sea_query::Value::Uuid(Some(Box::new(u))))
            .unwrap_or(sea_query::Value::String(Some(Box::new(value.to_string())))),
        FieldType::String | FieldType::Enum | FieldType::File => sea_query::Value::String(Some(
            Box::new(value.as_str().unwrap_or(&value.to_string()).to_string()),
        )),
        FieldType::Integer => sea_query::Value::Int(Some(value.as_i64().unwrap_or(0) as i32)),
        FieldType::Bigint => sea_query::Value::BigInt(Some(value.as_i64().unwrap_or(0))),
        FieldType::Number => sea_query::Value::Double(Some(value.as_f64().unwrap_or(0.0))),
        FieldType::Boolean => sea_query::Value::Bool(Some(value.as_bool().unwrap_or(false))),
        FieldType::Timestamp => sea_query::Value::ChronoDateTime(Some(Box::new(
            value
                .as_str()
                .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
                .map(|dt| dt.naive_utc())
                .unwrap_or_else(|| chrono::Utc::now().naive_utc()),
        ))),
        FieldType::Date => sea_query::Value::ChronoDate(Some(Box::new(
            value
                .as_str()
                .and_then(|s| s.parse::<chrono::NaiveDate>().ok())
                .unwrap_or_else(|| chrono::Utc::now().date_naive()),
        ))),
        FieldType::Json | FieldType::Array => sea_query::Value::Json(Some(Box::new(value.clone()))),
    }
}

fn coerce_filter_to_sea_value(value: &str, field: &FieldSchema) -> sea_query::Value {
    match field.field_type {
        FieldType::Uuid => uuid::Uuid::parse_str(value)
            .map(|u| sea_query::Value::Uuid(Some(Box::new(u))))
            .unwrap_or_else(|_| sea_query::Value::String(Some(Box::new(value.to_string())))),
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

/// ORM-backed resource query. Executes SQL via SeaORM (Postgres).
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

    fn table(&self) -> &str {
        &self.resource.resource
    }

    fn select_columns(&self) -> String {
        self.resource
            .schema
            .keys()
            .map(|c| format!("\"{c}\""))
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

    /// Find a single record by primary key.
    pub async fn find_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let pk = self.primary_key();
        let soft = if self.has_soft_delete() {
            " AND \"deleted_at\" IS NULL"
        } else {
            ""
        };
        let sql = format!(
            "SELECT {} FROM \"{}\" WHERE \"{}\" = $1{soft}",
            self.select_columns(),
            self.table(),
            pk,
        );
        let values = sea_query::Values(vec![sea_query::Value::Uuid(Some(Box::new(*id)))]);
        let stmt = sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            sql,
            values,
        );
        let span = telemetry::db_span("orm_find_by_id", self.table(), "SELECT");
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

    /// Find all with filters, sort, and pagination (no full-text search in ORM path).
    pub async fn find_all(
        &self,
        filters: &FilterSet,
        _search: Option<&SearchParam>,
        sort: &SortParam,
        page: &PageRequest,
    ) -> Result<(Vec<ResourceRow>, serde_json::Value), ShaperailError> {
        let mut sql = format!("SELECT {} FROM \"{}\"", self.select_columns(), self.table());
        let mut values_vec = Vec::new();
        let mut param = 1usize;

        if self.has_soft_delete() {
            sql.push_str(" WHERE \"deleted_at\" IS NULL");
        }
        for f in &filters.filters {
            if let Some(field) = self.resource.schema.get(&f.field) {
                if param == 1 && !self.has_soft_delete() {
                    sql.push_str(" WHERE ");
                } else {
                    sql.push_str(" AND ");
                }
                sql.push_str(&format!("\"{}\" = ${param}", f.field));
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
                sql.push_str(&format!("\"{}\" {dir}", s.field));
            }
        }

        match page {
            PageRequest::Cursor { after, limit } => {
                if let Some(cursor) = after {
                    let id_str = decode_cursor(cursor)?;
                    let id = uuid::Uuid::parse_str(&id_str).map_err(|_| {
                        ShaperailError::Validation(vec![shaperail_core::FieldError {
                            field: "cursor".to_string(),
                            message: "Invalid cursor value".to_string(),
                            code: "invalid_cursor".to_string(),
                        }])
                    })?;
                    if param == 1 && !self.has_soft_delete() && filters.is_empty() {
                        sql.push_str(" WHERE ");
                    } else {
                        sql.push_str(" AND ");
                    }
                    sql.push_str(&format!("\"id\" > ${param}"));
                    values_vec.push(sea_query::Value::Uuid(Some(Box::new(id))));
                }
                if sort.fields.is_empty() {
                    sql.push_str(" ORDER BY \"id\" ASC");
                }
                sql.push_str(&format!(" LIMIT {}", limit + 1));
                let values = sea_query::Values(values_vec);
                let stmt = sea_orm::Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Postgres,
                    sql,
                    values,
                );
                let span = telemetry::db_span("orm_find_all", self.table(), "SELECT");
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
                let stmt = sea_orm::Statement::from_sql_and_values(
                    sea_orm::DatabaseBackend::Postgres,
                    sql,
                    values,
                );
                let span = telemetry::db_span("orm_find_all", self.table(), "SELECT");
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

    /// Insert a record. Returns the inserted row (RETURNING).
    pub async fn insert(
        &self,
        data: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<ResourceRow, ShaperailError> {
        let mut columns = Vec::new();
        let mut placeholders = Vec::new();
        let mut values_vec = Vec::new();
        let mut param = 1usize;

        for (name, field) in &self.resource.schema {
            if field.generated {
                match field.field_type {
                    FieldType::Uuid => {
                        columns.push(format!("\"{name}\""));
                        placeholders.push(format!("${param}"));
                        values_vec
                            .push(sea_query::Value::Uuid(Some(Box::new(uuid::Uuid::new_v4()))));
                        param += 1;
                    }
                    FieldType::Timestamp => {
                        columns.push(format!("\"{name}\""));
                        placeholders.push(format!("${param}"));
                        values_vec.push(sea_query::Value::ChronoDateTime(Some(Box::new(
                            chrono::Utc::now().naive_utc(),
                        ))));
                        param += 1;
                    }
                    _ => {}
                }
                continue;
            }
            if let Some(value) = data.get(name) {
                columns.push(format!("\"{name}\""));
                placeholders.push(format!("${param}"));
                values_vec.push(json_to_sea_value(value, field));
                param += 1;
            } else if let Some(default) = &field.default {
                columns.push(format!("\"{name}\""));
                placeholders.push(format!("${param}"));
                values_vec.push(json_to_sea_value(default, field));
                param += 1;
            }
        }

        let sql = format!(
            "INSERT INTO \"{}\" ({}) VALUES ({}) RETURNING {}",
            self.table(),
            columns.join(", "),
            placeholders.join(", "),
            self.select_columns(),
        );
        let values = sea_query::Values(values_vec);
        let stmt = sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            sql,
            values,
        );
        let span = telemetry::db_span("orm_insert", self.table(), "INSERT");
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
                set_parts.push(format!("\"{name}\" = ${param}"));
                values_vec.push(json_to_sea_value(value, field));
                param += 1;
            }
        }
        if self.resource.schema.contains_key("updated_at") {
            set_parts.push(format!("\"updated_at\" = ${param}"));
            values_vec.push(sea_query::Value::ChronoDateTime(Some(Box::new(
                chrono::Utc::now().naive_utc(),
            ))));
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
            " AND \"deleted_at\" IS NULL"
        } else {
            ""
        };
        values_vec.push(sea_query::Value::Uuid(Some(Box::new(*id))));
        let sql = format!(
            "UPDATE \"{}\" SET {} WHERE \"{}\" = ${param}{soft} RETURNING {}",
            self.table(),
            set_parts.join(", "),
            pk,
            self.select_columns(),
        );
        let values = sea_query::Values(values_vec);
        let stmt = sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            sql,
            values,
        );
        let span = telemetry::db_span("orm_update", self.table(), "UPDATE");
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
    }

    /// Soft-delete by setting deleted_at.
    pub async fn soft_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let pk = self.primary_key();
        let sql = format!(
            "UPDATE \"{}\" SET \"deleted_at\" = $1 WHERE \"{}\" = $2 AND \"deleted_at\" IS NULL RETURNING {}",
            self.table(),
            pk,
            self.select_columns(),
        );
        let values = sea_query::Values(vec![
            sea_query::Value::ChronoDateTime(Some(Box::new(chrono::Utc::now().naive_utc()))),
            sea_query::Value::Uuid(Some(Box::new(*id))),
        ]);
        let stmt = sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            sql,
            values,
        );
        let span = telemetry::db_span("orm_soft_delete", self.table(), "UPDATE");
        let _enter = span.enter();
        let rows = self
            .connection
            .inner
            .query_all(stmt)
            .await
            .map_err(|e| ShaperailError::Internal(format!("ORM soft_delete failed: {e}")))?;
        rows.into_iter()
            .next()
            .ok_or(ShaperailError::NotFound)
            .and_then(|row| query_result_to_json(&row, self.resource).map(ResourceRow))
    }

    /// Hard-delete by primary key.
    pub async fn hard_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        let row = self.find_by_id(id).await?;
        let pk = self.primary_key();
        let sql = format!(
            "DELETE FROM \"{}\" WHERE \"{}\" = $1 RETURNING {}",
            self.table(),
            pk,
            self.select_columns(),
        );
        let values = sea_query::Values(vec![sea_query::Value::Uuid(Some(Box::new(*id)))]);
        let stmt = sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            sql,
            values,
        );
        let span = telemetry::db_span("orm_hard_delete", self.table(), "DELETE");
        let _enter = span.enter();
        self.connection
            .inner
            .execute(stmt)
            .await
            .map_err(|e| ShaperailError::Internal(format!("ORM hard_delete failed: {e}")))?;
        Ok(row)
    }
}
