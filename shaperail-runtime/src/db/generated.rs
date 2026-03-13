use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};

use super::{FilterSet, ResourceRow, SortDirection, SortParam};
use shaperail_core::{FieldError, ShaperailError};

pub fn parse_optional_json<T: DeserializeOwned>(
    data: &Map<String, Value>,
    field: &str,
) -> Result<Option<T>, ShaperailError> {
    let Some(value) = data.get(field) else {
        return Ok(None);
    };

    if value.is_null() {
        return Ok(None);
    }

    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|_| {
            ShaperailError::Validation(vec![FieldError {
                field: field.to_string(),
                message: format!("{field} has an invalid value"),
                code: "invalid_value".to_string(),
            }])
        })
}

pub fn parse_embedded_json<T: DeserializeOwned>(
    field: &str,
    value: Value,
) -> Result<T, ShaperailError> {
    serde_json::from_value(value).map_err(|_| {
        ShaperailError::Internal(format!(
            "Invalid generated default value for field '{field}'"
        ))
    })
}

pub fn require_field<T>(value: Option<T>, field: &str) -> Result<T, ShaperailError> {
    value.ok_or_else(|| {
        ShaperailError::Validation(vec![FieldError {
            field: field.to_string(),
            message: format!("{field} is required"),
            code: "required".to_string(),
        }])
    })
}

pub fn parse_filter_text(filters: &FilterSet, field: &str) -> Option<String> {
    filters
        .filters
        .iter()
        .find(|filter| filter.field == field)
        .map(|filter| filter.value.clone())
}

pub fn parse_filter<T>(
    filters: &FilterSet,
    field: &str,
    code: &str,
    parser: impl Fn(&str) -> Result<T, ShaperailError>,
) -> Result<Option<T>, ShaperailError> {
    let Some(raw) = parse_filter_text(filters, field) else {
        return Ok(None);
    };

    parser(&raw).map(Some).map_err(|_| {
        ShaperailError::Validation(vec![FieldError {
            field: field.to_string(),
            message: format!("{field} has an invalid filter value"),
            code: code.to_string(),
        }])
    })
}

pub fn sort_field_at(sort: &SortParam, index: usize) -> Option<String> {
    sort.fields.get(index).map(|field| field.field.clone())
}

pub fn sort_direction_at(sort: &SortParam, index: usize) -> Option<&'static str> {
    sort.fields.get(index).map(|field| match field.direction {
        SortDirection::Asc => "asc",
        SortDirection::Desc => "desc",
    })
}

pub fn row_from_model<T: Serialize>(model: &T) -> Result<ResourceRow, ShaperailError> {
    serde_json::to_value(model)
        .map(ResourceRow)
        .map_err(|e| ShaperailError::Internal(format!("Failed to serialize row: {e}")))
}
