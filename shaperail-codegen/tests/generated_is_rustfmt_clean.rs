//! Asserts that the rust codegen produces files that are idempotent under
//! `rustfmt --check`. Skipped when rustfmt is not on PATH.

use std::process::Command;

#[test]
fn generated_files_are_rustfmt_clean() {
    // Skip gracefully when rustfmt is not available.
    if Command::new("rustfmt")
        .arg("--version")
        .status()
        .map(|s| !s.success())
        .unwrap_or(true)
    {
        eprintln!("Skipping: rustfmt not on PATH");
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let out_dir = tmp.path().join("generated");
    std::fs::create_dir_all(&out_dir).unwrap();

    // Build two minimal resources — one with filters/sort (exercises those code
    // paths) and one without (exercises the let _ = ... suppression paths).
    let yaml_with_filters = "resource: orders\nversion: 1\nschema:\n  id: {type: uuid, primary: true, generated: true}\n  status: {type: string, required: true}\nendpoints:\n  list:\n    auth: public\n    filters: [status]\n    sort: [status]\n    pagination: offset\n";
    let yaml_no_filters = "resource: tags\nversion: 1\nschema:\n  id: {type: uuid, primary: true, generated: true}\n  name: {type: string, required: true}\nendpoints:\n  list:\n    auth: public\n";

    let resources: Vec<_> = [yaml_with_filters, yaml_no_filters]
        .iter()
        .map(|yaml| shaperail_codegen::parser::parse_resource(yaml).expect("parse"))
        .collect();

    let project = shaperail_codegen::rust::generate_project(&resources).expect("generate_project");

    // Write each module to the temp dir and run rustfmt_in_place on it.
    for module in &project.modules {
        let path = out_dir.join(&module.file_name);
        std::fs::write(&path, &module.contents).expect("write module");
        shaperail_codegen::rust::rustfmt_in_place(&path);
    }

    // Also write mod.rs (the registry module — covers the pub mod resources
    // aggregator added in v0.14). Per-module loop above doesn't include it.
    let mod_rs_path = out_dir.join("mod.rs");
    std::fs::write(&mod_rs_path, &project.mod_rs).expect("write mod.rs");
    shaperail_codegen::rust::rustfmt_in_place(&mod_rs_path);

    // Now assert every .rs file in out_dir passes `rustfmt --check`.
    for entry in std::fs::read_dir(&out_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let status = Command::new("rustfmt")
            .arg("--check")
            .arg("--edition")
            .arg("2021")
            .arg(&path)
            .status()
            .expect("rustfmt --check");
        assert!(
            status.success(),
            "{} is not rustfmt-clean after generation",
            path.display()
        );
    }
}
