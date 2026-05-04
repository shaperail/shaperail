pub mod config_parser;
pub mod diagnostics {
    mod inner;
    pub mod registry;
    pub use inner::*;
}
pub mod feature_check;
pub mod json_schema;
pub mod openapi;
pub mod parser;
pub mod proto;
pub mod rust;
pub mod service_client;
pub mod typescript;
pub mod validator;
pub mod workspace_parser;
