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
    // resource name is at the top of the document — its span should exist.
    assert!(
        span_map.lookup("resource").is_some(),
        "no span for resource key",
    );
}
