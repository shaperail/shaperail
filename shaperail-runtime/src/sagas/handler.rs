//! HTTP handlers for saga route endpoints.

use std::sync::Arc;

use actix_web::{web, HttpResponse, Responder};
use shaperail_core::SagaDefinition;
use uuid::Uuid;

use crate::handlers::crud::AppState;

/// POST /v1/sagas/{name} — start a saga execution.
pub async fn start_saga(
    path: web::Path<String>,
    body: web::Json<serde_json::Value>,
    state: web::Data<Arc<AppState>>,
    sagas: web::Data<Vec<SagaDefinition>>,
) -> impl Responder {
    let name = path.into_inner();
    let Some(executor) = state.saga_executor.as_ref() else {
        return HttpResponse::ServiceUnavailable()
            .json(serde_json::json!({"error": "Saga executor not configured"}));
    };
    let Some(saga) = sagas.iter().find(|s| s.saga == name) else {
        return HttpResponse::NotFound()
            .json(serde_json::json!({"error": format!("Saga '{name}' not found")}));
    };

    let executor = Arc::clone(executor);
    let saga = saga.clone();
    let input = body.into_inner();
    match executor.start(&saga, input).await {
        Ok(execution_id) => {
            HttpResponse::Accepted().json(serde_json::json!({ "execution_id": execution_id }))
        }
        Err(e) => {
            HttpResponse::InternalServerError().json(serde_json::json!({ "error": e.to_string() }))
        }
    }
}

/// GET /v1/sagas/{id} — get saga execution status.
pub async fn get_saga_status(
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> impl Responder {
    let id_str = path.into_inner();
    let Ok(id) = id_str.parse::<Uuid>() else {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"error": "Invalid execution ID format"}));
    };
    let Some(executor) = state.saga_executor.as_ref() else {
        return HttpResponse::ServiceUnavailable()
            .json(serde_json::json!({"error": "Saga executor not configured"}));
    };

    match executor.get_status(&id).await {
        Ok(exec) => HttpResponse::Ok().json(serde_json::json!({
            "id": exec.id,
            "saga_name": exec.saga_name,
            "status": exec.status,
            "current_step": exec.current_step,
            "error": exec.error,
        })),
        Err(_) => {
            HttpResponse::NotFound().json(serde_json::json!({ "error": "Execution not found" }))
        }
    }
}
