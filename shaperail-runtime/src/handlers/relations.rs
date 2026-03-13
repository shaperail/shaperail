use std::sync::Arc;

use actix_web::web;
use shaperail_core::{EndpointSpec, HttpMethod, RelationType, ResourceDefinition, ShaperailError};

use crate::db::{FilterParam, FilterSet, PageRequest, ResourceQuery, SortParam};

use super::crud::AppState;

/// Returns the generated store for a resource when the app has a store registry and the resource is registered.
fn store_for(
    state: &AppState,
    resource: &ResourceDefinition,
) -> Option<Arc<dyn crate::db::ResourceStore>> {
    state
        .stores
        .as_ref()
        .and_then(|stores| stores.get(&resource.resource).cloned())
}

/// Returns a list endpoint for the resource (GET without :id) for use with store.find_all. Returns None if none declared.
fn list_endpoint_for(resource: &ResourceDefinition) -> Option<&EndpointSpec> {
    resource.endpoints.as_ref().and_then(|eps| {
        eps.iter()
            .find(|(_, ep)| ep.method == HttpMethod::Get && !ep.path.contains(":id"))
            .map(|(_, ep)| ep)
    })
}

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

        let store = store_for(state, related_resource);
        let rq = ResourceQuery::new(related_resource, &state.pool);

        match relation.relation_type {
            RelationType::BelongsTo => {
                let default_key = format!("{}_id", relation_name);
                let key = relation.key.as_deref().unwrap_or(&default_key);

                for item in items.iter_mut() {
                    if let Some(fk_value) = item.get(key).and_then(|v| v.as_str()) {
                        if let Ok(fk_uuid) = uuid::Uuid::parse_str(fk_value) {
                            let result = if let Some(ref s) = store {
                                s.find_by_id(&fk_uuid).await
                            } else {
                                rq.find_by_id(&fk_uuid).await
                            };
                            match result {
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

                let pk = resource
                    .schema
                    .iter()
                    .find(|(_, fs)| fs.primary)
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("id");

                let filter_set = FilterSet {
                    filters: vec![FilterParam {
                        field: foreign_key.to_string(),
                        value: String::new(), // filled per item
                    }],
                };
                let sort = SortParam::default();
                let page = PageRequest::Cursor {
                    after: None,
                    limit: 100,
                };

                let list_ep = list_endpoint_for(related_resource);

                for item in items.iter_mut() {
                    if let Some(pk_value) = item.get(pk).and_then(|v| v.as_str()) {
                        let mut filters = filter_set.clone();
                        filters.filters[0].value = pk_value.to_string();

                        let (rows, _meta) = if let (Some(ref s), Some(ep)) = (&store, list_ep) {
                            s.find_all(ep, &filters, None, &sort, &page).await?
                        } else {
                            rq.find_all(&filters, None, &sort, &page).await?
                        };

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
