use std::path::Path;

/// Export OpenAPI 3.1 spec to stdout or a file.
pub fn run_openapi(output: Option<&Path>) -> i32 {
    let resources = match super::load_all_resources() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    let config = match super::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    let spec = shaperail_codegen::openapi::generate(&config, &resources);

    match output {
        Some(path) => {
            let content = if path.extension().is_some_and(|e| e == "yaml" || e == "yml") {
                match shaperail_codegen::openapi::to_yaml(&spec) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error serializing YAML: {e}");
                        return 1;
                    }
                }
            } else {
                match shaperail_codegen::openapi::to_json(&spec) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error serializing JSON: {e}");
                        return 1;
                    }
                }
            };

            if let Err(e) = std::fs::write(path, content) {
                eprintln!("Error writing {}: {e}", path.display());
                return 1;
            }
            println!("OpenAPI spec written to {}", path.display());
        }
        None => match shaperail_codegen::openapi::to_json(&spec) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("Error serializing JSON: {e}");
                return 1;
            }
        },
    }

    0
}

/// Generate a TypeScript client SDK from the OpenAPI spec.
pub fn run_sdk(lang: &str, output: Option<&Path>) -> i32 {
    if lang != "ts" && lang != "typescript" {
        eprintln!("Unsupported SDK language: '{lang}'. Supported: ts");
        return 1;
    }

    let resources = match super::load_all_resources() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    let config = match super::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    let output_dir = output.unwrap_or_else(|| Path::new("sdk"));
    if let Err(e) = std::fs::create_dir_all(output_dir) {
        eprintln!("Error creating SDK directory: {e}");
        return 1;
    }

    // Generate OpenAPI spec first, then derive TS types from it
    let spec = shaperail_codegen::openapi::generate(&config, &resources);
    let files = shaperail_codegen::typescript::generate_from_spec(&spec);

    for (filename, content) in &files {
        let file_path = output_dir.join(filename);
        if let Err(e) = std::fs::write(&file_path, content) {
            eprintln!("Error writing {}: {e}", file_path.display());
            return 1;
        }
        println!("Generated {}", file_path.display());
    }

    // Also write the OpenAPI spec JSON for use with openapi-typescript
    let spec_path = output_dir.join("openapi.json");
    match shaperail_codegen::openapi::to_json(&spec) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&spec_path, json) {
                eprintln!("Error writing {}: {e}", spec_path.display());
                return 1;
            }
            println!("Generated {}", spec_path.display());
        }
        Err(e) => {
            eprintln!("Error serializing spec: {e}");
            return 1;
        }
    }

    println!("TypeScript SDK generated in {}", output_dir.display());
    println!(
        "Tip: Run `npx openapi-typescript sdk/openapi.json -o sdk/api.ts` for full client types"
    );
    0
}

/// Export JSON Schema for resource YAML files to stdout or a file.
pub fn run_json_schema(output: Option<&Path>) -> i32 {
    let schema = shaperail_codegen::json_schema::render_json_schema();

    match output {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &schema) {
                eprintln!("Error writing {}: {e}", path.display());
                return 1;
            }
            println!("JSON Schema written to {}", path.display());
        }
        None => println!("{schema}"),
    }

    0
}
