//! gRPC support (M16). Dynamic service from resources, streaming RPCs,
//! JWT auth via metadata interceptors, server reflection, health checks.

mod codec;
mod interceptor;
mod server;
mod service;

pub use interceptor::{auth_interceptor, extract_grpc_auth};
pub use server::{build_grpc_server, GrpcServerHandle};
