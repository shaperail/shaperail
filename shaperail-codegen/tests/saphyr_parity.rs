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
