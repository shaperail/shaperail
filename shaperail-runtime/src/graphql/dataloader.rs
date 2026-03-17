//! DataLoader for GraphQL relations (M15). Batches and caches relation lookups
//! to prevent N+1 queries when resolving nested relations.
//!
//! Each GraphQL request gets a `RelationLoader` that groups lookups for the same
//! (resource, foreign_key, value) triple and resolves them in a single batch query.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use shaperail_core::{
    EndpointSpec, HttpMethod, PaginationStyle, ResourceDefinition, ShaperailError,
};

use crate::db::{FilterParam, FilterSet, PageRequest, ResourceQuery, ResourceRow, SortParam};
use crate::handlers::crud::{store_for_or_error, AppState};

/// Cache key: (resource_name, filter_field, filter_value).
type CacheKey = (String, String, String);

/// Batched relation loader. Caches results per request to prevent N+1 queries.
///
/// Thread-safe via `Mutex` — shared across all resolvers in a single request.
#[derive(Clone)]
pub struct RelationLoader {
    state: Arc<AppState>,
    resources: Vec<ResourceDefinition>,
    /// Cache of already-loaded rows: key → rows.
    cache: Arc<Mutex<HashMap<CacheKey, Vec<ResourceRow>>>>,
}

impl RelationLoader {
    pub fn new(state: Arc<AppState>, resources: Vec<ResourceDefinition>) -> Self {
        Self {
            state,
            resources,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Load a single record by ID (belongs_to). Uses cache to avoid duplicate queries.
    pub async fn load_by_id(
        &self,
        resource_name: &str,
        id: &uuid::Uuid,
    ) -> Result<Option<ResourceRow>, ShaperailError> {
        let key: CacheKey = (resource_name.to_string(), "id".to_string(), id.to_string());

        // Check cache first.
        {
            let cache = self.cache.lock().await;
            if let Some(rows) = cache.get(&key) {
                return Ok(rows.first().cloned());
            }
        }

        // Cache miss: load from DB.
        let resource = self
            .resources
            .iter()
            .find(|r| r.resource == resource_name)
            .ok_or_else(|| {
                ShaperailError::Internal(format!("Resource '{resource_name}' not found"))
            })?;

        let store_opt = store_for_or_error(&self.state, resource)?;
        let row = if let Some(store) = store_opt {
            store.find_by_id(id).await?
        } else {
            let rq = ResourceQuery::new(resource, &self.state.pool);
            rq.find_by_id(id).await?
        };

        // Cache result.
        {
            let mut cache = self.cache.lock().await;
            cache.insert(key, vec![row.clone()]);
        }

        Ok(Some(row))
    }

    /// Load related records by a filter field (has_many/has_one).
    /// Results are cached per (resource, field, value) triple.
    pub async fn load_by_filter(
        &self,
        resource_name: &str,
        filter_field: &str,
        filter_value: &str,
    ) -> Result<Vec<ResourceRow>, ShaperailError> {
        let key: CacheKey = (
            resource_name.to_string(),
            filter_field.to_string(),
            filter_value.to_string(),
        );

        // Check cache first.
        {
            let cache = self.cache.lock().await;
            if let Some(rows) = cache.get(&key) {
                return Ok(rows.clone());
            }
        }

        // Cache miss: load from DB.
        let resource = self
            .resources
            .iter()
            .find(|r| r.resource == resource_name)
            .ok_or_else(|| {
                ShaperailError::Internal(format!("Resource '{resource_name}' not found"))
            })?;

        let endpoint = resource
            .endpoints
            .as_ref()
            .and_then(|e| e.get("list"))
            .cloned()
            .unwrap_or_else(|| EndpointSpec {
                method: Some(HttpMethod::Get),
                path: Some(format!("/{}", resource.resource)),
                auth: None,
                input: None,
                filters: None,
                search: None,
                pagination: Some(PaginationStyle::Offset),
                sort: None,
                cache: None,
                controller: None,
                events: None,
                jobs: None,
                upload: None,
                soft_delete: false,
            });

        let filters = FilterSet {
            filters: vec![FilterParam {
                field: filter_field.to_string(),
                value: filter_value.to_string(),
            }],
        };
        let sort = SortParam::default();
        let page = PageRequest::Offset {
            offset: 0,
            limit: 1000,
        };

        let store_opt = store_for_or_error(&self.state, resource)?;
        let (rows, _) = if let Some(store) = store_opt {
            store
                .find_all(&endpoint, &filters, None, &sort, &page)
                .await?
        } else {
            let rq = ResourceQuery::new(resource, &self.state.pool);
            rq.find_all(&filters, None, &sort, &page).await?
        };

        // Cache result.
        {
            let mut cache = self.cache.lock().await;
            cache.insert(key, rows.clone());
        }

        Ok(rows)
    }

    /// Returns the number of cached entries (for testing N+1 prevention).
    pub async fn cache_size(&self) -> usize {
        self.cache.lock().await.len()
    }
}
