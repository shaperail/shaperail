use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use shaperail_runtime::handlers::response::SingleResponse;
use shaperail_runtime::observability::health_handler;

fn bench_health_handler(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build benchmark runtime");

    let mut group = c.benchmark_group("runtime");
    group.throughput(Throughput::Elements(1));
    group.bench_function("health_handler", |b| {
        b.to_async(&runtime).iter(|| async {
            let response = health_handler().await;
            assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        });
    });
    group.finish();
}

/// Measures the JSON serialization throughput of a health-style response envelope.
///
/// This isolates the serialization cost from the async handler overhead,
/// providing a baseline for the "simple JSON response" PRD target.
fn bench_response_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime");
    let data = serde_json::json!({"status": "healthy", "version": "0.2.2"});

    group.throughput(Throughput::Elements(1));
    group.bench_function("health_response_json_encode", |b| {
        b.iter(|| {
            let envelope = SingleResponse {
                data: black_box(data.clone()),
            };
            let bytes = serde_json::to_vec(&envelope).unwrap();
            black_box(bytes);
        });
    });
    group.finish();
}

criterion_group!(benches, bench_health_handler, bench_response_serialization);
criterion_main!(benches);
