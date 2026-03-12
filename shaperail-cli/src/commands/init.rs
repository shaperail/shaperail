use std::fs;
use std::path::Path;

const SHAPERAIL_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEV_WORKSPACE_ENV: &str = "SHAPERAIL_DEV_WORKSPACE";

/// Scaffold a new Shaperail project with the correct directory structure.
pub fn run(name: &str) -> i32 {
    let project_dir = Path::new(name);

    if project_dir.exists() {
        eprintln!("Error: directory '{name}' already exists");
        return 1;
    }

    if let Err(e) = scaffold(name, project_dir) {
        eprintln!("Error: {e}");
        return 1;
    }

    println!("Created Shaperail project '{name}'");
    println!();
    println!("  cd {name}");
    println!("  shaperail serve");
    0
}

fn scaffold(name: &str, root: &Path) -> Result<(), String> {
    // Create directory structure
    let dirs = [
        "",
        "resources",
        "migrations",
        "hooks",
        "seeds",
        "tests",
        "channels",
        "src",
    ];

    for dir in &dirs {
        let path = root.join(dir);
        fs::create_dir_all(&path)
            .map_err(|e| format!("Failed to create {}: {e}", path.display()))?;
    }

    // shaperail.config.yaml
    let config = format!(
        r#"project: {name}
port: 3000
workers: auto

database:
  type: postgresql
  host: localhost
  port: 5432
  name: {db_name}
  pool_size: 20

cache:
  type: redis
  url: redis://localhost:6379

auth:
  provider: jwt
  secret_env: JWT_SECRET
  expiry: 24h
  refresh_expiry: 30d

logging:
  level: info
  format: json
"#,
        db_name = name.replace('-', "_")
    );
    write_file(&root.join("shaperail.config.yaml"), &config)?;

    // Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
shaperail-runtime = {shaperail_runtime_dep}
shaperail-core = {shaperail_core_dep}
shaperail-codegen = {shaperail_codegen_dep}
actix-web = "4"
tokio = {{ version = "1", features = ["full"] }}
sqlx = {{ version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono", "json"] }}
serde_json = "1"
tracing = "0.1"
tracing-subscriber = {{ version = "0.3", features = ["env-filter"] }}
dotenvy = "0.15"
"#,
        shaperail_runtime_dep = shaperail_dependency("shaperail-runtime"),
        shaperail_core_dep = shaperail_dependency("shaperail-core"),
        shaperail_codegen_dep = shaperail_dependency("shaperail-codegen")
    );
    write_file(&root.join("Cargo.toml"), &cargo_toml)?;

    // src/main.rs
    let main_rs = r#"use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use shaperail_runtime::auth::jwt::JwtConfig;
use shaperail_runtime::handlers::{register_all_resources, AppState};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter("info").init();

    // Parse resource files
    let resources_dir = std::path::Path::new("resources");
    let mut resources = Vec::new();
    for entry in std::fs::read_dir(resources_dir).expect("resources/ directory not found") {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "yaml" || e == "yml") {
            let rd = shaperail_codegen::parser::parse_resource_file(&path)
                .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", path.display()));
            resources.push(rd);
        }
    }
    tracing::info!("Loaded {} resource(s)", resources.len());

    // Connect to database
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set (check .env file)");
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");
    tracing::info!("Connected to database");

    // JWT auth (reads JWT_SECRET env var)
    let jwt_config = JwtConfig::from_env().map(Arc::new);

    let port: u16 = std::env::var("SHAPERAIL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    // Build app state
    let state = Arc::new(AppState {
        pool,
        resources: resources.clone(),
        jwt_config: jwt_config.clone(),
        cache: None,
        event_emitter: None,
    });

    tracing::info!("Starting Shaperail server on port {port}");

    let state_clone = state.clone();
    let resources_clone = resources.clone();

    HttpServer::new(move || {
        let st = state_clone.clone();
        let res = resources_clone.clone();
        let mut app = App::new()
            .app_data(web::Data::new(st.clone()));
        if let Some(ref jwt) = jwt_config {
            app = app.app_data(web::Data::new(jwt.clone()));
        }
        app.configure(|cfg| register_all_resources(cfg, &res, st))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
"#;
    write_file(&root.join("src/main.rs"), main_rs)?;

    // Example resource file
    let example_resource = r#"resource: posts
version: 1

schema:
  id:         { type: uuid, primary: true, generated: true }
  title:      { type: string, min: 1, max: 500, required: true }
  body:       { type: string, required: true }
  author_id:  { type: uuid, required: true }
  published:  { type: boolean, default: false }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }

endpoints:
  list:
    method: GET
    path: /posts
    auth: public
    filters: [author_id, published]
    search: [title, body]
    pagination: cursor
    sort: [created_at, title]

  get:
    method: GET
    path: /posts/:id
    auth: public

  create:
    method: POST
    path: /posts
    auth: [admin, member]
    input: [title, body, author_id, published]

  update:
    method: PATCH
    path: /posts/:id
    auth: [admin, owner]
    input: [title, body, published]

  delete:
    method: DELETE
    path: /posts/:id
    auth: [admin]
    soft_delete: true
"#;
    write_file(&root.join("resources/posts.yaml"), example_resource)?;

    // .env
    let dotenv = format!(
        r#"DATABASE_URL=postgresql://shaperail:shaperail@localhost:5432/{db_name}
REDIS_URL=redis://localhost:6379
JWT_SECRET=change-me-in-production
"#,
        db_name = name.replace('-', "_")
    );
    write_file(&root.join(".env"), &dotenv)?;

    // .gitignore
    let gitignore = r#"/target
.env
*.swp
*.swo
"#;
    write_file(&root.join(".gitignore"), gitignore)?;

    // docker-compose.yml
    let docker_compose = format!(
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: {db_name}
      POSTGRES_USER: shaperail
      POSTGRES_PASSWORD: shaperail
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U shaperail"]
      interval: 5s
      timeout: 3s
      retries: 10

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 3s
      retries: 10

volumes:
  postgres_data:
"#,
        db_name = name.replace('-', "_")
    );
    write_file(&root.join("docker-compose.yml"), &docker_compose)?;

    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

fn shaperail_dependency(crate_name: &str) -> String {
    match std::env::var(DEV_WORKSPACE_ENV) {
        Ok(workspace_root) if !workspace_root.is_empty() => {
            let path = Path::new(&workspace_root).join(crate_name);
            let path = path.to_string_lossy().replace('\\', "\\\\");
            format!(r#"{{ version = "{SHAPERAIL_VERSION}", path = "{path}" }}"#)
        }
        _ => format!(r#"{{ version = "{SHAPERAIL_VERSION}" }}"#),
    }
}
