use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn shaperail() -> Command {
    Command::cargo_bin("shaperail").unwrap()
}

/// Returns the workspace root directory (where resources/ lives).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
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
        .stdout(predicate::str::contains("job queue"));
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
        .stdout(predicate::str::contains("Created Shaperail project"));

    // Check directory structure
    assert!(project_dir.join("shaperail.config.yaml").exists());
    assert!(project_dir.join("Cargo.toml").exists());
    assert!(project_dir.join("src/main.rs").exists());
    assert!(project_dir.join("resources").is_dir());
    assert!(project_dir.join("migrations").is_dir());
    assert!(project_dir.join("hooks").is_dir());
    assert!(project_dir.join("seeds").is_dir());
    assert!(project_dir.join("tests").is_dir());
    assert!(project_dir.join("channels").is_dir());
    assert!(project_dir.join(".env").exists());
    assert!(project_dir.join(".gitignore").exists());
    assert!(project_dir.join("docker-compose.yml").exists());
    assert!(project_dir.join("resources/posts.yaml").exists());

    // Verify config content
    let config = std::fs::read_to_string(project_dir.join("shaperail.config.yaml")).unwrap();
    assert!(config.contains("project: test-project"));
    assert!(config.contains("port: 3000"));
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
        .stdout(predicate::str::contains("/users"));
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
fn init_scaffold_compiles_with_local_workspace_deps() {
    let tmp = TempDir::new().unwrap();
    let root = workspace_root();
    let project_dir = tmp.path().join("compile-check");
    let target_dir = root.join("target/scaffold-smoke");

    shaperail()
        .args(["init", "compile-check"])
        .env("SHAPERAIL_DEV_WORKSPACE", root.to_str().unwrap())
        .current_dir(tmp.path())
        .assert()
        .success();

    let status = StdCommand::new("cargo")
        .args(["check", "--offline"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .current_dir(&project_dir)
        .status()
        .unwrap();

    assert!(status.success(), "scaffolded project should compile");
}
