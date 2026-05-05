//! Asserts every code in the diagnostic registry has a corresponding
//! docs/errors/<code>.md page. CI guard against guide drift.

use shaperail_codegen::diagnostics::registry::REGISTRY;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn every_registry_code_has_a_doc_page() {
    let root = repo_root();
    let mut missing = Vec::new();
    for entry in REGISTRY {
        let page = root
            .join("docs")
            .join("errors")
            .join(format!("{}.md", entry.code));
        if !page.exists() {
            missing.push(entry.code);
        }
    }
    assert!(
        missing.is_empty(),
        "missing docs/errors/<code>.md for: {:?}",
        missing,
    );
}

#[test]
fn doc_pages_have_jekyll_front_matter() {
    let root = repo_root();
    for entry in REGISTRY {
        let page = root
            .join("docs")
            .join("errors")
            .join(format!("{}.md", entry.code));
        if !page.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&page).expect("readable");
        assert!(
            content.starts_with("---\n"),
            "{} missing Jekyll front matter",
            entry.code,
        );
        assert!(
            content.contains(&format!("title: {}", entry.code)),
            "{} front matter missing title: {}",
            entry.code,
            entry.code,
        );
    }
}
