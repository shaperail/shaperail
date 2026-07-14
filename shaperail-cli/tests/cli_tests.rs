use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn shaperail() -> Command {
    cargo_bin_cmd!("shaperail")
}

/// Returns the workspace root directory (where resources/ lives).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn collect_example_resources(directory: &std::path::Path, resources: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_example_resources(&path, resources);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "yaml")
            && path
                .parent()
                .and_then(std::path::Path::file_name)
                .is_some_and(|parent| parent == "resources")
        {
            resources.push(path);
        }
    }
}

/// Run the shaperail CLI with the given arguments from the workspace root and
/// return the raw Output so tests can inspect stdout, stderr, and status.
fn run_cli(args: &[&str]) -> std::process::Output {
    let root = workspace_root();
    let mut cmd = cargo_bin_cmd!("shaperail");
    cmd.args(args).current_dir(&root);
    cmd.output().expect("failed to run shaperail")
}

// --- Help output tests ---

#[test]
fn help_shows_all_commands() {
    shaperail()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("generate"))
        .stdout(predicate::str::contains("serve"))
        .stdout(predicate::str::contains("build"))
        .stdout(predicate::str::contains("validate"))
        .stdout(predicate::str::contains("test"))
        .stdout(predicate::str::contains("migrate"))
        .stdout(predicate::str::contains("seed"))
        .stdout(predicate::str::contains("export"))
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("routes"))
        .stdout(predicate::str::contains("jobs:status"));
}

#[test]
fn version_flag() {
    shaperail().arg("--version").assert().success();
}

#[test]
fn init_help() {
    shaperail()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Scaffold"));
}

#[test]
fn generate_help() {
    shaperail()
        .args(["generate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("codegen"));
}

#[test]
fn serve_help() {
    shaperail()
        .args(["serve", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dev server"))
        .stdout(predicate::str::contains("--check"));
}

#[test]
fn build_help() {
    shaperail()
        .args(["build", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("release binary"));
}

#[test]
fn validate_help() {
    shaperail()
        .args(["validate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Validate"));
}

#[test]
fn test_help() {
    shaperail()
        .args(["test", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tests"));
}

#[test]
fn migrate_help() {
    shaperail()
        .args(["migrate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("migration"));
}

#[test]
fn seed_help() {
    shaperail()
        .args(["seed", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fixture"));
}

#[test]
fn export_help() {
    shaperail()
        .args(["export", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("openapi"))
        .stdout(predicate::str::contains("sdk"));
}

#[test]
fn doctor_help() {
    shaperail()
        .args(["doctor", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dependencies"));
}

#[test]
fn routes_help() {
    shaperail()
        .args(["routes", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("routes"));
}

#[test]
fn jobs_status_help() {
    shaperail()
        .args(["jobs:status", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("job queue"))
        .stdout(predicate::str::contains("[JOB_ID]"));
}

// --- Init tests ---

#[test]
fn init_creates_project_structure() {
    let tmp = TempDir::new().unwrap();
    let project_name = "test-project";
    let project_dir = tmp.path().join(project_name);

    shaperail()
        .args(["init", project_name])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Created Shaperail project"))
        .stdout(predicate::str::contains("http://localhost:3000/docs"))
        .stdout(predicate::str::contains(
            "http://localhost:3000/openapi.json",
        ));

    // Check directory structure
    assert!(project_dir.join("shaperail.config.yaml").exists());
    assert!(project_dir.join("README.md").exists());
    assert!(project_dir.join("Cargo.toml").exists());
    assert!(project_dir.join("src/main.rs").exists());
    assert!(project_dir.join("resources").is_dir());
    assert!(project_dir.join("migrations").is_dir());
    assert!(project_dir.join("controllers").is_dir());
    assert!(project_dir.join("seeds").is_dir());
    assert!(project_dir.join("tests").is_dir());
    assert!(project_dir.join("channels").is_dir());
    assert!(project_dir.join("generated").is_dir());
    assert!(project_dir.join(".env").exists());
    assert!(project_dir.join(".gitignore").exists());
    assert!(project_dir.join("docker-compose.yml").exists());
    assert!(project_dir.join("resources/posts.yaml").exists());
    assert!(project_dir.join("resources/posts.controller.rs").exists());
    assert!(project_dir
        .join("migrations/0001_create_posts.sql")
        .exists());

    // Verify config content
    let config = std::fs::read_to_string(project_dir.join("shaperail.config.yaml")).unwrap();
    assert!(config.contains("project: test-project"));
    assert!(config.contains("port: 3000"));
    assert!(config.contains("# proxy:"));
    assert!(config.contains("trusted_proxies: [127.0.0.1/32]"));

    let readme = std::fs::read_to_string(project_dir.join("README.md")).unwrap();
    assert!(readme.contains("docker compose up -d"));
    assert!(readme.contains("http://localhost:3000/docs"));
    assert!(readme.contains("No manual database creation is required"));

    let main_rs = std::fs::read_to_string(project_dir.join("src/main.rs")).unwrap();
    assert!(main_rs.contains(r#"route("/openapi.json""#));
    assert!(main_rs.contains(r#"route("/docs""#));
    assert!(
        main_rs.contains("AppState::new(pool.clone(), resources.clone(), config.proxy.as_ref())")
    );

    let posts = std::fs::read_to_string(project_dir.join("resources/posts.yaml")).unwrap();
    assert!(posts.contains("created_by: { type: string, required: true }"));
    assert!(posts.contains("input: [title, body, published]"));
    assert!(posts.contains("controller: { before: set_created_by }"));
    assert!(!posts.contains("author_id"));

    let controller =
        std::fs::read_to_string(project_dir.join("resources/posts.controller.rs")).unwrap();
    assert!(controller.contains("user.sub"));
    assert!(controller.contains(r#".insert("created_by".to_string()"#));
}

#[test]
fn init_fails_if_dir_exists() {
    let tmp = TempDir::new().unwrap();
    let project_name = "existing";
    std::fs::create_dir(tmp.path().join(project_name)).unwrap();

    shaperail()
        .args(["init", project_name])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

// --- Validate tests ---

#[test]
fn validate_valid_resource() {
    let root = workspace_root();
    shaperail()
        .args(["validate", "resources/users.yaml"])
        .current_dir(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[test]
fn validate_nonexistent_file() {
    shaperail()
        .args(["validate", "nonexistent.yaml"])
        .assert()
        .failure();
}

#[test]
fn validate_invalid_yaml() {
    let tmp = TempDir::new().unwrap();
    let bad_file = tmp.path().join("bad.yaml");
    std::fs::write(&bad_file, "not: [valid: yaml: here").unwrap();

    shaperail()
        .args(["validate", bad_file.to_str().unwrap()])
        .assert()
        .failure();
}

// --- Doctor test ---

#[test]
fn doctor_runs() {
    shaperail()
        .args(["doctor"])
        .assert()
        .stdout(predicate::str::contains("Shaperail Doctor"));
}

// --- Routes test (requires resources/ dir) ---

#[test]
fn routes_shows_endpoints() {
    let root = workspace_root();
    shaperail()
        .args(["routes"])
        .current_dir(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("METHOD"))
        .stdout(predicate::str::contains("/v1/users"));
}

// --- Generate test ---

#[test]
fn generate_produces_files() {
    let tmp = TempDir::new().unwrap();

    // Set up a mini project with a resource file
    std::fs::create_dir(tmp.path().join("resources")).unwrap();
    std::fs::copy(
        workspace_root().join("resources/users.yaml"),
        tmp.path().join("resources/users.yaml"),
    )
    .unwrap();

    shaperail()
        .args(["generate"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Generated"));

    assert!(tmp.path().join("generated/users.rs").exists());
    assert!(tmp.path().join("generated/mod.rs").exists());
}

// --- Export tests ---

/// Set up a minimal project directory with config + resource files.
fn setup_project_dir(tmp: &TempDir) {
    let config = "project: test-app\nport: 3000\n";
    std::fs::write(tmp.path().join("shaperail.config.yaml"), config).unwrap();
    std::fs::create_dir(tmp.path().join("resources")).unwrap();
    std::fs::copy(
        workspace_root().join("resources/users.yaml"),
        tmp.path().join("resources/users.yaml"),
    )
    .unwrap();
}

#[test]
fn export_openapi_to_stdout() {
    let tmp = TempDir::new().unwrap();
    setup_project_dir(&tmp);

    shaperail()
        .args(["export", "openapi"])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("openapi"))
        .stdout(predicate::str::contains("3.1.0"));
}

#[test]
fn export_openapi_to_file() {
    let tmp = TempDir::new().unwrap();
    setup_project_dir(&tmp);
    let output = tmp.path().join("spec.json");

    shaperail()
        .args(["export", "openapi", "--output", output.to_str().unwrap()])
        .current_dir(tmp.path())
        .assert()
        .success();

    let content = std::fs::read_to_string(&output).unwrap();
    assert!(content.contains("openapi"));
    assert!(content.contains("3.1.0"));
}

#[test]
fn export_sdk_typescript() {
    let tmp = TempDir::new().unwrap();
    setup_project_dir(&tmp);
    let output = tmp.path().join("sdk");

    shaperail()
        .args([
            "export",
            "sdk",
            "--lang",
            "ts",
            "--output",
            output.to_str().unwrap(),
        ])
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("TypeScript SDK generated"));

    assert!(output.join("users.ts").exists());
    assert!(output.join("index.ts").exists());
    assert!(output.join("openapi.json").exists());
}

#[test]
fn export_sdk_unsupported_lang() {
    let root = workspace_root();
    shaperail()
        .args(["export", "sdk", "--lang", "python"])
        .current_dir(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unsupported SDK language"));
}

// --- End-to-end: init creates a valid project ---

#[test]
fn init_generates_valid_config() {
    let tmp = TempDir::new().unwrap();
    shaperail()
        .args(["init", "e2e-test"])
        .current_dir(tmp.path())
        .assert()
        .success();

    // Validate the generated resource file
    let resource_path = tmp.path().join("e2e-test/resources/posts.yaml");
    shaperail()
        .args(["validate", resource_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[test]
fn checked_in_example_resources_parse_and_validate() {
    let mut resources = Vec::new();
    collect_example_resources(&workspace_root().join("examples"), &mut resources);
    resources.sort();
    assert!(!resources.is_empty(), "no example resources found");

    let mut failures = Vec::new();
    for path in resources {
        match shaperail_codegen::parser::parse_resource_file(&path) {
            Ok(resource) => {
                for error in shaperail_codegen::validator::validate_resource(&resource) {
                    failures.push(format!("{}: {error}", path.display()));
                }
            }
            Err(error) => failures.push(format!("{}: {error}", path.display())),
        }
    }

    assert!(
        failures.is_empty(),
        "checked-in example resources must stay valid:\n{}",
        failures.join("\n")
    );
}

#[test]
fn serve_check_validates_scaffolded_project() {
    let tmp = TempDir::new().unwrap();
    let root = workspace_root();
    let project_dir = tmp.path().join("serve-check");

    shaperail()
        .args(["init", "serve-check"])
        .env("SHAPERAIL_DEV_WORKSPACE", root.to_str().unwrap())
        .current_dir(tmp.path())
        .assert()
        .success();

    shaperail()
        .args(["serve", "--check"])
        .current_dir(&project_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("Serve check passed."))
        .stdout(predicate::str::contains("Resources: 1"))
        .stdout(predicate::str::contains("Command: cargo"));
}

#[test]
fn project_commands_load_dotenv_without_overriding_explicit_env() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("dotenv-project");

    shaperail()
        .args(["init", "dotenv-project"])
        .current_dir(tmp.path())
        .assert()
        .success();

    std::fs::write(
        project_dir.join("shaperail.config.yaml"),
        "project: ${SHAPERAIL_TEST_PROJECT_NAME}\n",
    )
    .unwrap();
    std::fs::write(
        project_dir.join(".env"),
        "SHAPERAIL_TEST_PROJECT_NAME=from-dotenv\n",
    )
    .unwrap();

    shaperail()
        .args(["llm-context", "--json"])
        .env_remove("SHAPERAIL_TEST_PROJECT_NAME")
        .current_dir(&project_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"from-dotenv\""));

    shaperail()
        .args(["llm-context", "--json"])
        .env("SHAPERAIL_TEST_PROJECT_NAME", "from-environment")
        .current_dir(&project_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"from-environment\""));
}

#[test]
fn project_commands_report_malformed_dotenv() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".env"), "BROKEN='unterminated\n").unwrap();

    shaperail()
        .args(["validate"])
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error: Failed to load .env:"));
}

/// Requires DATABASE_URL (e.g. CI or `docker compose up -d`). Skips when unset so
/// `cargo test` passes without a local Postgres.
#[test]
fn init_scaffold_compiles_with_local_workspace_deps() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("Skipping: DATABASE_URL not set (set it or run in CI to run this test)");
            return;
        }
    };

    let tmp = TempDir::new().unwrap();
    let root = workspace_root();
    let project_dir = tmp.path().join("compile-check");
    let target_dir = root.join("target/scaffold-smoke");
    let schema = format!(
        "scaffold_compile_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let pool = runtime
        .block_on(sqlx::PgPool::connect(&database_url))
        .unwrap();
    runtime
        .block_on(sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&pool))
        .unwrap();
    let separator = if database_url.contains('?') { '&' } else { '?' };
    let scoped_database_url = format!("{database_url}{separator}options=-csearch_path%3D{schema}");

    shaperail()
        .args(["init", project_dir.to_str().unwrap()])
        .env("SHAPERAIL_DEV_WORKSPACE", root.to_str().unwrap())
        .current_dir(tmp.path())
        .assert()
        .success();

    let status = StdCommand::new("cargo")
        // Compile coverage must not depend on which transitive crates happen to
        // be present in the developer's local Cargo cache.
        .args(["check", "--quiet"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .env("DATABASE_URL", &scoped_database_url)
        .current_dir(&project_dir)
        .status()
        .unwrap();

    runtime
        .block_on(sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&pool))
        .unwrap();
    assert!(status.success(), "scaffolded project should compile");
}

#[test]
fn scaffold_writes_llm_context_files() {
    let tmp = TempDir::new().unwrap();
    let project_name = "llm-test";

    shaperail()
        .args(["init", project_name])
        .current_dir(tmp.path())
        .assert()
        .success();

    let root = tmp.path().join(project_name);

    assert!(
        root.join("llm-context.md").exists(),
        "llm-context.md missing"
    );
    assert!(root.join("CLAUDE.md").exists(), "CLAUDE.md missing");
    assert!(root.join("AGENTS.md").exists(), "AGENTS.md missing");
    assert!(root.join("GEMINI.md").exists(), "GEMINI.md missing");
    assert!(
        root.join(".cursor/rules/shaperail.md").exists(),
        ".cursor/rules/shaperail.md missing"
    );
    assert!(
        root.join(".github/copilot-instructions.md").exists(),
        ".github/copilot-instructions.md missing"
    );
    assert!(
        root.join(".windsurfrules").exists(),
        ".windsurfrules missing"
    );

    let claude = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap();
    assert!(
        claude.contains("llm-context.md"),
        "CLAUDE.md should reference llm-context.md"
    );
    assert!(
        claude.contains("shaperail llm-context"),
        "agent adapters should name the real project-context command"
    );

    let ctx = std::fs::read_to_string(root.join("llm-context.md")).unwrap();
    assert!(
        ctx.contains("shaperail llm-context"),
        "llm-context.md should mention the llm-context command"
    );
    assert!(
        ctx.contains("$schema=./.schema.json"),
        "resource-local schema references should resolve from resources/*.yaml"
    );
    assert!(
        !ctx.contains("$schema=./resources/.schema.json"),
        "resource schema references must not duplicate the resources directory"
    );
    assert!(
        ctx.contains("resource:"),
        "llm-context.md should contain resource syntax"
    );
    assert!(
        ctx.contains("| number"),
        "llm-context.md should document the canonical number field type"
    );
    assert!(
        !ctx.contains("| float"),
        "llm-context.md must not teach the removed float field type"
    );
    assert!(
        ctx.contains("handlers::controller::{Context, ControllerResult}"),
        "llm-context.md should use the callable controller API"
    );
    assert!(
        ctx.contains("AuthenticatedUser.sub"),
        "llm-context.md should use the RFC 7519 subject field name"
    );
    assert!(
        !ctx.contains("Result<(), String>"),
        "llm-context.md must not teach stringly typed controller errors"
    );

    shaperail()
        .args(["llm-context", "--json"])
        .current_dir(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\": \"llm-test\""));
}

// --- Task 8: Validations section ---

#[test]
fn explain_prints_validation_rules_section() {
    let output = run_cli(&[
        "explain",
        "examples/incident-platform/resources/incidents.yaml",
    ]);
    assert!(output.status.success(), "explain should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Validations:"),
        "expected 'Validations:' header in output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("title: required, min=1, max=200"),
        "expected compact validation line for `title`:\n{}",
        stdout
    );
    assert!(
        stdout.contains("severity: enum [sev1, sev2, sev3, sev4]"),
        "expected compact validation line for `severity`:\n{}",
        stdout
    );
}

// --- Task 9: OpenAPI fragments section ---

#[test]
fn explain_prints_openapi_fragments() {
    let output = run_cli(&[
        "explain",
        "examples/incident-platform/resources/incidents.yaml",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("OpenAPI fragments:"),
        "expected 'OpenAPI fragments:' header in:\n{}",
        stdout
    );
    assert!(stdout.contains("list:"), "expected 'list:' fragment");
    assert!(
        stdout.contains("200:"),
        "expected '200:' status code under list"
    );
    assert!(
        stdout.contains("401:"),
        "expected '401:' status code (auth gate)"
    );
}

// --- Task 10: --format json ---

#[test]
fn explain_format_json_emits_valid_json() {
    let output = run_cli(&[
        "explain",
        "examples/incident-platform/resources/incidents.yaml",
        "--format",
        "json",
    ]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

    assert_eq!(v["resource"], "incidents");
    assert!(v["routes"].is_array(), "routes should be an array");
    assert!(
        v["table"]["columns"].is_array(),
        "table.columns should be an array"
    );
    assert!(
        v["validations"].is_object(),
        "validations should be an object keyed by field"
    );
    assert!(
        v["openapi"].is_object(),
        "openapi should be an object keyed by action"
    );
}

#[test]
fn explain_format_json_matches_documented_schema() {
    // Walk every key documented in docs/cli-reference.md's `### --format
    // <text|json>` table and assert it's actually emitted. Catches drift
    // between the documented contract and the serializer.
    let output = run_cli(&[
        "explain",
        "examples/incident-platform/resources/incidents.yaml",
        "--format",
        "json",
    ]);
    assert!(
        output.status.success(),
        "explain --format json must succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

    // Top-level keys (per docs/cli-reference.md `### --format <text|json>` table).
    let top_level = [
        "resource",
        "version",
        "db",
        "tenant_key",
        "routes",
        "table",
        "relations",
        "validations",
        "openapi",
        "indexes",
    ];
    for key in &top_level {
        assert!(
            v.get(*key).is_some(),
            "missing top-level key `{key}` in JSON output:\n{v:#}",
        );
    }

    // routes[*] shape (per the documented Route entry).
    let routes = v["routes"].as_array().expect("routes must be array");
    assert!(
        !routes.is_empty(),
        "incidents fixture has multiple endpoints"
    );
    let route_keys = [
        "method",
        "path",
        "action",
        "auth",
        "filters",
        "search",
        "sort",
        "pagination",
        "cache_ttl_seconds",
        "rate_limit",
        "soft_delete",
        "upload",
        "controller",
        "events",
        "jobs",
    ];
    for key in &route_keys {
        assert!(
            routes[0].get(*key).is_some(),
            "routes[0] missing key `{key}`:\n{:#}",
            routes[0],
        );
    }

    // table.columns[*] shape.
    let columns = v["table"]["columns"]
        .as_array()
        .expect("table.columns must be array");
    assert!(!columns.is_empty(), "incidents has columns");
    let column_keys = [
        "name",
        "type",
        "nullable",
        "primary_key",
        "unique",
        "generated",
        "references",
        "default",
        "sensitive",
    ];
    for key in &column_keys {
        assert!(
            columns[0].get(*key).is_some(),
            "table.columns[0] missing key `{key}`:\n{:#}",
            columns[0],
        );
    }

    // validations: BTreeMap<String, Vec<String>>. Each value is an array of
    // compact constraint strings.
    let validations = v["validations"]
        .as_object()
        .expect("validations must be object");
    assert!(!validations.is_empty(), "incidents has validations");
    for (field_name, parts) in validations {
        assert!(
            parts.is_array(),
            "validations[{field_name}] must be array, got {parts:?}",
        );
    }

    // openapi.<action>: { request, responses, auth }.
    let openapi = v["openapi"].as_object().expect("openapi must be object");
    assert!(!openapi.is_empty(), "incidents has openapi fragments");
    for (action, frag) in openapi {
        for key in &["request", "responses", "auth"] {
            assert!(
                frag.get(*key).is_some(),
                "openapi[{action}] missing key `{key}`:\n{frag:#}",
            );
        }
        assert!(
            frag["responses"].is_object(),
            "openapi[{action}].responses must be object (status -> body summary)",
        );
        assert!(
            frag["auth"].is_array(),
            "openapi[{action}].auth must be array of role names",
        );
    }

    // relations[*] shape.
    let relations = v["relations"].as_array().expect("relations must be array");
    if let Some(rel0) = relations.first() {
        for key in &["name", "type", "resource"] {
            assert!(
                rel0.get(*key).is_some(),
                "relations[0] missing key `{key}`:\n{rel0:#}",
            );
        }
    }
}
