//! Runs only when the `saphyr-spans` feature is on. Asserts the saphyr
//! parser produces the same ResourceDefinition as serde_yaml for every
//! valid fixture, and additionally produces a non-empty SpanMap.

#![cfg(feature = "saphyr-spans")]

#[test]
fn saphyr_matches_serde_yaml_on_users_archetype() {
    let yaml = include_str!("fixtures/valid/users_archetype.yaml");
    let serde_rd =
        shaperail_codegen::parser::parse_resource_str(yaml).expect("serde_yaml parse must succeed");
    let (saphyr_rd, span_map) = shaperail_codegen::parser_saphyr::parse_with_spans(yaml)
        .expect("saphyr parse must succeed");
    assert_eq!(
        serde_rd, saphyr_rd,
        "saphyr produced a different ResourceDefinition"
    );
    assert!(!span_map.is_empty(), "saphyr produced an empty SpanMap");

    // Root-level keys.
    let resource_span = span_map.lookup("resource").expect("no span for resource");
    let version_span = span_map.lookup("version").expect("no span for version");
    let schema_span = span_map.lookup("schema").expect("no span for schema");

    // Sanity: spans are 1-indexed and within the document.
    assert!(resource_span.line >= 1, "line should be 1-indexed");
    assert!(resource_span.col >= 1, "col should be 1-indexed");

    // resource: users -> the span of `users` (value), not of `resource:` (key).
    // The fixture's first line is `resource: users`; the value `users` starts
    // at column 11 (after `resource:` which is 9 chars, plus one space).
    // Don't pin the exact column; just assert it's past the colon, i.e. col > 9
    // ("resource:".len() == 9). This is the value-span vs. key-span check.
    assert!(
        resource_span.col > 9,
        "resource lookup should return value span (col > 9, past 'resource:'), got col={}",
        resource_span.col,
    );

    // version comes after resource in the document.
    assert!(
        version_span.line > resource_span.line,
        "version span line ({}) should be after resource span line ({})",
        version_span.line,
        resource_span.line,
    );

    // schema block comes after version.
    assert!(
        schema_span.line > version_span.line,
        "schema span line ({}) should be after version span line ({})",
        schema_span.line,
        version_span.line,
    );

    // Nested key. The fixture has a `schema:` block with `id:` field; verify
    // the recursion indexed it.
    assert!(
        span_map.lookup("schema.id").is_some(),
        "expected nested span for schema.id; SpanMap likely failed to recurse",
    );

    // Confirm the old __value convention is gone — nothing should live at
    // a __value-suffixed path.
    assert!(
        span_map.lookup("resource.__value").is_none(),
        "obsolete __value suffix still present; lookup contract should be value-first",
    );
}

#[test]
fn diagnostics_carry_spans_when_saphyr_is_used() {
    // Use a fixture that triggers a known diagnostic with a known field path.
    let yaml = r#"resource: ""
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
"#;
    let (rd, span_map) = shaperail_codegen::parser_saphyr::parse_with_spans(yaml).unwrap();
    let diags = shaperail_codegen::diagnostics::diagnose_resource_with_spans(&rd, &span_map);

    let sr001 = diags
        .iter()
        .find(|d| d.code == "SR001")
        .expect("expected SR001");
    let span = sr001.span.as_ref().expect("SR001 should carry a span");
    assert_eq!(span.line, 1, "expected line 1 for resource: at top of file");
}

#[test]
fn sr002_carries_span_for_version_field() {
    // version: 0 triggers SR002. field_path_for maps SR002 -> "version".
    let yaml = "resource: test\nversion: 0\nschema:\n  id: { type: uuid, primary: true, generated: true }\n";
    let (rd, span_map) = shaperail_codegen::parser_saphyr::parse_with_spans(yaml).unwrap();
    let diags = shaperail_codegen::diagnostics::diagnose_resource_with_spans(&rd, &span_map);
    let sr002 = diags
        .iter()
        .find(|d| d.code == "SR002")
        .expect("expected SR002");
    let span = sr002.span.as_ref().expect("SR002 should carry a span");
    assert_eq!(span.line, 2, "version: 0 is on line 2");
}

#[test]
fn sr003_carries_span_for_schema_container() {
    // Empty schema mapping triggers SR003. field_path_for maps SR003 -> "schema".
    let yaml = "resource: test\nversion: 1\nschema: {}\n";
    let (rd, span_map) = shaperail_codegen::parser_saphyr::parse_with_spans(yaml).unwrap();
    let diags = shaperail_codegen::diagnostics::diagnose_resource_with_spans(&rd, &span_map);
    let sr003 = diags
        .iter()
        .find(|d| d.code == "SR003")
        .expect("expected SR003");
    let span = sr003.span.as_ref().expect("SR003 should carry a span");
    assert_eq!(span.line, 3, "schema: {{}} is on line 3");
}

#[test]
fn sr004_carries_span_for_schema_container() {
    // Schema with no primary triggers SR004. field_path_for maps SR004 -> "schema".
    let yaml = "resource: test\nversion: 1\nschema:\n  id: { type: uuid }\n";
    let (rd, span_map) = shaperail_codegen::parser_saphyr::parse_with_spans(yaml).unwrap();
    let diags = shaperail_codegen::diagnostics::diagnose_resource_with_spans(&rd, &span_map);
    let sr004 = diags
        .iter()
        .find(|d| d.code == "SR004")
        .expect("expected SR004");
    let span = sr004.span.as_ref().expect("SR004 should carry a span");
    // Span points at the schema block (line >= 3).
    assert!(span.line >= 3, "expected span line >= 3, got {}", span.line);
}

#[test]
fn sr005_carries_span_for_schema_container() {
    // Schema with two primaries triggers SR005. field_path_for maps SR005 -> "schema".
    let yaml = "resource: test\nversion: 1\nschema:\n  id: { type: uuid, primary: true, generated: true }\n  alt: { type: uuid, primary: true, generated: true }\n";
    let (rd, span_map) = shaperail_codegen::parser_saphyr::parse_with_spans(yaml).unwrap();
    let diags = shaperail_codegen::diagnostics::diagnose_resource_with_spans(&rd, &span_map);
    let sr005 = diags
        .iter()
        .find(|d| d.code == "SR005")
        .expect("expected SR005");
    let span = sr005.span.as_ref().expect("SR005 should carry a span");
    assert!(span.line >= 3, "expected span line >= 3, got {}", span.line);
}

#[test]
fn span_field_survives_json_roundtrip() {
    // Regression guard for commit 3f0d3ff: shaperail-cli previously rebuilt
    // the JSON object by hand and silently dropped the optional `span` field.
    // This test mirrors the CLI's serialize path (serde_json::to_value(&d))
    // and asserts the new fields all survive — not just the originals.
    let yaml = r#"resource: ""
version: 1
schema:
  id: { type: uuid, primary: true, generated: true }
"#;
    let (rd, span_map) = shaperail_codegen::parser_saphyr::parse_with_spans(yaml).unwrap();
    let diags = shaperail_codegen::diagnostics::diagnose_resource_with_spans(&rd, &span_map);
    let sr001 = diags
        .iter()
        .find(|d| d.code == "SR001")
        .expect("expected SR001");

    let value = serde_json::to_value(sr001).expect("Diagnostic must serialize to JSON");
    let span = value
        .get("span")
        .expect("span field must be present in JSON output");
    assert!(span.is_object(), "span must be an object, got {span:?}");
    assert_eq!(span["line"].as_u64(), Some(1));
    assert!(span["col"].as_u64().unwrap_or(0) > 0);
    assert!(span["end_line"].as_u64().unwrap_or(0) >= 1);
    assert!(span["end_col"].as_u64().unwrap_or(0) >= 1);
    assert!(span["file"].is_string());

    let severity = value
        .get("severity")
        .and_then(|v| v.as_str())
        .expect("severity must be present and a string");
    assert_eq!(severity, "error");

    let doc_url = value
        .get("doc_url")
        .and_then(|v| v.as_str())
        .expect("doc_url must be present and a string");
    assert_eq!(doc_url, "https://shaperail.io/errors/SR001.html");
}

#[test]
fn span_is_omitted_from_json_when_per_field_code_has_no_path() {
    // SR010 (enum-without-values) is a per-field code; field_path_for returns
    // None for it today, so its span stays None and serde's
    // skip_serializing_if = "Option::is_none" omits the field entirely.
    let yaml = "resource: test\nversion: 1\nschema:\n  id: { type: uuid, primary: true, generated: true }\n  status: { type: enum }\n";
    let (rd, span_map) = shaperail_codegen::parser_saphyr::parse_with_spans(yaml).unwrap();
    let diags = shaperail_codegen::diagnostics::diagnose_resource_with_spans(&rd, &span_map);
    let sr010 = diags
        .iter()
        .find(|d| d.code == "SR010")
        .expect("expected SR010");
    assert!(
        sr010.span.is_none(),
        "per-field codes should not yet carry spans (v0.14.x scope is root-level codes only)",
    );
    let value = serde_json::to_value(sr010).expect("serialize");
    assert!(
        value.get("span").is_none() || value["span"].is_null(),
        "span key should be omitted from JSON when None, got {value}",
    );
}
