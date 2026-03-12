use std::sync::Arc;

use actix_web::{web, HttpResponse};
use shaperail_core::ShaperailError;

/// Shared health check state holding connections to check.
#[derive(Clone)]
pub struct HealthState {
    pub pool: Option<sqlx::PgPool>,
    pub redis_pool: Option<Arc<deadpool_redis::Pool>>,
}

impl HealthState {
    pub fn new(pool: Option<sqlx::PgPool>, redis_pool: Option<Arc<deadpool_redis::Pool>>) -> Self {
        Self { pool, redis_pool }
    }
}

/// `GET /health` — shallow health check, returns 200 if the process is running.
pub async fn health_handler() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
    }))
}

/// `GET /health/ready` — deep health check. Verifies DB, Redis, and reports status.
///
/// Returns 200 if all checks pass, 503 if any check fails.
pub async fn health_ready_handler(
    health: web::Data<HealthState>,
) -> Result<HttpResponse, ShaperailError> {
    let mut checks = serde_json::Map::new();
    let mut all_ok = true;

    // Check database
    if let Some(ref pool) = health.pool {
        match crate::db::health_check(pool).await {
            Ok(()) => {
                checks.insert("database".to_string(), serde_json::json!({"status": "ok"}));
            }
            Err(e) => {
                all_ok = false;
                checks.insert(
                    "database".to_string(),
                    serde_json::json!({"status": "error", "message": e.to_string()}),
                );
            }
        }
    }

    // Check Redis
    if let Some(ref redis_pool) = health.redis_pool {
        match redis_health_check(redis_pool).await {
            Ok(()) => {
                checks.insert("redis".to_string(), serde_json::json!({"status": "ok"}));
            }
            Err(e) => {
                all_ok = false;
                checks.insert(
                    "redis".to_string(),
                    serde_json::json!({"status": "error", "message": e.to_string()}),
                );
            }
        }
    }

    let status = if all_ok { "ok" } else { "degraded" };
    let body = serde_json::json!({
        "status": status,
        "checks": checks,
    });

    if all_ok {
        Ok(HttpResponse::Ok().json(body))
    } else {
        Ok(HttpResponse::ServiceUnavailable().json(body))
    }
}

/// Performs a PING against Redis to verify connectivity.
async fn redis_health_check(pool: &deadpool_redis::Pool) -> Result<(), ShaperailError> {
    let mut conn = pool
        .get()
        .await
        .map_err(|e| ShaperailError::Internal(format!("Redis health check failed: {e}")))?;
    let _: String = redis::cmd("PING")
        .query_async(&mut conn)
        .await
        .map_err(|e| ShaperailError::Internal(format!("Redis PING failed: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_web::test]
    async fn health_handler_returns_ok() {
        let resp = health_handler().await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn health_ready_returns_503_when_db_down() {
        // Create a pool with an invalid URL to simulate DB down
        let bad_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgresql://bad:bad@localhost:59999/nope")
            .unwrap();

        let health = HealthState::new(Some(bad_pool), None);
        let health_data = web::Data::new(health);

        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(health_data)
                .route("/health/ready", web::get().to(health_ready_handler)),
        )
        .await;

        let req = actix_web::test::TestRequest::get()
            .uri("/health/ready")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 503);
    }

    #[actix_web::test]
    async fn health_ready_returns_ok_when_no_deps() {
        let health = HealthState::new(None, None);
        let health_data = web::Data::new(health);

        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(health_data)
                .route("/health/ready", web::get().to(health_ready_handler)),
        )
        .await;

        let req = actix_web::test::TestRequest::get()
            .uri("/health/ready")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }
}
