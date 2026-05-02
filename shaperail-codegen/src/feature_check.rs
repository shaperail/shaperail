use shaperail_core::ResourceDefinition;

/// A feature that a resource requires but may not be enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredFeature {
    /// The Cargo feature name (e.g., "storage").
    pub feature: &'static str,
    /// Why this feature is needed.
    pub reason: String,
    /// How to enable it.
    pub enable_hint: String,
}

/// Check what Cargo features a set of resources require.
///
/// Returns a list of features that resources use. The caller can then
/// compare against the project's enabled features to detect mismatches early
/// (before the compile-time error).
pub fn check_required_features(resources: &[ResourceDefinition]) -> Vec<RequiredFeature> {
    let mut required = Vec::new();

    for resource in resources {
        let res = &resource.resource;

        // Upload endpoints are supported by the runtime without an extra Cargo
        // feature. Keep validating the resource shape elsewhere, but do not
        // emit a misleading feature warning here.
        if let Some(endpoints) = &resource.endpoints {
            for (action, ep) in endpoints {
                // Check for WASM controllers → wasm-plugins feature
                if let Some(controller) = &ep.controller {
                    if controller.has_wasm_before() || controller.has_wasm_after() {
                        required.push(RequiredFeature {
                            feature: "wasm-plugins",
                            reason: format!(
                                "resource '{res}' endpoint '{action}' uses WASM controller"
                            ),
                            enable_hint: "Add to Cargo.toml: shaperail-runtime = { features = [\"wasm-plugins\"] }".into(),
                        });
                    }
                }
            }
        }

        // Check for tenant_key → implicit (always available, no feature needed)
        // Check for multi-db → multi-db feature
        if resource.db.is_some() {
            required.push(RequiredFeature {
                feature: "multi-db",
                reason: format!("resource '{res}' uses 'db' key for multi-database routing"),
                enable_hint: "Add to Cargo.toml: shaperail-runtime = { features = [\"multi-db\"] }"
                    .into(),
            });
        }
    }

    // Deduplicate by feature name
    required.sort_by(|a, b| a.feature.cmp(b.feature));
    required.dedup_by(|a, b| a.feature == b.feature);
    required
}

/// Format required features as user-facing warnings.
pub fn format_feature_warnings(required: &[RequiredFeature]) -> String {
    if required.is_empty() {
        return String::new();
    }

    let mut out = String::from("Feature requirements detected:\n");
    for feat in required {
        out.push_str(&format!(
            "  - feature '{}' needed: {}\n    {}\n",
            feat.feature, feat.reason, feat.enable_hint
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_resource;

    #[test]
    fn no_features_for_basic_resource() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  list:
    auth: public
    pagination: cursor
"#;
        let rd = parse_resource(yaml).unwrap();
        let required = check_required_features(&[rd]);
        assert!(required.is_empty());
    }

    #[test]
    fn upload_requires_no_extra_feature() {
        let yaml = r#"
resource: assets
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  file: { type: file, required: true }
endpoints:
  upload:
    method: POST
    path: /assets/upload
    input: [file]
    upload:
      field: file
      storage: s3
      max_size: 5mb
"#;
        let rd = parse_resource(yaml).unwrap();
        let required = check_required_features(&[rd]);
        assert!(!required.iter().any(|f| f.feature == "storage"));
    }

    #[test]
    fn wasm_controller_requires_wasm_plugins_feature() {
        let yaml = r#"
resource: items
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  create:
    method: POST
    path: /items
    input: [name]
    controller: { before: "wasm:./plugins/validator.wasm" }
"#;
        let rd = parse_resource(yaml).unwrap();
        let required = check_required_features(&[rd]);
        assert!(required.iter().any(|f| f.feature == "wasm-plugins"));
    }

    #[test]
    fn multi_db_requires_feature() {
        let yaml = r#"
resource: events
version: 1
db: analytics
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let required = check_required_features(&[rd]);
        assert!(required.iter().any(|f| f.feature == "multi-db"));
    }

    #[test]
    fn features_are_deduplicated_across_resources() {
        // Two resources both using WASM → only one wasm-plugins entry
        let yaml1 = r#"
resource: a
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  create:
    method: POST
    path: /a
    input: [name]
    controller: { before: "wasm:./plugins/validator_a.wasm" }
"#;
        let yaml2 = r#"
resource: b
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  create:
    method: POST
    path: /b
    input: [name]
    controller: { before: "wasm:./plugins/validator_b.wasm" }
"#;
        let rd1 = parse_resource(yaml1).unwrap();
        let rd2 = parse_resource(yaml2).unwrap();
        let required = check_required_features(&[rd1, rd2]);
        let wasm_count = required
            .iter()
            .filter(|f| f.feature == "wasm-plugins")
            .count();
        assert_eq!(wasm_count, 1, "WASM feature should be deduplicated");
    }

    #[test]
    fn wasm_after_controller_requires_wasm_plugins_feature() {
        let yaml = r#"
resource: items
version: 1
schema:
  id:   { type: uuid, primary: true, generated: true }
  name: { type: string, required: true }
endpoints:
  create:
    method: POST
    path: /items
    input: [name]
    controller: { after: "wasm:./plugins/enricher.wasm" }
"#;
        let rd = parse_resource(yaml).unwrap();
        let required = check_required_features(&[rd]);
        assert!(
            required.iter().any(|f| f.feature == "wasm-plugins"),
            "wasm-plugins should be required for wasm after controller"
        );
    }

    #[test]
    fn format_feature_warnings_no_features() {
        let s = format_feature_warnings(&[]);
        assert!(s.is_empty());
    }

    #[test]
    fn format_feature_warnings_with_features() {
        let yaml = r#"
resource: events
version: 1
db: analytics
schema:
  id: { type: uuid, primary: true, generated: true }
"#;
        let rd = parse_resource(yaml).unwrap();
        let required = check_required_features(&[rd]);
        let s = format_feature_warnings(&required);
        assert!(s.contains("multi-db"), "Expected multi-db in warnings");
        assert!(s.contains("Feature requirements"), "Expected header");
    }
}
