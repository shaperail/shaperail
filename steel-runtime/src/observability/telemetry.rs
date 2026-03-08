use steel_core::SteelError;

/// Initializes OpenTelemetry tracing with OTLP export.
///
/// Reads configuration from environment variables:
/// - `OTEL_EXPORTER_OTLP_ENDPOINT` — OTLP endpoint (e.g., `http://localhost:4317`)
/// - `OTEL_SERVICE_NAME` — service name (defaults to `steel-api`)
///
/// If `OTEL_EXPORTER_OTLP_ENDPOINT` is not set, telemetry is disabled (no-op).
///
/// Spans are created for: HTTP requests, DB queries, cache operations, job execution.
pub fn init_telemetry() -> Result<Option<opentelemetry_sdk::trace::TracerProvider>, SteelError> {
    let endpoint = match std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        Ok(ep) if !ep.is_empty() => ep,
        _ => return Ok(None),
    };

    let service_name =
        std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "steel-api".to_string());

    use opentelemetry::KeyValue;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::TracerProvider;
    use opentelemetry_sdk::Resource;

    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(&endpoint);

    let config = opentelemetry_sdk::trace::Config::default().with_resource(Resource::new(vec![
        KeyValue::new("service.name", service_name),
    ]));

    let provider = TracerProvider::builder()
        .with_batch_exporter(
            exporter.build_span_exporter().map_err(|e| {
                SteelError::Internal(format!("Failed to create OTLP exporter: {e}"))
            })?,
            opentelemetry_sdk::runtime::Tokio,
        )
        .with_config(config)
        .build();

    Ok(Some(provider))
}

/// Shuts down the OpenTelemetry tracer provider, flushing pending spans.
pub fn shutdown_telemetry(provider: Option<opentelemetry_sdk::trace::TracerProvider>) {
    if let Some(provider) = provider {
        // In opentelemetry_sdk 0.23, shutdown happens via drop.
        // Force flush first to ensure all spans are exported.
        for result in provider.force_flush() {
            if let Err(e) = result {
                tracing::warn!(error = %e, "Failed to flush OpenTelemetry spans");
            }
        }
        drop(provider);
    }
}

/// Records an OpenTelemetry span for a database query.
///
/// Used to instrument `execute_query` and `execute_count` in the DB module.
#[inline]
pub fn db_span(operation: &str, table: &str, sql: &str) -> tracing::Span {
    tracing::info_span!(
        "db.query",
        db.operation = %operation,
        db.table = %table,
        db.statement = %sql,
        otel.kind = "client",
    )
}

/// Records an OpenTelemetry span for a cache operation.
#[inline]
pub fn cache_span(operation: &str, key: &str) -> tracing::Span {
    tracing::info_span!(
        "cache.op",
        cache.operation = %operation,
        cache.key = %key,
        otel.kind = "client",
    )
}

/// Records an OpenTelemetry span for a job execution.
#[inline]
pub fn job_span(job_name: &str, job_id: &str) -> tracing::Span {
    tracing::info_span!(
        "job.execute",
        job.name = %job_name,
        job.id = %job_id,
        otel.kind = "consumer",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_telemetry_returns_none_when_no_env() {
        std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        let result = init_telemetry().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn db_span_created() {
        let _subscriber = tracing_subscriber::fmt().with_test_writer().try_init();
        let span = db_span("select", "users", "SELECT * FROM users");
        // Span is valid (may or may not be disabled depending on subscriber state)
        let _ = span;
    }

    #[test]
    fn cache_span_created() {
        let _subscriber = tracing_subscriber::fmt().with_test_writer().try_init();
        let span = cache_span("get", "steel:users:list:abc:admin");
        let _ = span;
    }

    #[test]
    fn job_span_created() {
        let _subscriber = tracing_subscriber::fmt().with_test_writer().try_init();
        let span = job_span("send_email", "job-123");
        let _ = span;
    }
}
