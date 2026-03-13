use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};

use super::{DatabaseManager, ResourceRow};
use super::{FilterSet, OrmResourceQuery, PageRequest, SearchParam, SortParam, SqlConnection};
use shaperail_core::{EndpointSpec, ResourceDefinition, ShaperailError};

/// Typed resource store implemented by generated per-resource query modules or OrmBackedStore (M14).
#[async_trait]
pub trait ResourceStore: Send + Sync {
    fn resource_name(&self) -> &str;

    async fn find_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError>;

    async fn find_all(
        &self,
        endpoint: &EndpointSpec,
        filters: &FilterSet,
        search: Option<&SearchParam>,
        sort: &SortParam,
        page: &PageRequest,
    ) -> Result<(Vec<ResourceRow>, Value), ShaperailError>;

    async fn insert(&self, data: &Map<String, Value>) -> Result<ResourceRow, ShaperailError>;

    async fn update_by_id(
        &self,
        id: &uuid::Uuid,
        data: &Map<String, Value>,
    ) -> Result<ResourceRow, ShaperailError>;

    async fn soft_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError>;

    async fn hard_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError>;
}

pub type StoreRegistry = Arc<HashMap<String, Arc<dyn ResourceStore>>>;

/// ORM-backed store (M14). Delegates to OrmResourceQuery using a per-resource connection.
pub struct OrmBackedStore {
    resource: Arc<ResourceDefinition>,
    connection: SqlConnection,
}

impl OrmBackedStore {
    pub fn new(resource: Arc<ResourceDefinition>, connection: SqlConnection) -> Self {
        Self {
            resource,
            connection,
        }
    }

    fn orm(&self) -> OrmResourceQuery<'_> {
        OrmResourceQuery::new(self.resource.as_ref(), &self.connection)
    }
}

#[async_trait]
impl ResourceStore for OrmBackedStore {
    fn resource_name(&self) -> &str {
        &self.resource.resource
    }

    async fn find_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        self.orm().find_by_id(id).await
    }

    async fn find_all(
        &self,
        _endpoint: &EndpointSpec,
        filters: &FilterSet,
        search: Option<&SearchParam>,
        sort: &SortParam,
        page: &PageRequest,
    ) -> Result<(Vec<ResourceRow>, Value), ShaperailError> {
        self.orm().find_all(filters, search, sort, page).await
    }

    async fn insert(&self, data: &Map<String, Value>) -> Result<ResourceRow, ShaperailError> {
        self.orm().insert(data).await
    }

    async fn update_by_id(
        &self,
        id: &uuid::Uuid,
        data: &Map<String, Value>,
    ) -> Result<ResourceRow, ShaperailError> {
        self.orm().update_by_id(id, data).await
    }

    async fn soft_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        self.orm().soft_delete_by_id(id).await
    }

    async fn hard_delete_by_id(&self, id: &uuid::Uuid) -> Result<ResourceRow, ShaperailError> {
        self.orm().hard_delete_by_id(id).await
    }
}

/// Builds a store registry from DatabaseManager and resources (M14 ORM path).
/// Each resource gets an OrmBackedStore using the connection for its `db` (or default).
pub fn build_orm_store_registry(
    manager: &DatabaseManager,
    resources: &[ResourceDefinition],
) -> Result<StoreRegistry, ShaperailError> {
    let mut stores: HashMap<String, Arc<dyn ResourceStore>> = HashMap::new();
    for resource in resources {
        let conn = manager
            .sql_for_resource(resource.db.as_ref())
            .ok_or_else(|| {
                ShaperailError::Internal(format!(
                    "No SQL connection for resource '{}' (db: {:?})",
                    resource.resource, resource.db
                ))
            })?;
        let store = OrmBackedStore::new(Arc::new(resource.clone()), conn);
        stores.insert(resource.resource.clone(), Arc::new(store));
    }
    Ok(Arc::new(stores))
}
