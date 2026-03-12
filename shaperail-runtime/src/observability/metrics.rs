use std::sync::Arc;

use actix_web::{web, HttpResponse};
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry, TextEncoder,
};
use shaperail_core::ShaperailError;

/// Shared metrics state holding all Prometheus metric collectors.
#[derive(Clone)]
pub struct MetricsState {
    pub registry: Arc<Registry>,
    pub http_requests_total: IntCounterVec,
    pub http_request_duration: HistogramVec,
    pub db_pool_size: IntGauge,
    pub cache_hit_total: IntCounterVec,
    pub job_queue_depth: IntGauge,
    pub error_rate: IntCounterVec,
}

impl MetricsState {
    pub fn new() -> Result<Self, ShaperailError> {
        let registry = Registry::new();

        let http_requests_total = IntCounterVec::new(
            Opts::new("shaperail_http_requests_total", "Total HTTP requests"),
            &["method", "path", "status"],
        )
        .map_err(|e| ShaperailError::Internal(format!("Failed to create metric: {e}")))?;

        let http_request_duration = HistogramVec::new(
            HistogramOpts::new(
                "shaperail_http_request_duration_seconds",
                "HTTP request duration in seconds",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0,
            ]),
            &["method", "path"],
        )
        .map_err(|e| ShaperailError::Internal(format!("Failed to create metric: {e}")))?;

        let db_pool_size =
            IntGauge::new("shaperail_db_pool_size", "Current DB connection pool size")
                .map_err(|e| ShaperailError::Internal(format!("Failed to create metric: {e}")))?;

        let cache_hit_total = IntCounterVec::new(
            Opts::new("shaperail_cache_total", "Cache operations"),
            &["result"], // "hit" or "miss"
        )
        .map_err(|e| ShaperailError::Internal(format!("Failed to create metric: {e}")))?;

        let job_queue_depth = IntGauge::new("shaperail_job_queue_depth", "Current job queue depth")
            .map_err(|e| ShaperailError::Internal(format!("Failed to create metric: {e}")))?;

        let error_rate = IntCounterVec::new(
            Opts::new("shaperail_errors_total", "Total errors by type"),
            &["error_type"],
        )
        .map_err(|e| ShaperailError::Internal(format!("Failed to create metric: {e}")))?;

        registry
            .register(Box::new(http_requests_total.clone()))
            .map_err(|e| ShaperailError::Internal(format!("Failed to register metric: {e}")))?;
        registry
            .register(Box::new(http_request_duration.clone()))
            .map_err(|e| ShaperailError::Internal(format!("Failed to register metric: {e}")))?;
        registry
            .register(Box::new(db_pool_size.clone()))
            .map_err(|e| ShaperailError::Internal(format!("Failed to register metric: {e}")))?;
        registry
            .register(Box::new(cache_hit_total.clone()))
            .map_err(|e| ShaperailError::Internal(format!("Failed to register metric: {e}")))?;
        registry
            .register(Box::new(job_queue_depth.clone()))
            .map_err(|e| ShaperailError::Internal(format!("Failed to register metric: {e}")))?;
        registry
            .register(Box::new(error_rate.clone()))
            .map_err(|e| ShaperailError::Internal(format!("Failed to register metric: {e}")))?;

        Ok(Self {
            registry: Arc::new(registry),
            http_requests_total,
            http_request_duration,
            db_pool_size,
            cache_hit_total,
            job_queue_depth,
            error_rate,
        })
    }

    /// Records an HTTP request metric.
    pub fn record_request(&self, method: &str, path: &str, status: u16, duration_secs: f64) {
        self.http_requests_total
            .with_label_values(&[method, path, &status.to_string()])
            .inc();
        self.http_request_duration
            .with_label_values(&[method, path])
            .observe(duration_secs);
    }

    /// Records a cache hit or miss.
    pub fn record_cache(&self, hit: bool) {
        let label = if hit { "hit" } else { "miss" };
        self.cache_hit_total.with_label_values(&[label]).inc();
    }

    /// Records an error by type.
    pub fn record_error(&self, error_type: &str) {
        self.error_rate.with_label_values(&[error_type]).inc();
    }

    /// Updates the DB pool size gauge.
    pub fn set_db_pool_size(&self, size: i64) {
        self.db_pool_size.set(size);
    }

    /// Updates the job queue depth gauge.
    pub fn set_job_queue_depth(&self, depth: i64) {
        self.job_queue_depth.set(depth);
    }
}

/// `GET /metrics` — returns Prometheus text format metrics.
pub async fn metrics_handler(
    metrics: web::Data<MetricsState>,
) -> Result<HttpResponse, ShaperailError> {
    let encoder = TextEncoder::new();
    let metric_families = metrics.registry.gather();
    let mut buffer = Vec::new();
    encoder
        .encode(&metric_families, &mut buffer)
        .map_err(|e| ShaperailError::Internal(format!("Failed to encode metrics: {e}")))?;
    let output = String::from_utf8(buffer)
        .map_err(|e| ShaperailError::Internal(format!("Metrics encoding error: {e}")))?;
    Ok(HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_state_creation() {
        let state = MetricsState::new().unwrap();
        state.record_request("GET", "/users", 200, 0.015);
        state.record_cache(true);
        state.record_cache(false);
        state.record_error("not_found");
        state.set_db_pool_size(10);
        state.set_job_queue_depth(5);

        let encoder = TextEncoder::new();
        let families = state.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&families, &mut buffer).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("shaperail_http_requests_total"));
        assert!(output.contains("shaperail_http_request_duration_seconds"));
        assert!(output.contains("shaperail_db_pool_size"));
        assert!(output.contains("shaperail_cache_total"));
        assert!(output.contains("shaperail_job_queue_depth"));
        assert!(output.contains("shaperail_errors_total"));
    }

    #[test]
    fn metrics_prometheus_format() {
        let state = MetricsState::new().unwrap();
        state.record_request("GET", "/users", 200, 0.01);

        let encoder = TextEncoder::new();
        let families = state.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&families, &mut buffer).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        // Prometheus format: # HELP, # TYPE, metric_name{labels} value
        assert!(output.contains("# HELP shaperail_http_requests_total"));
        assert!(output.contains("# TYPE shaperail_http_requests_total counter"));
        assert!(output.contains(
            r#"shaperail_http_requests_total{method="GET",path="/users",status="200"} 1"#
        ));
    }
}
