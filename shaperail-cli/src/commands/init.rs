use std::fs;
use std::path::Path;

const SHAPERAIL_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEV_WORKSPACE_ENV: &str = "SHAPERAIL_DEV_WORKSPACE";
const RUST_TOOLCHAIN_VERSION: &str = "1.85";

/// Scaffold a new Shaperail project with the correct directory structure.
pub fn run(name: &str) -> i32 {
    let project_dir = Path::new(name);
    let project_name = match derive_project_name(project_dir) {
        Ok(name) => name,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    if project_dir.exists() {
        eprintln!("Error: directory '{name}' already exists");
        return 1;
    }

    if let Err(e) = scaffold(&project_name, project_dir) {
        eprintln!("Error: {e}");
        return 1;
    }

    println!("Created Shaperail project '{}'", project_dir.display());
    println!();
    println!("  cd {}", project_dir.display());
    println!("  docker compose up -d");
    println!("  shaperail serve");
    println!();
    println!("  Docs:    http://localhost:3000/docs");
    println!("  OpenAPI: http://localhost:3000/openapi.json");
    0
}

fn derive_project_name(project_dir: &Path) -> Result<String, String> {
    project_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("Invalid project path '{}'", project_dir.display()))
}

fn scaffold(project_name: &str, root: &Path) -> Result<(), String> {
    // Create directory structure
    let dirs = [
        "",
        "resources",
        "migrations",
        "hooks",
        "seeds",
        "tests",
        "channels",
        "generated",
        "src",
    ];

    for dir in &dirs {
        let path = root.join(dir);
        fs::create_dir_all(&path)
            .map_err(|e| format!("Failed to create {}: {e}", path.display()))?;
    }

    // shaperail.config.yaml
    let config = format!(
        r#"project: {project_name}
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
        db_name = project_name.replace('-', "_")
    );
    write_file(&root.join("shaperail.config.yaml"), &config)?;

    // Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "{project_name}"
version = "0.1.0"
edition = "2021"
rust-version = "{RUST_TOOLCHAIN_VERSION}"

[dependencies]
shaperail-runtime = {shaperail_runtime_dep}
shaperail-core = {shaperail_core_dep}
shaperail-codegen = {shaperail_codegen_dep}
actix-web = "4"
tokio = {{ version = "1", features = ["full"] }}
sqlx = {{ version = "0.8", default-features = false, features = ["runtime-tokio", "postgres", "uuid", "chrono", "json", "migrate"] }}
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
    let main_rs = r###"use std::io;
use std::path::Path;
use std::sync::Arc;

use actix_web::{web, App, HttpResponse, HttpServer};
use shaperail_runtime::auth::jwt::JwtConfig;
use shaperail_runtime::cache::{create_redis_pool, RedisCache};
use shaperail_runtime::events::EventEmitter;
use shaperail_runtime::handlers::{register_all_resources, AppState};
use shaperail_runtime::jobs::JobQueue;
use shaperail_runtime::observability::{
    health_handler, health_ready_handler, metrics_handler, HealthState, MetricsState,
};

fn io_error(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

const DOCS_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Shaperail API Docs</title>
  <style>
    :root {
      --bg: #f5efe6;
      --surface: rgba(255, 252, 247, 0.94);
      --surface-strong: #ffffff;
      --ink: #201a17;
      --muted: #6d625a;
      --border: rgba(74, 54, 41, 0.16);
      --accent: #a64b2a;
      --accent-soft: rgba(166, 75, 42, 0.12);
      --get: #1f7a4d;
      --post: #9c3f0f;
      --patch: #8250df;
      --delete: #b42318;
      --shadow: 0 24px 80px rgba(74, 54, 41, 0.12);
    }

    * {
      box-sizing: border-box;
    }

    body {
      margin: 0;
      font-family: "Iowan Old Style", "Palatino Linotype", "Book Antiqua", Georgia, serif;
      color: var(--ink);
      background:
        radial-gradient(circle at top left, rgba(166, 75, 42, 0.14), transparent 28rem),
        linear-gradient(180deg, #fbf7f1 0%, var(--bg) 100%);
      min-height: 100vh;
    }

    a {
      color: inherit;
    }

    .shell {
      width: min(1100px, calc(100% - 2rem));
      margin: 0 auto;
      padding: 2rem 0 3rem;
    }

    .hero {
      display: grid;
      gap: 1rem;
      padding: 1.5rem;
      border: 1px solid var(--border);
      border-radius: 28px;
      background: linear-gradient(135deg, rgba(255, 252, 247, 0.96), rgba(255, 245, 239, 0.92));
      box-shadow: var(--shadow);
    }

    .eyebrow {
      margin: 0;
      font-family: "IBM Plex Mono", "SFMono-Regular", "Consolas", monospace;
      font-size: 0.78rem;
      letter-spacing: 0.12em;
      text-transform: uppercase;
      color: var(--accent);
    }

    h1 {
      margin: 0;
      font-size: clamp(2.2rem, 5vw, 4rem);
      line-height: 0.95;
      letter-spacing: -0.04em;
    }

    .hero-copy {
      margin: 0;
      max-width: 50rem;
      font-size: 1.05rem;
      color: var(--muted);
    }

    .hero-links {
      display: flex;
      flex-wrap: wrap;
      gap: 0.75rem;
    }

    .hero-links a {
      padding: 0.7rem 1rem;
      border-radius: 999px;
      border: 1px solid var(--border);
      background: var(--surface-strong);
      text-decoration: none;
      font-family: "IBM Plex Mono", "SFMono-Regular", "Consolas", monospace;
      font-size: 0.85rem;
    }

    .grid {
      display: grid;
      gap: 1rem;
      margin-top: 1rem;
    }

    .card {
      border: 1px solid var(--border);
      border-radius: 22px;
      background: var(--surface);
      padding: 1.1rem 1.2rem;
      box-shadow: 0 12px 40px rgba(74, 54, 41, 0.08);
      backdrop-filter: blur(10px);
    }

    .error {
      border-color: rgba(180, 35, 24, 0.25);
      color: var(--delete);
      background: rgba(255, 241, 239, 0.94);
    }

    .summary {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
      gap: 0.8rem;
    }

    .metric-value {
      display: block;
      margin-top: 0.35rem;
      font-family: "IBM Plex Mono", "SFMono-Regular", "Consolas", monospace;
      font-size: 1.3rem;
      color: var(--ink);
    }

    .metric-label {
      color: var(--muted);
      font-size: 0.9rem;
    }

    .ops {
      display: grid;
      gap: 0.85rem;
      margin-top: 1rem;
    }

    .op {
      border: 1px solid var(--border);
      border-radius: 24px;
      overflow: hidden;
      background: var(--surface);
      box-shadow: 0 14px 40px rgba(74, 54, 41, 0.07);
    }

    .op summary {
      list-style: none;
      display: grid;
      grid-template-columns: auto 1fr auto;
      gap: 1rem;
      align-items: start;
      padding: 1rem 1.2rem;
      cursor: pointer;
    }

    .op summary::-webkit-details-marker {
      display: none;
    }

    .op-body {
      display: grid;
      gap: 0.75rem;
      padding: 0 1.2rem 1.2rem;
    }

    .method {
      min-width: 5.5rem;
      text-align: center;
      padding: 0.45rem 0.7rem;
      border-radius: 999px;
      font-family: "IBM Plex Mono", "SFMono-Regular", "Consolas", monospace;
      font-size: 0.84rem;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: white;
    }

    .method.get { background: var(--get); }
    .method.post { background: var(--post); }
    .method.patch { background: var(--patch); }
    .method.put { background: #155eef; }
    .method.delete { background: var(--delete); }

    .path {
      font-family: "IBM Plex Mono", "SFMono-Regular", "Consolas", monospace;
      font-size: 1rem;
      word-break: break-word;
    }

    .muted {
      color: var(--muted);
    }

    .pills {
      display: flex;
      flex-wrap: wrap;
      gap: 0.5rem;
    }

    .pill {
      display: inline-flex;
      align-items: center;
      gap: 0.3rem;
      padding: 0.35rem 0.6rem;
      border-radius: 999px;
      background: var(--accent-soft);
      color: var(--ink);
      font-family: "IBM Plex Mono", "SFMono-Regular", "Consolas", monospace;
      font-size: 0.75rem;
    }

    .section-title {
      margin: 0;
      font-size: 0.88rem;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--muted);
      font-family: "IBM Plex Mono", "SFMono-Regular", "Consolas", monospace;
    }

    pre {
      margin: 0;
      overflow-x: auto;
      padding: 0.9rem 1rem;
      border-radius: 16px;
      background: #201a17;
      color: #f6efe8;
      font-family: "IBM Plex Mono", "SFMono-Regular", "Consolas", monospace;
      font-size: 0.8rem;
    }

    @media (max-width: 720px) {
      .shell {
        width: min(100% - 1rem, 1100px);
        padding-top: 1rem;
      }

      .hero,
      .card {
        border-radius: 20px;
      }

      .op summary {
        grid-template-columns: 1fr;
      }
    }
  </style>
</head>
<body>
  <main class="shell">
    <section class="hero">
      <p class="eyebrow">Shaperail Documentation</p>
      <div>
        <h1 id="doc-title">Loading API docs</h1>
        <p class="hero-copy" id="doc-copy">Reading <code>/openapi.json</code> from the running app and rendering the live contract.</p>
      </div>
      <div class="hero-links">
        <a href="/openapi.json" target="_blank" rel="noreferrer">Open raw OpenAPI JSON</a>
        <a href="/health" target="_blank" rel="noreferrer">Health check</a>
      </div>
    </section>

    <section class="grid">
      <div id="status" class="card">Loading the generated OpenAPI specification...</div>
      <div id="summary" class="card summary" hidden></div>
      <div id="operations" class="ops"></div>
    </section>
  </main>

  <script>
    const statusEl = document.getElementById("status");
    const summaryEl = document.getElementById("summary");
    const operationsEl = document.getElementById("operations");
    const titleEl = document.getElementById("doc-title");
    const copyEl = document.getElementById("doc-copy");

    function escapeHtml(value) {
      return String(value)
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;")
        .replaceAll('"', "&quot;")
        .replaceAll("'", "&#39;");
    }

    function methodClass(method) {
      return method.toLowerCase();
    }

    function renderPills(values, formatter) {
      if (!values.length) {
        return '<span class="muted">None</span>';
      }

      return `<div class="pills">${values.map((value) => `<span class="pill">${formatter(value)}</span>`).join("")}</div>`;
    }

    function renderOperation(path, method, operation) {
      const parameters = operation.parameters || [];
      const security = operation.security || [];
      const responses = operation.responses || {};
      const requestBody = operation.requestBody || null;
      const responseEntries = Object.entries(responses);
      const description = operation.description || operation.summary || "No description provided.";
      const summary = operation.summary || `${method.toUpperCase()} ${path}`;

      return `
        <details class="op">
          <summary>
            <span class="method ${methodClass(method)}">${escapeHtml(method)}</span>
            <div>
              <div class="path">${escapeHtml(path)}</div>
              <div class="muted">${escapeHtml(summary)}</div>
            </div>
            <div class="muted">${escapeHtml(operation.operationId || "")}</div>
          </summary>
          <div class="op-body">
            <div>
              <h3 class="section-title">Description</h3>
              <p>${escapeHtml(description)}</p>
            </div>
            <div>
              <h3 class="section-title">Auth</h3>
              ${renderPills(
                security.map((entry) => Object.keys(entry).join(", ")).filter(Boolean),
                (value) => escapeHtml(value)
              )}
            </div>
            <div>
              <h3 class="section-title">Parameters</h3>
              ${renderPills(
                parameters.map((param) => `${param.in}: ${param.name}`),
                (value) => escapeHtml(value)
              )}
            </div>
            <div>
              <h3 class="section-title">Request body</h3>
              ${requestBody
                ? `<pre>${escapeHtml(JSON.stringify(requestBody, null, 2))}</pre>`
                : '<span class="muted">None</span>'}
            </div>
            <div>
              <h3 class="section-title">Responses</h3>
              ${responseEntries.length
                ? responseEntries.map(([status, response]) => `
                    <div>
                      <div class="pill">${escapeHtml(status)}</div>
                      <pre>${escapeHtml(JSON.stringify(response, null, 2))}</pre>
                    </div>
                  `).join("")
                : '<span class="muted">None</span>'}
            </div>
          </div>
        </details>
      `;
    }

    function render(spec) {
      const operations = [];
      for (const [path, pathItem] of Object.entries(spec.paths || {})) {
        for (const [method, operation] of Object.entries(pathItem)) {
          operations.push({ path, method: method.toUpperCase(), operation });
        }
      }

      operations.sort((left, right) => {
        const pathOrder = left.path.localeCompare(right.path);
        if (pathOrder !== 0) {
          return pathOrder;
        }
        return left.method.localeCompare(right.method);
      });

      const title = spec.info?.title || "Shaperail API";
      const version = spec.info?.version || "1.0.0";
      titleEl.textContent = `${title} docs`;
      copyEl.textContent = `OpenAPI ${spec.openapi || "3.1.0"} · ${operations.length} operations · generated from your Shaperail resources.`;

      summaryEl.hidden = false;
      summaryEl.innerHTML = `
        <div>
          <span class="metric-label">Title</span>
          <span class="metric-value">${escapeHtml(title)}</span>
        </div>
        <div>
          <span class="metric-label">Version</span>
          <span class="metric-value">${escapeHtml(version)}</span>
        </div>
        <div>
          <span class="metric-label">OpenAPI</span>
          <span class="metric-value">${escapeHtml(spec.openapi || "3.1.0")}</span>
        </div>
        <div>
          <span class="metric-label">Operations</span>
          <span class="metric-value">${operations.length}</span>
        </div>
      `;

      operationsEl.innerHTML = operations
        .map(({ path, method, operation }) => renderOperation(path, method, operation))
        .join("");

      statusEl.remove();
    }

    async function boot() {
      try {
        const response = await fetch("/openapi.json", {
          headers: { Accept: "application/json" }
        });
        if (!response.ok) {
          throw new Error(`HTTP ${response.status}`);
        }
        const spec = await response.json();
        render(spec);
      } catch (error) {
        statusEl.classList.add("error");
        statusEl.textContent = `Failed to load /openapi.json: ${error.message}`;
      }
    }

    boot();
  </script>
</body>
</html>
"##;

async fn openapi_json_handler(spec: web::Data<Arc<String>>) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .content_type("application/json")
        .body(spec.get_ref().as_ref().clone())
}

async fn docs_handler() -> HttpResponse {
    HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .content_type("text/html; charset=utf-8")
        .body(DOCS_HTML)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter("info").init();

    let config_path = Path::new("shaperail.config.yaml");
    let config = shaperail_codegen::config_parser::parse_config_file(config_path)
        .map_err(|e| io_error(format!("Failed to parse {}: {e}", config_path.display())))?;

    let resources_dir = Path::new("resources");
    let mut resources = Vec::new();
    for entry in std::fs::read_dir(resources_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "yaml" || e == "yml") {
            let rd = shaperail_codegen::parser::parse_resource_file(&path)
                .map_err(|e| io_error(format!("Failed to parse {}: {e}", path.display())))?;
            let validation_errors = shaperail_codegen::validator::validate_resource(&rd);
            if !validation_errors.is_empty() {
                let rendered = validation_errors
                    .into_iter()
                    .map(|err| err.to_string())
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(io_error(format!("{}: {rendered}", path.display())));
            }
            resources.push(rd);
        }
    }
    tracing::info!("Loaded {} resource(s)", resources.len());

    let openapi_spec = shaperail_codegen::openapi::generate(&config, &resources);
    let openapi_json = Arc::new(
        shaperail_codegen::openapi::to_json(&openapi_spec)
            .map_err(|e| io_error(format!("Failed to serialize OpenAPI spec: {e}")))?,
    );

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| io_error("DATABASE_URL must be set (generated by shaperail init in .env)"))?;
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .map_err(|e| io_error(format!(
            "Failed to connect to database: {e}. Run `docker compose up -d` to start the preconfigured Postgres and Redis services."
        )))?;
    let migrator = sqlx::migrate::Migrator::new(Path::new("./migrations"))
        .await
        .map_err(|e| io_error(format!("Failed to load migrations: {e}")))?;
    migrator
        .run(&pool)
        .await
        .map_err(|e| io_error(format!("Failed to apply migrations: {e}")))?;
    tracing::info!("Connected to database and applied migrations");

    let redis_pool = match config.cache.as_ref() {
        Some(cache_config) => Some(Arc::new(
            create_redis_pool(&cache_config.url)
                .map_err(|e| io_error(format!("Failed to create Redis pool: {e}")))?,
        )),
        None => None,
    };
    let cache = redis_pool.as_ref().map(|pool| RedisCache::new(pool.clone()));
    let job_queue = redis_pool.as_ref().map(|pool| JobQueue::new(pool.clone()));
    let event_emitter = job_queue
        .clone()
        .map(|queue| EventEmitter::new(queue, config.events.as_ref()));
    let jwt_config = JwtConfig::from_env().map(Arc::new);

    let port: u16 = std::env::var("SHAPERAIL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(config.port);

    let state = Arc::new(AppState {
        pool: pool.clone(),
        resources: resources.clone(),
        jwt_config: jwt_config.clone(),
        cache,
        event_emitter,
        job_queue,
    });
    let health_state = web::Data::new(HealthState::new(Some(pool), redis_pool));
    let metrics_state = web::Data::new(
        MetricsState::new().map_err(|e| io_error(format!("Failed to initialize metrics: {e}")))?,
    );

    tracing::info!("Starting Shaperail server on port {port}");
    tracing::info!("OpenAPI spec available at http://localhost:{port}/openapi.json");
    tracing::info!("API docs available at http://localhost:{port}/docs");

    let state_clone = state.clone();
    let resources_clone = resources.clone();
    let health_state_clone = health_state.clone();
    let metrics_state_clone = metrics_state.clone();
    let jwt_config_clone = jwt_config.clone();
    let openapi_json_clone = openapi_json.clone();

    HttpServer::new(move || {
        let st = state_clone.clone();
        let res = resources_clone.clone();
        let spec = openapi_json_clone.clone();
        let mut app = App::new()
            .app_data(web::Data::new(st.clone()))
            .app_data(web::Data::new(spec))
            .app_data(health_state_clone.clone())
            .app_data(metrics_state_clone.clone())
            .route("/health", web::get().to(health_handler))
            .route("/health/ready", web::get().to(health_ready_handler))
            .route("/metrics", web::get().to(metrics_handler))
            .route("/openapi.json", web::get().to(openapi_json_handler))
            .route("/docs", web::get().to(docs_handler));
        if let Some(ref jwt) = jwt_config_clone {
            app = app.app_data(web::Data::new(jwt.clone()));
        }
        app.configure(|cfg| register_all_resources(cfg, &res, st))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
"###;
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
    let parsed_example = shaperail_codegen::parser::parse_resource(example_resource)
        .map_err(|e| format!("Failed to parse example resource: {e}"))?;
    let validation_errors = shaperail_codegen::validator::validate_resource(&parsed_example);
    if !validation_errors.is_empty() {
        let rendered = validation_errors
            .into_iter()
            .map(|err| err.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("Example resource failed validation: {rendered}"));
    }
    let initial_migration = super::migrate::render_migration_sql(&parsed_example);
    write_file(
        &root.join("migrations/0001_create_posts.sql"),
        &initial_migration,
    )?;

    // .env
    let dotenv = format!(
        r#"DATABASE_URL=postgresql://shaperail:shaperail@localhost:5432/{db_name}
REDIS_URL=redis://localhost:6379
JWT_SECRET=change-me-in-production
"#,
        db_name = project_name.replace('-', "_")
    );
    write_file(&root.join(".env"), &dotenv)?;

    let readme = format!(
        r#"# {project_name}

```bash
docker compose up -d
shaperail serve
```

Local development is Docker-first. No manual database creation is required:
the included `docker-compose.yml` starts Postgres and Redis with credentials
that already match `.env`, and Postgres creates the `{db_name}` database
automatically on first boot.

- App: http://localhost:3000
- Docs: http://localhost:3000/docs
- OpenAPI: http://localhost:3000/openapi.json

When you change resource schemas later:

```bash
shaperail migrate
shaperail serve
```
"#,
        db_name = project_name.replace('-', "_")
    );
    write_file(&root.join("README.md"), &readme)?;

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
        db_name = project_name.replace('-', "_")
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
