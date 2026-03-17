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

        // Check for upload endpoints → storage feature
        if let Some(endpoints) = &resource.endpoints {
            for (action, ep) in endpoints {
                if ep.upload.is_some() {
                    required.push(RequiredFeature {
                        feature: "storage",
                        reason: format!("resource '{res}' endpoint '{action}' uses upload"),
                        enable_hint:
                            "Add to Cargo.toml: shaperail-runtime = { features = [\"storage\"] }"
                                .into(),
                    });
                }

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
    fn upload_requires_storage_feature() {
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
        assert!(required.iter().any(|f| f.feature == "storage"));
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
}
