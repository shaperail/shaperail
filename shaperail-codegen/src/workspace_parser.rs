use shaperail_core::{SagaDefinition, WorkspaceConfig};

use crate::parser::ParseError;

/// Parse a YAML string into a `WorkspaceConfig`.
pub fn parse_workspace(yaml: &str) -> Result<WorkspaceConfig, ParseError> {
    let interpolated = crate::config_parser::interpolate_env(yaml)?;
    let config: WorkspaceConfig = serde_yaml::from_str(&interpolated)?;
    validate_workspace(&config)?;
    Ok(config)
}

/// Parse a `shaperail.workspace.yaml` file from disk.
pub fn parse_workspace_file(path: &std::path::Path) -> Result<WorkspaceConfig, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_workspace(&content)
}

/// Parse a saga YAML string into a `SagaDefinition`.
pub fn parse_saga(yaml: &str) -> Result<SagaDefinition, ParseError> {
    let saga: SagaDefinition = serde_yaml::from_str(yaml)?;
    validate_saga(&saga)?;
    Ok(saga)
}

/// Parse a saga YAML file from disk.
pub fn parse_saga_file(path: &std::path::Path) -> Result<SagaDefinition, ParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_saga(&content)
}

/// Validate workspace config: no duplicate ports, valid depends_on references.
fn validate_workspace(config: &WorkspaceConfig) -> Result<(), ParseError> {
    if config.workspace.is_empty() {
        return Err(ParseError::ConfigInterpolation(
            "workspace name cannot be empty".to_string(),
        ));
    }

    if config.services.is_empty() {
        return Err(ParseError::ConfigInterpolation(
            "workspace must declare at least one service".to_string(),
        ));
    }

    // Check depends_on references
    for (name, svc) in &config.services {
        for dep in &svc.depends_on {
            if !config.services.contains_key(dep) {
                return Err(ParseError::ConfigInterpolation(format!(
                    "service '{name}': depends_on references unknown service '{dep}'"
                )));
            }
            if dep == name {
                return Err(ParseError::ConfigInterpolation(format!(
                    "service '{name}': cannot depend on itself"
                )));
            }
        }
    }

    // Check for circular dependencies via topological sort
    if has_circular_deps(config) {
        return Err(ParseError::ConfigInterpolation(
            "workspace has circular service dependencies".to_string(),
        ));
    }

    // Check for duplicate ports
    let mut ports: std::collections::HashMap<u16, &str> = std::collections::HashMap::new();
    for (name, svc) in &config.services {
        if let Some(existing) = ports.get(&svc.port) {
            return Err(ParseError::ConfigInterpolation(format!(
                "services '{existing}' and '{name}' use the same port {port}",
                port = svc.port
            )));
        }
        ports.insert(svc.port, name);
    }

    Ok(())
}

fn has_circular_deps(config: &WorkspaceConfig) -> bool {
    let mut visited = std::collections::HashSet::new();
    let mut in_stack = std::collections::HashSet::new();

    for name in config.services.keys() {
        if !visited.contains(name.as_str()) && dfs_cycle(config, name, &mut visited, &mut in_stack)
        {
            return true;
        }
    }
    false
}

fn dfs_cycle<'a>(
    config: &'a WorkspaceConfig,
    node: &'a str,
    visited: &mut std::collections::HashSet<&'a str>,
    in_stack: &mut std::collections::HashSet<&'a str>,
) -> bool {
    visited.insert(node);
    in_stack.insert(node);

    if let Some(svc) = config.services.get(node) {
        for dep in &svc.depends_on {
            if !visited.contains(dep.as_str()) {
                if dfs_cycle(config, dep, visited, in_stack) {
                    return true;
                }
            } else if in_stack.contains(dep.as_str()) {
                return true;
            }
        }
    }

    in_stack.remove(node);
    false
}

/// Validate saga definition: unique step names, valid action format.
fn validate_saga(saga: &SagaDefinition) -> Result<(), ParseError> {
    if saga.saga.is_empty() {
        return Err(ParseError::ConfigInterpolation(
            "saga name cannot be empty".to_string(),
        ));
    }

    if saga.steps.is_empty() {
        return Err(ParseError::ConfigInterpolation(format!(
            "saga '{}': must have at least one step",
            saga.saga
        )));
    }

    let mut step_names = std::collections::HashSet::new();
    for step in &saga.steps {
        if !step_names.insert(&step.name) {
            return Err(ParseError::ConfigInterpolation(format!(
                "saga '{}': duplicate step name '{}'",
                saga.saga, step.name
            )));
        }

        // Validate action format: "METHOD /path"
        if !is_valid_action(&step.action) {
            return Err(ParseError::ConfigInterpolation(format!(
                "saga '{}' step '{}': action must be 'METHOD /path' (e.g. 'POST /v1/items'), got '{}'",
                saga.saga, step.name, step.action
            )));
        }

        if !is_valid_action(&step.compensate) {
            return Err(ParseError::ConfigInterpolation(format!(
                "saga '{}' step '{}': compensate must be 'METHOD /path', got '{}'",
                saga.saga, step.name, step.compensate
            )));
        }
    }

    Ok(())
}

fn is_valid_action(action: &str) -> bool {
    let parts: Vec<&str> = action.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return false;
    }
    let method = parts[0];
    let path = parts[1];
    matches!(method, "GET" | "POST" | "PUT" | "PATCH" | "DELETE") && path.starts_with('/')
}

/// Compute topological order of services (dependency-first).
/// Returns service names in startup order.
pub fn topological_order(config: &WorkspaceConfig) -> Vec<String> {
    let mut visited = std::collections::HashSet::new();
    let mut order = Vec::new();

    for name in config.services.keys() {
        if !visited.contains(name.as_str()) {
            topo_visit(config, name, &mut visited, &mut order);
        }
    }

    order
}

fn topo_visit<'a>(
    config: &'a WorkspaceConfig,
    node: &'a str,
    visited: &mut std::collections::HashSet<&'a str>,
    order: &mut Vec<String>,
) {
    visited.insert(node);

    if let Some(svc) = config.services.get(node) {
        for dep in &svc.depends_on {
            if !visited.contains(dep.as_str()) {
                topo_visit(config, dep, visited, order);
            }
        }
    }

    order.push(node.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_workspace_minimal() {
        let yaml = r#"
workspace: my-platform
services:
  api:
    path: services/api
    port: 3001
"#;
        let cfg = parse_workspace(yaml).unwrap();
        assert_eq!(cfg.workspace, "my-platform");
        assert_eq!(cfg.services.len(), 1);
        assert_eq!(cfg.services.get("api").unwrap().port, 3001);
    }

    #[test]
    fn parse_workspace_full() {
        let yaml = r#"
workspace: my-platform
services:
  users-api:
    path: services/users-api
    port: 3001
  orders-api:
    path: services/orders-api
    port: 3002
    depends_on: [users-api]
shared:
  cache:
    type: redis
    url: redis://localhost:6379
  auth:
    provider: jwt
    secret_env: JWT_SECRET
    expiry: 24h
"#;
        let cfg = parse_workspace(yaml).unwrap();
        assert_eq!(cfg.services.len(), 2);
        let orders = cfg.services.get("orders-api").unwrap();
        assert_eq!(orders.depends_on, vec!["users-api"]);
        assert!(cfg.shared.is_some());
    }

    #[test]
    fn parse_workspace_empty_name_fails() {
        let yaml = r#"
workspace: ""
services:
  api:
    path: services/api
"#;
        let err = parse_workspace(yaml).unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn parse_workspace_no_services_fails() {
        let yaml = r#"
workspace: test
services: {}
"#;
        let err = parse_workspace(yaml).unwrap_err();
        assert!(err.to_string().contains("at least one service"));
    }

    #[test]
    fn parse_workspace_unknown_dependency_fails() {
        let yaml = r#"
workspace: test
services:
  api:
    path: services/api
    depends_on: [nonexistent]
"#;
        let err = parse_workspace(yaml).unwrap_err();
        assert!(err.to_string().contains("unknown service 'nonexistent'"));
    }

    #[test]
    fn parse_workspace_self_dependency_fails() {
        let yaml = r#"
workspace: test
services:
  api:
    path: services/api
    depends_on: [api]
"#;
        let err = parse_workspace(yaml).unwrap_err();
        assert!(err.to_string().contains("cannot depend on itself"));
    }

    #[test]
    fn parse_workspace_circular_dependency_fails() {
        let yaml = r#"
workspace: test
services:
  a:
    path: services/a
    port: 3001
    depends_on: [b]
  b:
    path: services/b
    port: 3002
    depends_on: [a]
"#;
        let err = parse_workspace(yaml).unwrap_err();
        assert!(err.to_string().contains("circular"));
    }

    #[test]
    fn parse_workspace_duplicate_port_fails() {
        let yaml = r#"
workspace: test
services:
  a:
    path: services/a
    port: 3001
  b:
    path: services/b
    port: 3001
"#;
        let err = parse_workspace(yaml).unwrap_err();
        assert!(err.to_string().contains("same port 3001"));
    }

    #[test]
    fn parse_saga_minimal() {
        let yaml = r#"
saga: create_order
steps:
  - name: reserve
    service: inventory-api
    action: POST /v1/reservations
    compensate: DELETE /v1/reservations/:id
"#;
        let saga = parse_saga(yaml).unwrap();
        assert_eq!(saga.saga, "create_order");
        assert_eq!(saga.version, 1);
        assert_eq!(saga.steps.len(), 1);
    }

    #[test]
    fn parse_saga_full() {
        let yaml = r#"
saga: create_order
version: 2
steps:
  - name: reserve
    service: inventory-api
    action: POST /v1/reservations
    compensate: DELETE /v1/reservations/:id
    timeout_secs: 5
  - name: charge
    service: payments-api
    action: POST /v1/charges
    compensate: POST /v1/charges/:id/refund
    timeout_secs: 10
  - name: create_record
    service: orders-api
    action: POST /v1/orders
    compensate: DELETE /v1/orders/:id
"#;
        let saga = parse_saga(yaml).unwrap();
        assert_eq!(saga.version, 2);
        assert_eq!(saga.steps.len(), 3);
        assert_eq!(saga.steps[0].timeout_secs, 5);
        assert_eq!(saga.steps[2].timeout_secs, 30);
    }

    #[test]
    fn parse_saga_empty_name_fails() {
        let yaml = r#"
saga: ""
steps:
  - name: step
    service: svc
    action: POST /v1/x
    compensate: DELETE /v1/x/:id
"#;
        let err = parse_saga(yaml).unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn parse_saga_no_steps_fails() {
        let yaml = r#"
saga: test
steps: []
"#;
        let err = parse_saga(yaml).unwrap_err();
        assert!(err.to_string().contains("at least one step"));
    }

    #[test]
    fn parse_saga_duplicate_step_name_fails() {
        let yaml = r#"
saga: test
steps:
  - name: step1
    service: svc
    action: POST /v1/x
    compensate: DELETE /v1/x/:id
  - name: step1
    service: svc
    action: POST /v1/y
    compensate: DELETE /v1/y/:id
"#;
        let err = parse_saga(yaml).unwrap_err();
        assert!(err.to_string().contains("duplicate step name"));
    }

    #[test]
    fn parse_saga_invalid_action_fails() {
        let yaml = r#"
saga: test
steps:
  - name: step1
    service: svc
    action: invalid
    compensate: DELETE /v1/x/:id
"#;
        let err = parse_saga(yaml).unwrap_err();
        assert!(err.to_string().contains("must be 'METHOD /path'"));
    }

    #[test]
    fn topological_order_respects_deps() {
        let yaml = r#"
workspace: test
services:
  c:
    path: services/c
    port: 3003
    depends_on: [a, b]
  b:
    path: services/b
    port: 3002
    depends_on: [a]
  a:
    path: services/a
    port: 3001
"#;
        let cfg = parse_workspace(yaml).unwrap();
        let order = topological_order(&cfg);
        let a_pos = order.iter().position(|s| s == "a").unwrap();
        let b_pos = order.iter().position(|s| s == "b").unwrap();
        let c_pos = order.iter().position(|s| s == "c").unwrap();
        assert!(a_pos < b_pos);
        assert!(a_pos < c_pos);
        assert!(b_pos < c_pos);
    }

    #[test]
    fn topological_order_no_deps() {
        let yaml = r#"
workspace: test
services:
  a:
    path: services/a
    port: 3001
  b:
    path: services/b
    port: 3002
"#;
        let cfg = parse_workspace(yaml).unwrap();
        let order = topological_order(&cfg);
        assert_eq!(order.len(), 2);
    }
}
