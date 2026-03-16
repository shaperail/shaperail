//! gRPC server builder (M16).
//!
//! Builds a Tonic gRPC server with dynamic resource services, JWT auth
//! interceptor, server reflection, and health check.

use std::net::SocketAddr;
use std::sync::Arc;

use http_body_util::BodyExt;
use prost::bytes::Bytes;
use shaperail_core::{GrpcConfig, ResourceDefinition};
use tokio::task::JoinHandle;
use tonic::server::NamedService;
use tonic::transport::Server;
use tonic::Status;

use super::service;
use crate::auth::extractor::AuthenticatedUser;
use crate::auth::jwt::JwtConfig;
use crate::handlers::crud::AppState;

/// Handle to a running gRPC server — can be used to await or abort.
pub struct GrpcServerHandle {
    pub handle: JoinHandle<Result<(), tonic::transport::Error>>,
    pub addr: SocketAddr,
}

/// Dynamic gRPC service that routes to resource handlers based on path.
#[derive(Clone)]
pub struct ShaperailGrpcService {
    state: Arc<AppState>,
    resources: Vec<ResourceDefinition>,
    jwt_config: Option<Arc<JwtConfig>>,
}

impl ShaperailGrpcService {
    pub fn new(
        state: Arc<AppState>,
        resources: Vec<ResourceDefinition>,
        jwt_config: Option<Arc<JwtConfig>>,
    ) -> Self {
        Self {
            state,
            resources,
            jwt_config,
        }
    }

    /// Parse a gRPC path like `/shaperail.v1.users.UserService/GetUser`
    /// into (resource_name, method_name).
    pub fn parse_grpc_path(path: &str) -> Option<(String, String)> {
        let path = path.strip_prefix('/')?;
        let (service_part, method) = path.split_once('/')?;
        let parts: Vec<&str> = service_part.split('.').collect();
        if parts.len() >= 4 && parts[0] == "shaperail" {
            let resource_name = parts[2].to_string();
            Some((resource_name, method.to_string()))
        } else {
            None
        }
    }

    /// Handle a unary or server-streaming gRPC call.
    async fn handle_request(
        &self,
        resource_name: &str,
        method_name: &str,
        user: Option<&AuthenticatedUser>,
        body: &[u8],
    ) -> Result<GrpcResponse, Status> {
        let resource = self
            .resources
            .iter()
            .find(|r| r.resource == resource_name)
            .ok_or_else(|| Status::not_found(format!("Unknown resource: {resource_name}")))?;

        if method_name.starts_with("Get") {
            let data = service::handle_get(self.state.clone(), resource, user, body).await?;
            Ok(GrpcResponse::Unary(data))
        } else if method_name.starts_with("Stream") {
            let items =
                service::handle_stream_list(self.state.clone(), resource, user, body).await?;
            Ok(GrpcResponse::Stream(items))
        } else if method_name.starts_with("List") {
            let data = service::handle_list(self.state.clone(), resource, user, body).await?;
            Ok(GrpcResponse::Unary(data))
        } else if method_name.starts_with("Create") {
            let data = service::handle_create(self.state.clone(), resource, user, body).await?;
            Ok(GrpcResponse::Unary(data))
        } else if method_name.starts_with("Update") {
            Err(Status::unimplemented("Update not yet implemented"))
        } else if method_name.starts_with("Delete") {
            let data = service::handle_delete(self.state.clone(), resource, user, body).await?;
            Ok(GrpcResponse::Unary(data))
        } else {
            Err(Status::unimplemented(format!(
                "Unknown method: {method_name}"
            )))
        }
    }
}

enum GrpcResponse {
    Unary(Bytes),
    Stream(Vec<Bytes>),
}

/// The tonic body type used in 0.12.
type TonicBody = tonic::body::BoxBody;

/// Wrapper implementing tonic's Service trait for dynamic dispatch.
#[derive(Clone)]
struct ShaperailGrpcServiceServer {
    inner: ShaperailGrpcService,
}

impl NamedService for ShaperailGrpcServiceServer {
    const NAME: &'static str = "shaperail";
}

impl tower::Service<http::Request<TonicBody>> for ShaperailGrpcServiceServer {
    type Response = http::Response<TonicBody>;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<TonicBody>) -> Self::Future {
        let inner = self.inner.clone();

        Box::pin(async move {
            let path = req.uri().path().to_string();

            // Extract auth from headers
            let user = extract_user_from_headers(req.headers(), inner.jwt_config.as_deref());

            // Collect body bytes
            let body_bytes = collect_body(req.into_body()).await;

            // Strip gRPC framing: 1 byte compression + 4 bytes length
            let message_data = if body_bytes.len() >= 5 {
                &body_bytes[5..]
            } else {
                &body_bytes[..]
            };

            // Parse path and dispatch
            let (resource_name, method_name) = match ShaperailGrpcService::parse_grpc_path(&path) {
                Some(v) => v,
                None => {
                    return Ok(grpc_error_response(
                        tonic::Code::Unimplemented,
                        &format!("Unknown path: {path}"),
                    ));
                }
            };

            match inner
                .handle_request(&resource_name, &method_name, user.as_ref(), message_data)
                .await
            {
                Ok(GrpcResponse::Unary(data)) => Ok(grpc_data_response(&data)),
                Ok(GrpcResponse::Stream(items)) => {
                    let mut combined = Vec::new();
                    for item in &items {
                        let len = item.len() as u32;
                        combined.push(0u8);
                        combined.extend_from_slice(&len.to_be_bytes());
                        combined.extend_from_slice(item);
                    }
                    Ok(grpc_data_response(&combined))
                }
                Err(status) => Ok(grpc_error_response(status.code(), status.message())),
            }
        })
    }
}

/// Extract a user from HTTP headers (for JWT auth via gRPC metadata).
fn extract_user_from_headers(
    headers: &http::HeaderMap,
    jwt_config: Option<&JwtConfig>,
) -> Option<AuthenticatedUser> {
    let auth_str = headers.get("authorization")?.to_str().ok()?;
    let token = auth_str.strip_prefix("Bearer ")?;
    let jwt = jwt_config?;
    let claims = jwt.decode(token).ok()?;
    if claims.token_type != "access" {
        return None;
    }
    Some(AuthenticatedUser {
        id: claims.sub,
        role: claims.role,
        tenant_id: None,
    })
}

/// Collect body bytes from a tonic BoxBody.
async fn collect_body(body: TonicBody) -> Bytes {
    use http_body_util::BodyExt;
    match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => Bytes::new(),
    }
}

/// Build a successful gRPC response with data.
fn grpc_data_response(data: &[u8]) -> http::Response<TonicBody> {
    // gRPC frame: 0 (no compression) + 4 byte big-endian length + data
    let mut frame = Vec::with_capacity(5 + data.len());
    frame.push(0u8);
    let len = data.len() as u32;
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(data);

    let body = http_body_util::Full::new(Bytes::from(frame))
        .map_err(|never: std::convert::Infallible| match never {});
    let boxed = TonicBody::new(body);

    http::Response::builder()
        .status(200)
        .header("content-type", "application/grpc")
        .header("grpc-status", "0")
        .body(boxed)
        .unwrap_or_else(|_| empty_grpc_response(13, "Internal error"))
}

/// Build a gRPC error response.
fn grpc_error_response(code: tonic::Code, message: &str) -> http::Response<TonicBody> {
    empty_grpc_response(code as i32, message)
}

/// Build an empty gRPC response with status and message headers.
fn empty_grpc_response(code: i32, message: &str) -> http::Response<TonicBody> {
    let body = http_body_util::Full::new(Bytes::new())
        .map_err(|never: std::convert::Infallible| match never {});
    let boxed = TonicBody::new(body);

    http::Response::builder()
        .status(200)
        .header("content-type", "application/grpc")
        .header("grpc-status", code.to_string())
        .header("grpc-message", message)
        .body(boxed)
        .unwrap_or_else(|_| {
            // Last resort fallback
            let fb = http_body_util::Full::new(Bytes::new())
                .map_err(|never: std::convert::Infallible| match never {});
            http::Response::new(TonicBody::new(fb))
        })
}

/// Build and start the gRPC server.
///
/// Returns a `GrpcServerHandle` that can be awaited or aborted.
/// The server runs on a separate port from the HTTP REST/GraphQL server.
pub async fn build_grpc_server(
    state: Arc<AppState>,
    resources: Vec<ResourceDefinition>,
    jwt_config: Option<Arc<JwtConfig>>,
    grpc_config: Option<&GrpcConfig>,
) -> Result<GrpcServerHandle, Box<dyn std::error::Error + Send + Sync>> {
    let port = grpc_config.map(|c| c.port).unwrap_or(50051);
    let reflection_enabled = grpc_config.map(|c| c.reflection).unwrap_or(true);

    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;

    let svc = ShaperailGrpcService::new(state, resources.clone(), jwt_config);
    let grpc_service = ShaperailGrpcServiceServer { inner: svc };

    // Health service
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<ShaperailGrpcServiceServer>()
        .await;

    for resource in &resources {
        let pascal = to_pascal_case(&to_singular(&resource.resource));
        let service_name = format!(
            "shaperail.v{}.{}.{}Service",
            resource.version, resource.resource, pascal
        );
        health_reporter
            .set_service_status(&service_name, tonic_health::ServingStatus::Serving)
            .await;
    }

    let mut builder = Server::builder();

    let handle = if reflection_enabled {
        let reflection_service = tonic_reflection::server::Builder::configure()
            .build_v1()
            .map_err(|e| format!("Failed to build reflection service: {e}"))?;

        let router = builder
            .add_service(health_service)
            .add_service(reflection_service)
            .add_service(grpc_service);

        tokio::spawn(async move { router.serve(addr).await })
    } else {
        let router = builder
            .add_service(health_service)
            .add_service(grpc_service);

        tokio::spawn(async move { router.serve(addr).await })
    };

    tracing::info!("gRPC server listening on {addr}");

    Ok(GrpcServerHandle { handle, addr })
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + chars.as_str()
                }
                None => String::new(),
            }
        })
        .collect()
}

fn to_singular(s: &str) -> String {
    const EXCEPTIONS: &[&str] = &["status", "bus", "alias", "canvas"];
    if EXCEPTIONS.iter().any(|e| s.ends_with(e)) {
        return s.to_string();
    }
    if let Some(stripped) = s.strip_suffix("ies") {
        format!("{stripped}y")
    } else if s.ends_with("ses") || s.ends_with("xes") || s.ends_with("zes") {
        s[..s.len() - 2].to_string()
    } else if let Some(stripped) = s.strip_suffix('s') {
        if stripped.ends_with('s') {
            s.to_string()
        } else {
            stripped.to_string()
        }
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_grpc_path_valid() {
        let result =
            ShaperailGrpcService::parse_grpc_path("/shaperail.v1.users.UserService/GetUser");
        assert_eq!(result, Some(("users".to_string(), "GetUser".to_string())));
    }

    #[test]
    fn parse_grpc_path_list() {
        let result =
            ShaperailGrpcService::parse_grpc_path("/shaperail.v1.orders.OrderService/ListOrders");
        assert_eq!(
            result,
            Some(("orders".to_string(), "ListOrders".to_string()))
        );
    }

    #[test]
    fn parse_grpc_path_invalid() {
        assert!(ShaperailGrpcService::parse_grpc_path("/invalid").is_none());
        assert!(ShaperailGrpcService::parse_grpc_path("").is_none());
    }

    #[test]
    fn parse_grpc_path_stream() {
        let result =
            ShaperailGrpcService::parse_grpc_path("/shaperail.v1.users.UserService/StreamUsers");
        assert_eq!(
            result,
            Some(("users".to_string(), "StreamUsers".to_string()))
        );
    }

    #[test]
    fn pascal_and_singular() {
        assert_eq!(to_pascal_case("user"), "User");
        assert_eq!(to_pascal_case("blog_post"), "BlogPost");
        assert_eq!(to_singular("users"), "user");
        assert_eq!(to_singular("categories"), "category");
    }

    #[test]
    fn extract_user_no_header() {
        let headers = http::HeaderMap::new();
        assert!(extract_user_from_headers(&headers, None).is_none());
    }

    #[test]
    fn extract_user_valid_token() {
        let jwt = JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400);
        let token = jwt.encode_access("user-1", "admin").unwrap();

        let mut headers = http::HeaderMap::new();
        headers.insert(
            "authorization",
            http::HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );

        let user = extract_user_from_headers(&headers, Some(&jwt));
        assert!(user.is_some());
        let user = user.unwrap();
        assert_eq!(user.id, "user-1");
        assert_eq!(user.role, "admin");
    }

    #[test]
    fn extract_user_invalid_token() {
        let jwt = JwtConfig::new("test-secret-key-at-least-32-bytes-long!", 3600, 86400);

        let mut headers = http::HeaderMap::new();
        headers.insert(
            "authorization",
            http::HeaderValue::from_str("Bearer invalid.token.here").unwrap(),
        );

        assert!(extract_user_from_headers(&headers, Some(&jwt)).is_none());
    }
}
