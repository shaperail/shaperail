//! Saga state machine executor.

use std::collections::HashMap;
use std::sync::Arc;

use shaperail_core::{SagaDefinition, SagaExecutionStatus};
use sqlx::PgPool;

/// SQL to create the saga_executions table (run once at startup if not exists).
pub const CREATE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS saga_executions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    saga_name   TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'running',
    current_step INTEGER NOT NULL DEFAULT 0,
    step_results JSONB NOT NULL DEFAULT '[]',
    input       JSONB,
    error       TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
)
"#;

/// Runtime record for a saga execution.
#[derive(Debug, sqlx::FromRow)]
pub struct SagaExecution {
    pub id: uuid::Uuid,
    pub saga_name: String,
    pub status: String,
    pub current_step: i32,
    pub step_results: serde_json::Value,
    pub input: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Drives saga state machines: starts executions, advances steps, triggers compensation.
pub struct SagaExecutor {
    pool: PgPool,
    /// Maps service name → base URL (e.g., "inventory-api" → "http://inventory:8080")
    service_urls: HashMap<String, String>,
    http_client: reqwest::Client,
}

impl SagaExecutor {
    pub fn new(pool: PgPool, service_urls: HashMap<String, String>) -> Self {
        Self {
            pool,
            service_urls,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn ensure_table(&self) -> Result<(), sqlx::Error> {
        sqlx::query(CREATE_TABLE_SQL).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn start(
        self: &Arc<Self>,
        saga: &SagaDefinition,
        input: serde_json::Value,
    ) -> Result<uuid::Uuid, shaperail_core::ShaperailError> {
        let row: (uuid::Uuid,) = sqlx::query_as(
            "INSERT INTO saga_executions (saga_name, input) VALUES ($1, $2) RETURNING id",
        )
        .bind(&saga.saga)
        .bind(&input)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| shaperail_core::ShaperailError::Internal(e.to_string()))?;

        let execution_id = row.0;
        let executor = Arc::clone(self);
        let saga_clone = saga.clone();
        tokio::spawn(async move {
            if let Err(e) = executor.advance(&execution_id, &saga_clone).await {
                tracing::error!(execution_id = %execution_id, error = %e, "Saga advance failed");
            }
        });

        Ok(execution_id)
    }

    pub async fn advance(
        self: &Arc<Self>,
        execution_id: &uuid::Uuid,
        saga: &SagaDefinition,
    ) -> Result<SagaExecutionStatus, shaperail_core::ShaperailError> {
        loop {
            let exec: SagaExecution = sqlx::query_as("SELECT * FROM saga_executions WHERE id = $1")
                .bind(execution_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| shaperail_core::ShaperailError::Internal(e.to_string()))?;

            let step_index = exec.current_step as usize;
            if step_index >= saga.steps.len() {
                self.update_status(execution_id, SagaExecutionStatus::Completed, None)
                    .await?;
                return Ok(SagaExecutionStatus::Completed);
            }

            let step = &saga.steps[step_index];
            let base_url = self.service_urls.get(&step.service).ok_or_else(|| {
                shaperail_core::ShaperailError::Internal(format!(
                    "Service '{}' not in service registry",
                    step.service
                ))
            })?;

            let (method, path) = parse_action(&step.action)?;
            let url = format!("{base_url}{path}");
            let input = exec.input.clone().unwrap_or(serde_json::Value::Null);

            let response = self
                .http_client
                .request(method, &url)
                .json(&input)
                .timeout(std::time::Duration::from_secs(step.timeout_secs))
                .send()
                .await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    let result = resp
                        .json::<serde_json::Value>()
                        .await
                        .unwrap_or(serde_json::Value::Null);
                    sqlx::query(
                        "UPDATE saga_executions
                         SET current_step = current_step + 1,
                             step_results = step_results || $1::jsonb,
                             updated_at = NOW()
                         WHERE id = $2",
                    )
                    .bind(serde_json::json!([result]))
                    .bind(execution_id)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| shaperail_core::ShaperailError::Internal(e.to_string()))?;
                    // continue loop to process next step
                }
                Ok(resp) => {
                    let error_msg =
                        format!("Step '{}' failed with HTTP {}", step.name, resp.status());
                    self.update_status(
                        execution_id,
                        SagaExecutionStatus::Compensating,
                        Some(&error_msg),
                    )
                    .await?;
                    let executor = Arc::clone(self);
                    let exec_id = *execution_id;
                    let saga_clone = saga.clone();
                    tokio::spawn(async move {
                        if let Err(e) = executor.compensate(&exec_id, &saga_clone).await {
                            tracing::error!(execution_id = %exec_id, error = %e, "Saga compensation failed");
                        }
                    });
                    return Ok(SagaExecutionStatus::Compensating);
                }
                Err(e) => {
                    let error_msg = format!("Step '{}' request error: {e}", step.name);
                    self.update_status(
                        execution_id,
                        SagaExecutionStatus::Compensating,
                        Some(&error_msg),
                    )
                    .await?;
                    let executor = Arc::clone(self);
                    let exec_id = *execution_id;
                    let saga_clone = saga.clone();
                    tokio::spawn(async move {
                        if let Err(e) = executor.compensate(&exec_id, &saga_clone).await {
                            tracing::error!(execution_id = %exec_id, error = %e, "Saga compensation failed");
                        }
                    });
                    return Ok(SagaExecutionStatus::Compensating);
                }
            }
        }
    }

    pub async fn compensate(
        self: &Arc<Self>,
        execution_id: &uuid::Uuid,
        saga: &SagaDefinition,
    ) -> Result<(), shaperail_core::ShaperailError> {
        let exec: SagaExecution = sqlx::query_as("SELECT * FROM saga_executions WHERE id = $1")
            .bind(execution_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| shaperail_core::ShaperailError::Internal(e.to_string()))?;

        let completed_steps = exec.current_step as usize;
        let step_results: Vec<serde_json::Value> =
            serde_json::from_value(exec.step_results).unwrap_or_default();

        for i in (0..completed_steps).rev() {
            let step = &saga.steps[i];
            let base_url = match self.service_urls.get(&step.service) {
                Some(url) => url.clone(),
                None => continue,
            };

            let (method, path) = match parse_action(&step.compensate) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let result_id = step_results
                .get(i)
                .and_then(|r| r.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let path = path.replace(":id", result_id);
            let url = format!("{base_url}{path}");

            let _ = self
                .http_client
                .request(method, &url)
                .timeout(std::time::Duration::from_secs(step.timeout_secs))
                .send()
                .await;
        }

        self.update_status(execution_id, SagaExecutionStatus::Compensated, None)
            .await?;
        Ok(())
    }

    pub async fn get_status(
        &self,
        execution_id: &uuid::Uuid,
    ) -> Result<SagaExecution, shaperail_core::ShaperailError> {
        sqlx::query_as("SELECT * FROM saga_executions WHERE id = $1")
            .bind(execution_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| shaperail_core::ShaperailError::Internal(e.to_string()))?
            .ok_or(shaperail_core::ShaperailError::NotFound)
    }

    async fn update_status(
        &self,
        execution_id: &uuid::Uuid,
        status: SagaExecutionStatus,
        error: Option<&str>,
    ) -> Result<(), shaperail_core::ShaperailError> {
        sqlx::query(
            "UPDATE saga_executions SET status = $1, error = $2, updated_at = NOW() WHERE id = $3",
        )
        .bind(status.to_string())
        .bind(error)
        .bind(execution_id)
        .execute(&self.pool)
        .await
        .map_err(|e| shaperail_core::ShaperailError::Internal(e.to_string()))?;
        Ok(())
    }
}

/// Load all saga definitions from `*.saga.yaml` files in `dir`.
pub fn load_sagas(dir: &std::path::Path) -> Vec<SagaDefinition> {
    if !dir.exists() {
        return vec![];
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut sagas = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml")
            && path.to_str().is_some_and(|s| s.contains(".saga."))
        {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(saga) = serde_yaml::from_str::<SagaDefinition>(&content) {
                    sagas.push(saga);
                }
            }
        }
    }
    sagas
}

fn parse_action(action: &str) -> Result<(reqwest::Method, String), shaperail_core::ShaperailError> {
    let parts: Vec<&str> = action.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(shaperail_core::ShaperailError::Internal(format!(
            "Invalid saga action format: '{action}' — expected 'METHOD /path'"
        )));
    }
    let method = parts[0].parse::<reqwest::Method>().map_err(|_| {
        shaperail_core::ShaperailError::Internal(format!("Unknown HTTP method: {}", parts[0]))
    })?;
    Ok((method, parts[1].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_post() {
        let (method, path) = parse_action("POST /v1/reservations").unwrap();
        assert_eq!(method, reqwest::Method::POST);
        assert_eq!(path, "/v1/reservations");
    }

    #[test]
    fn parse_action_delete_with_id() {
        let (method, path) = parse_action("DELETE /v1/reservations/:id").unwrap();
        assert_eq!(method, reqwest::Method::DELETE);
        assert_eq!(path, "/v1/reservations/:id");
    }

    #[test]
    fn parse_action_invalid_format() {
        let result = parse_action("not-valid");
        assert!(result.is_err());
    }
}
