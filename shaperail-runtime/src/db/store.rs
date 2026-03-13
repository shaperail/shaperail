use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};

use super::{FilterSet, PageRequest, SearchParam, SortParam};
use crate::db::ResourceRow;
use shaperail_core::{EndpointSpec, ShaperailError};

pub type StoreRegistry = Arc<HashMap<String, Arc<dyn ResourceStore>>>;

/// Typed resource store implemented by generated per-resource query modules.
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
