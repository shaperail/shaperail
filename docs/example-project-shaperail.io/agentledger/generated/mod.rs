#![allow(dead_code)]

pub mod organizations;
pub mod users;

#[path = "../resources/users.controller.rs"]
mod users_controller;

pub fn build_store_registry(pool: sqlx::PgPool) -> shaperail_runtime::db::StoreRegistry {
    let mut stores: std::collections::HashMap<
        String,
        std::sync::Arc<dyn shaperail_runtime::db::ResourceStore>,
    > = std::collections::HashMap::new();
    stores.insert("organizations".to_string(), std::sync::Arc::new(organizations::OrganizationsStore::new(pool.clone())));
    stores.insert("users".to_string(), std::sync::Arc::new(users::UsersStore::new(pool.clone())));
    std::sync::Arc::new(stores)
}

pub fn build_controller_map() -> shaperail_runtime::handlers::controller::ControllerMap {
    let mut map = shaperail_runtime::handlers::controller::ControllerMap::new();
    map.register("users", "hash_password", users_controller::hash_password);
    map
}

pub fn build_job_registry() -> shaperail_runtime::jobs::JobRegistry {
    shaperail_runtime::jobs::JobRegistry::new()
}

pub fn build_handler_map() -> shaperail_runtime::handlers::custom::CustomHandlerMap {
    shaperail_runtime::handlers::custom::CustomHandlerMap::new()
}
