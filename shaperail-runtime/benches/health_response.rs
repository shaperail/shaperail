use criterion::{criterion_group, criterion_main, Criterion, Throughput};
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

criterion_group!(benches, bench_health_handler);
criterion_main!(benches);
