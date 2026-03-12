pub mod crud;
pub mod params;
pub mod relations;
pub mod response;
pub mod routes;
pub mod validate;

pub use crud::AppState;
pub use routes::{register_all_resources, register_resource};
