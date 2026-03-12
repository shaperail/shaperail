use shaperail_codegen::parser::parse_resource;
use shaperail_codegen::validator::validate_resource;

// ── Valid resource snapshots (5) ──────────────────────────────────────────

#[test]
fn snapshot_valid_users() {
    let yaml = include_str!("fixtures/valid/users.yaml");
    let rd = parse_resource(yaml).unwrap();
    let errors = validate_resource(&rd);
    assert!(errors.is_empty());
    insta::assert_yaml_snapshot!("valid_users", rd);
}

#[test]
fn snapshot_valid_tags() {
    let yaml = include_str!("fixtures/valid/tags.yaml");
    let rd = parse_resource(yaml).unwrap();
    let errors = validate_resource(&rd);
    assert!(errors.is_empty());
    insta::assert_yaml_snapshot!("valid_tags", rd);
}

#[test]
fn snapshot_valid_orders() {
    let yaml = include_str!("fixtures/valid/orders.yaml");
    let rd = parse_resource(yaml).unwrap();
    let errors = validate_resource(&rd);
    assert!(errors.is_empty());
    insta::assert_yaml_snapshot!("valid_orders", rd);
}

#[test]
fn snapshot_valid_organizations() {
    let yaml = include_str!("fixtures/valid/organizations.yaml");
    let rd = parse_resource(yaml).unwrap();
    let errors = validate_resource(&rd);
    assert!(errors.is_empty());
    insta::assert_yaml_snapshot!("valid_organizations", rd);
}

#[test]
fn snapshot_valid_minimal() {
    let yaml = include_str!("fixtures/valid/minimal.yaml");
    let rd = parse_resource(yaml).unwrap();
    let errors = validate_resource(&rd);
    assert!(errors.is_empty());
    insta::assert_yaml_snapshot!("valid_minimal", rd);
}

// ── Invalid resource snapshots (10) ───────────────────────────────────────

#[test]
fn snapshot_invalid_missing_resource() {
    let yaml = include_str!("fixtures/invalid/missing_resource.yaml");
    let err = parse_resource(yaml).unwrap_err();
    insta::assert_snapshot!("invalid_missing_resource", err.to_string());
}

#[test]
fn snapshot_invalid_missing_version() {
    let yaml = include_str!("fixtures/invalid/missing_version.yaml");
    let err = parse_resource(yaml).unwrap_err();
    insta::assert_snapshot!("invalid_missing_version", err.to_string());
}

#[test]
fn snapshot_invalid_missing_schema() {
    let yaml = include_str!("fixtures/invalid/missing_schema.yaml");
    let err = parse_resource(yaml).unwrap_err();
    insta::assert_snapshot!("invalid_missing_schema", err.to_string());
}

#[test]
fn snapshot_invalid_enum_no_values() {
    let yaml = include_str!("fixtures/invalid/enum_no_values.yaml");
    let rd = parse_resource(yaml).unwrap();
    let errors = validate_resource(&rd);
    let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
    insta::assert_yaml_snapshot!("invalid_enum_no_values", messages);
}

#[test]
fn snapshot_invalid_ref_not_uuid() {
    let yaml = include_str!("fixtures/invalid/ref_not_uuid.yaml");
    let rd = parse_resource(yaml).unwrap();
    let errors = validate_resource(&rd);
    let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
    insta::assert_yaml_snapshot!("invalid_ref_not_uuid", messages);
}

#[test]
fn snapshot_invalid_no_primary_key() {
    let yaml = include_str!("fixtures/invalid/no_primary_key.yaml");
    let rd = parse_resource(yaml).unwrap();
    let errors = validate_resource(&rd);
    let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
    insta::assert_yaml_snapshot!("invalid_no_primary_key", messages);
}

#[test]
fn snapshot_invalid_soft_delete_no_updated_at() {
    let yaml = include_str!("fixtures/invalid/soft_delete_no_updated_at.yaml");
    let rd = parse_resource(yaml).unwrap();
    let errors = validate_resource(&rd);
    let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
    insta::assert_yaml_snapshot!("invalid_soft_delete_no_updated_at", messages);
}

#[test]
fn snapshot_invalid_input_field_missing() {
    let yaml = include_str!("fixtures/invalid/input_field_missing.yaml");
    let rd = parse_resource(yaml).unwrap();
    let errors = validate_resource(&rd);
    let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
    insta::assert_yaml_snapshot!("invalid_input_field_missing", messages);
}

#[test]
fn snapshot_invalid_belongs_to_no_key() {
    let yaml = include_str!("fixtures/invalid/belongs_to_no_key.yaml");
    let rd = parse_resource(yaml).unwrap();
    let errors = validate_resource(&rd);
    let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
    insta::assert_yaml_snapshot!("invalid_belongs_to_no_key", messages);
}

#[test]
fn snapshot_invalid_yaml_syntax() {
    let yaml = include_str!("fixtures/invalid/invalid_yaml_syntax.yaml");
    let err = parse_resource(yaml).unwrap_err();
    insta::assert_snapshot!("invalid_yaml_syntax", err.to_string());
}
