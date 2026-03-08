use std::collections::HashSet;
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::Error;

/// Actix-web middleware that logs every request/response as a single structured JSON line.
///
/// Logged fields: method, path, status, duration_ms, user_id, request_id.
/// Sensitive fields from the resource schema are never included in log output.
#[derive(Clone)]
pub struct RequestLogger {
    sensitive_fields: Arc<HashSet<String>>,
}

impl RequestLogger {
    pub fn new(sensitive_fields: HashSet<String>) -> Self {
        Self {
            sensitive_fields: Arc::new(sensitive_fields),
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RequestLogger
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = RequestLoggerMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequestLoggerMiddleware {
            service,
            _sensitive_fields: self.sensitive_fields.clone(),
        }))
    }
}

pub struct RequestLoggerMiddleware<S> {
    service: S,
    _sensitive_fields: Arc<HashSet<String>>,
}

impl<S, B> Service<ServiceRequest> for RequestLoggerMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let start = Instant::now();
        let method = req.method().to_string();
        let path = req.path().to_string();
        let request_id = uuid::Uuid::new_v4().to_string();

        // Extract user_id from request extensions if auth middleware has run
        let user_id = req
            .headers()
            .get("x-request-user-id")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        // Insert request_id header for downstream use
        req.headers_mut().insert(
            actix_web::http::header::HeaderName::from_static("x-request-id"),
            actix_web::http::header::HeaderValue::from_str(&request_id)
                .unwrap_or_else(|_| actix_web::http::header::HeaderValue::from_static("unknown")),
        );

        let fut = self.service.call(req);

        Box::pin(async move {
            let res = fut.await?;

            let status = res.status().as_u16();
            let duration_ms = start.elapsed().as_millis() as u64;

            tracing::info!(
                request_id = %request_id,
                method = %method,
                path = %path,
                status = status,
                duration_ms = duration_ms,
                user_id = user_id.as_deref().unwrap_or("-"),
                "request completed"
            );

            Ok(res)
        })
    }
}

/// Extracts the request_id from the request headers.
pub fn get_request_id(req: &actix_web::HttpRequest) -> String {
    req.headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_logger_new() {
        let sensitive = HashSet::new();
        let logger = RequestLogger::new(sensitive);
        assert!(logger.sensitive_fields.is_empty());
    }

    #[test]
    fn get_request_id_missing() {
        let req = actix_web::test::TestRequest::default().to_http_request();
        assert_eq!(get_request_id(&req), "-");
    }

    #[test]
    fn get_request_id_present() {
        let req = actix_web::test::TestRequest::default()
            .insert_header(("x-request-id", "abc-123"))
            .to_http_request();
        assert_eq!(get_request_id(&req), "abc-123");
    }
}
