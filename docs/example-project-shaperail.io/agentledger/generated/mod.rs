pub mod organizations;

pub fn build_store_registry(pool: sqlx::PgPool) -> shaperail_runtime::db::StoreRegistry {
    let mut stores: std::collections::HashMap<
        String,
        std::sync::Arc<dyn shaperail_runtime::db::ResourceStore>,
    > = std::collections::HashMap::new();
    stores.insert("organizations".to_string(), std::sync::Arc::new(organizations::OrganizationsStore::new(pool.clone())));
    std::sync::Arc::new(stores)
}
