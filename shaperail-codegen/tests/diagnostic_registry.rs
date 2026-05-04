use shaperail_codegen::diagnostics::registry::{Severity, REGISTRY};
use shaperail_codegen::diagnostics::{Diagnostic, Span};
use std::path::PathBuf;

#[test]
fn registry_has_every_code_emitted_today() {
    let codes: Vec<&'static str> = REGISTRY.iter().map(|e| e.code).collect();

    // Sanity: 42 codes today (all codes emitted by shaperail-codegen/src/diagnostics.rs).
    // SR000 (YAML parse error, emitted in shaperail-cli) and SR100 (legacy INT/BIGINT drift
    // warning, also in shaperail-cli) are NOT in this registry — they live in cli's check.rs.
    // If you add or remove a code, update this number and the registry in the same commit.
    assert_eq!(
        codes.len(),
        42,
        "registry size drifted; update both registry and this assertion"
    );

    // Every code should be present and unique.
    let mut sorted = codes.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), codes.len(), "duplicate code in registry");

    // Spot-check known codes from the actual emission sites.
    for expected in ["SR001", "SR035", "SR080", "SR075"] {
        assert!(codes.contains(&expected), "missing {expected} in registry");
    }
}

#[test]
fn every_registry_entry_has_summary_and_severity() {
    for e in REGISTRY {
        assert!(!e.summary.is_empty(), "{} has empty summary", e.code);
        assert!(matches!(
            e.severity,
            Severity::Error | Severity::Warning | Severity::Info
        ));
    }
}

#[test]
fn lookup_finds_known_code() {
    use shaperail_codegen::diagnostics::registry::lookup;
    let entry = lookup("SR001").expect("SR001 must be in registry");
    assert_eq!(entry.code, "SR001");
    assert!(!entry.summary.is_empty());
}

#[test]
fn lookup_returns_none_for_unknown_code() {
    use shaperail_codegen::diagnostics::registry::lookup;
    assert!(lookup("SR999").is_none());
    assert!(lookup("").is_none());
}

#[test]
fn diagnostic_carries_optional_span() {
    let d = Diagnostic::error(
        "SR001",
        "empty resource name",
        "set `resource:` to a snake_case plural",
        "resource: users",
    );
    assert_eq!(d.code, "SR001");
    assert!(d.span.is_none(), "default constructor leaves span None");

    let d = d.with_span(Span {
        file: PathBuf::from("users.yaml"),
        line: 3,
        col: 1,
        end_line: 3,
        end_col: 1,
    });
    assert!(d.span.is_some());

    // doc_url comes from the registry, populated by `error()` constructor.
    assert_eq!(
        d.doc_url.as_deref(),
        Some("https://shaperail.io/errors/SR001.html"),
    );
}

#[test]
fn diagnostic_serializes_new_fields_to_json() {
    let d =
        Diagnostic::error("SR001", "empty resource name", "fix...", "example...").with_span(Span {
            file: PathBuf::from("u.yaml"),
            line: 2,
            col: 4,
            end_line: 2,
            end_col: 9,
        });

    let s = serde_json::to_string(&d).unwrap();
    assert!(s.contains("\"line\":2"), "json missing line: {}", s);
    assert!(s.contains("\"col\":4"));
    assert!(s.contains("\"severity\":\"error\""));
    assert!(s.contains("\"doc_url\""));
}

#[test]
fn diagnostic_without_span_omits_span_in_json() {
    let d = Diagnostic::error("SR001", "x", "y", "z");
    let v: serde_json::Value = serde_json::to_value(&d).unwrap();
    assert!(
        v.get("span").is_none_or(|s| s.is_null()),
        "span should be null/absent when not set: {}",
        v,
    );
}
