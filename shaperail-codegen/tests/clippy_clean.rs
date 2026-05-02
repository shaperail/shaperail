//! Regression tests asserting that the generated Rust code for resources without
//! filters or sort fields does not leave those parameters unused.

#[test]
fn find_all_body_binds_filters_when_no_filters_declared() {
    // A resource with a list endpoint but no `filters:` declared.
    let yaml = "resource: things\nversion: 1\nschema:\n  id: {type: uuid, primary: true, generated: true}\n  name: {type: string, required: true}\nendpoints:\n  list:\n    auth: public\n";
    let rd = shaperail_codegen::parser::parse_resource(yaml).unwrap();
    let project = shaperail_codegen::rust::generate_project(&[rd]).unwrap();
    let module = project
        .modules
        .iter()
        .find(|m| m.file_name == "things.rs")
        .unwrap();

    // The generated body must either read `filters` via parse_filter or suppress
    // the unused-variable warning with `let _ = filters;`.
    assert!(
        module.contents.contains("let _ = filters;")
            || module.contents.contains("parse_filter(filters"),
        "generated find_all body must bind `filters` to avoid unused-variable warning:\n{}",
        module.contents
    );
}

#[test]
fn find_all_body_binds_sort_when_no_sort_declared() {
    // A resource with a list endpoint but no `sort:` declared.
    let yaml = "resource: widgets\nversion: 1\nschema:\n  id: {type: uuid, primary: true, generated: true}\n  name: {type: string, required: true}\nendpoints:\n  list:\n    auth: public\n";
    let rd = shaperail_codegen::parser::parse_resource(yaml).unwrap();
    let project = shaperail_codegen::rust::generate_project(&[rd]).unwrap();
    let module = project
        .modules
        .iter()
        .find(|m| m.file_name == "widgets.rs")
        .unwrap();

    // The generated body must either use `sort` via sort_field_at or suppress
    // the unused-variable warning with `let _ = sort;`.
    assert!(
        module.contents.contains("let _ = sort;")
            || module.contents.contains("sort_field_at(sort,"),
        "generated find_all body must bind `sort` to avoid unused-variable warning:\n{}",
        module.contents
    );
}
