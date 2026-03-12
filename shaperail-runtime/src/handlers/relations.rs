use std::sync::Arc;

use actix_web::web;
use shaperail_core::{RelationType, ResourceDefinition, ShaperailError};

use crate::db::ResourceQuery;

use super::crud::AppState;

/// Loads related resources into each data item based on `?include=` parameter.
///
/// For `belongs_to` relations, performs a join-like lookup by the foreign key.
/// For `has_many` / `has_one`, looks up by foreign key on the related table.
pub async fn load_relations(
    items: &mut [serde_json::Value],
    resource: &ResourceDefinition,
    include: &[String],
    state: &web::Data<Arc<AppState>>,
) -> Result<(), ShaperailError> {
    let relations = match &resource.relations {
        Some(r) => r,
        None => return Ok(()),
    };

    for relation_name in include {
        let relation = match relations.get(relation_name) {
            Some(r) => r,
            None => continue,
        };

        // Find the related resource definition
        let related_resource = state
            .resources
            .iter()
            .find(|r| r.resource == relation.resource);

        let Some(related_resource) = related_resource else {
            continue;
        };

        let rq = ResourceQuery::new(related_resource, &state.pool);

        match relation.relation_type {
            RelationType::BelongsTo => {
                let default_key = format!("{}_id", relation_name);
                let key = relation.key.as_deref().unwrap_or(&default_key);

                for item in items.iter_mut() {
                    if let Some(fk_value) = item.get(key).and_then(|v| v.as_str()) {
                        if let Ok(fk_uuid) = uuid::Uuid::parse_str(fk_value) {
                            match rq.find_by_id(&fk_uuid).await {
                                Ok(row) => {
                                    if let Some(obj) = item.as_object_mut() {
                                        obj.insert(relation_name.clone(), row.0);
                                    }
                                }
                                Err(ShaperailError::NotFound) => {
                                    if let Some(obj) = item.as_object_mut() {
                                        obj.insert(relation_name.clone(), serde_json::Value::Null);
                                    }
                                }
                                Err(e) => return Err(e),
                            }
                        }
                    }
                }
            }
            RelationType::HasMany | RelationType::HasOne => {
                let foreign_key = relation.foreign_key.as_deref().unwrap_or("id");

                // Find the primary key field name on the current resource
                let pk = resource
                    .schema
                    .iter()
                    .find(|(_, fs)| fs.primary)
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("id");

                for item in items.iter_mut() {
                    if let Some(pk_value) = item.get(pk).and_then(|v| v.as_str()) {
                        // Query the related table where foreign_key = pk_value
                        let filter_set = crate::db::FilterSet {
                            filters: vec![crate::db::FilterParam {
                                field: foreign_key.to_string(),
                                value: pk_value.to_string(),
                            }],
                        };
                        let search = None;
                        let sort = crate::db::SortParam::default();
                        let page = crate::db::PageRequest::Cursor {
                            after: None,
                            limit: 100,
                        };

                        let (rows, _meta) = rq.find_all(&filter_set, search, &sort, &page).await?;

                        let values: Vec<serde_json::Value> =
                            rows.into_iter().map(|r| r.0).collect();

                        if let Some(obj) = item.as_object_mut() {
                            if relation.relation_type == RelationType::HasOne {
                                obj.insert(
                                    relation_name.clone(),
                                    values.into_iter().next().unwrap_or(serde_json::Value::Null),
                                );
                            } else {
                                obj.insert(relation_name.clone(), serde_json::Value::Array(values));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
