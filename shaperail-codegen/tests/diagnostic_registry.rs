use shaperail_codegen::diagnostics::registry::{Severity, REGISTRY};

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
