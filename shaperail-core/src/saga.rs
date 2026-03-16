use serde::{Deserialize, Serialize};

/// A distributed saga definition parsed from `sagas/<name>.saga.yaml`.
///
/// Sagas coordinate multi-service transactions with compensating actions.
///
/// ```yaml
/// saga: create_order
/// version: 1
/// steps:
///   - name: reserve_inventory
///     service: inventory-api
///     action: POST /v1/reservations
///     compensate: DELETE /v1/reservations/:id
///     timeout_secs: 5
///   - name: charge_payment
///     service: payments-api
///     action: POST /v1/charges
///     compensate: POST /v1/charges/:id/refund
///     timeout_secs: 10
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SagaDefinition {
    /// Saga name (unique within workspace).
    pub saga: String,

    /// Saga version.
    #[serde(default = "default_saga_version")]
    pub version: u32,

    /// Ordered list of saga steps. Executed sequentially; on failure,
    /// compensating actions run in reverse order.
    pub steps: Vec<SagaStep>,
}

fn default_saga_version() -> u32 {
    1
}

/// A single step within a saga.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SagaStep {
    /// Step name (unique within saga).
    pub name: String,

    /// Target service name (must exist in workspace).
    pub service: String,

    /// Forward action: HTTP method + path (e.g. "POST /v1/reservations").
    pub action: String,

    /// Compensating action: HTTP method + path, run on rollback.
    pub compensate: String,

    /// Optional input mapping (JSON template).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,

    /// Step timeout in seconds. Default: 30.
    #[serde(default = "default_step_timeout")]
    pub timeout_secs: u64,
}

fn default_step_timeout() -> u64 {
    30
}

/// Runtime status of a saga execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SagaExecutionStatus {
    /// Saga is running forward steps.
    Running,
    /// All steps completed successfully.
    Completed,
    /// A step failed; compensating actions are running.
    Compensating,
    /// All compensating actions finished (saga rolled back).
    Compensated,
    /// Compensating actions also failed — requires manual intervention.
    Failed,
}

impl std::fmt::Display for SagaExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Compensating => write!(f, "compensating"),
            Self::Compensated => write!(f, "compensated"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saga_definition_minimal() {
        let json = r#"{
            "saga": "create_order",
            "steps": [
                {
                    "name": "reserve_inventory",
                    "service": "inventory-api",
                    "action": "POST /v1/reservations",
                    "compensate": "DELETE /v1/reservations/:id"
                }
            ]
        }"#;
        let saga: SagaDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(saga.saga, "create_order");
        assert_eq!(saga.version, 1);
        assert_eq!(saga.steps.len(), 1);
        assert_eq!(saga.steps[0].timeout_secs, 30);
    }

    #[test]
    fn saga_definition_full() {
        let json = r#"{
            "saga": "create_order",
            "version": 2,
            "steps": [
                {
                    "name": "reserve",
                    "service": "inventory-api",
                    "action": "POST /v1/reservations",
                    "compensate": "DELETE /v1/reservations/:id",
                    "input": {"product_id": "from:order.product_id"},
                    "timeout_secs": 5
                },
                {
                    "name": "charge",
                    "service": "payments-api",
                    "action": "POST /v1/charges",
                    "compensate": "POST /v1/charges/:id/refund",
                    "timeout_secs": 10
                }
            ]
        }"#;
        let saga: SagaDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(saga.version, 2);
        assert_eq!(saga.steps.len(), 2);
        assert_eq!(saga.steps[0].timeout_secs, 5);
        assert!(saga.steps[0].input.is_some());
        assert_eq!(saga.steps[1].service, "payments-api");
    }

    #[test]
    fn saga_definition_unknown_field_fails() {
        let json = r#"{
            "saga": "test",
            "steps": [],
            "unknown": true
        }"#;
        let err = serde_json::from_str::<SagaDefinition>(json);
        assert!(err.is_err());
    }

    #[test]
    fn saga_definition_serde_roundtrip() {
        let saga = SagaDefinition {
            saga: "roundtrip".to_string(),
            version: 1,
            steps: vec![
                SagaStep {
                    name: "step1".to_string(),
                    service: "svc-a".to_string(),
                    action: "POST /v1/items".to_string(),
                    compensate: "DELETE /v1/items/:id".to_string(),
                    input: None,
                    timeout_secs: 30,
                },
                SagaStep {
                    name: "step2".to_string(),
                    service: "svc-b".to_string(),
                    action: "POST /v1/records".to_string(),
                    compensate: "DELETE /v1/records/:id".to_string(),
                    input: Some(serde_json::json!({"key": "value"})),
                    timeout_secs: 15,
                },
            ],
        };
        let json = serde_json::to_string(&saga).unwrap();
        let back: SagaDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(saga, back);
    }

    #[test]
    fn saga_execution_status_display() {
        assert_eq!(SagaExecutionStatus::Running.to_string(), "running");
        assert_eq!(SagaExecutionStatus::Completed.to_string(), "completed");
        assert_eq!(
            SagaExecutionStatus::Compensating.to_string(),
            "compensating"
        );
        assert_eq!(SagaExecutionStatus::Compensated.to_string(), "compensated");
        assert_eq!(SagaExecutionStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn saga_step_defaults() {
        let json = r#"{
            "name": "step",
            "service": "svc",
            "action": "POST /v1/x",
            "compensate": "DELETE /v1/x/:id"
        }"#;
        let step: SagaStep = serde_json::from_str(json).unwrap();
        assert_eq!(step.timeout_secs, 30);
        assert!(step.input.is_none());
    }
}
