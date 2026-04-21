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
        "controllers",
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
build = "build.rs"

[features]
default = []
graphql = ["shaperail-runtime/graphql"]
grpc = ["shaperail-runtime/grpc"]
wasm-plugins = ["shaperail-runtime/wasm-plugins"]

[dependencies]
# Core framework — default-features = false gives you REST + Postgres (Tier 1).
# Enable optional features as needed (in [features] above):
#   cargo build --features graphql       — GraphQL via async-graphql
#   cargo build --features grpc          — gRPC via tonic
#   cargo build --features wasm-plugins  — WASM controller hooks via wasmtime
shaperail-runtime = {shaperail_runtime_dep}
shaperail-core = {shaperail_core_dep}
shaperail-codegen = {shaperail_codegen_dep}
actix-web = "4"
tokio = {{ version = "1", features = ["full"] }}
sqlx = {{ version = "0.8", default-features = false, features = ["runtime-tokio", "postgres", "uuid", "chrono", "json", "migrate", "macros"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
uuid = {{ version = "1", features = ["v4", "serde"] }}
chrono = {{ version = "0.4", features = ["serde"] }}
tracing = "0.1"
tracing-subscriber = {{ version = "0.3", features = ["env-filter"] }}
dotenvy = "0.15"

[build-dependencies]
dotenvy = "0.15"
sqlx = {{ version = "0.8", default-features = false, features = ["runtime-tokio", "postgres", "migrate"] }}
tokio = {{ version = "1", features = ["rt-multi-thread"] }}
"#,
        shaperail_runtime_dep = shaperail_dependency_no_defaults("shaperail-runtime"),
        shaperail_core_dep = shaperail_dependency("shaperail-core"),
        shaperail_codegen_dep = shaperail_dependency("shaperail-codegen")
    );
    write_file(&root.join("Cargo.toml"), &cargo_toml)?;

    // src/main.rs
    let main_rs = r###"use std::io;
use std::path::Path;
use std::sync::Arc;

use actix_web::{web, App, HttpResponse, HttpServer};
#[path = "../generated/mod.rs"]
mod generated;
use shaperail_runtime::auth::jwt::JwtConfig;
use shaperail_runtime::cache::{create_redis_pool, RedisCache};
use shaperail_runtime::events::EventEmitter;
use shaperail_runtime::handlers::{register_all_resources, AppState};
use shaperail_runtime::jobs::{JobQueue, Worker};
use shaperail_runtime::observability::{
    health_handler, health_ready_handler, metrics_handler, sensitive_fields, HealthState,
    MetricsState, RequestLogger,
};
use shaperail_runtime::ws::{load_channels, RedisPubSub, RoomManager};

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

    function schemaRefName(ref) {
      return ref.split("/").filter(Boolean).at(-1) || ref;
    }

    function resolvePointer(spec, pointer) {
      if (!pointer || !pointer.startsWith("#/")) {
        return null;
      }

      return pointer
        .slice(2)
        .split("/")
        .reduce((value, segment) => {
          if (value == null) {
            return null;
          }
          const key = segment.replaceAll("~1", "/").replaceAll("~0", "~");
          return value[key];
        }, spec);
    }

    function resolveSchema(schema, spec, seen = new Set()) {
      if (!schema) {
        return null;
      }

      if (schema.$ref) {
        if (seen.has(schema.$ref)) {
          return { title: schemaRefName(schema.$ref) };
        }

        const target = resolvePointer(spec, schema.$ref);
        if (!target) {
          return { title: schemaRefName(schema.$ref) };
        }

        const nextSeen = new Set(seen);
        nextSeen.add(schema.$ref);
        const resolved = resolveSchema(target, spec, nextSeen) || {};
        return {
          ...resolved,
          title: resolved.title || schemaRefName(schema.$ref)
        };
      }

      if (schema.allOf && schema.allOf.length) {
        const merged = {
          ...schema,
          properties: {},
          required: []
        };

        for (const part of schema.allOf) {
          const resolved = resolveSchema(part, spec, seen) || {};
          if (resolved.properties) {
            Object.assign(merged.properties, resolved.properties);
          }
          if (resolved.required) {
            merged.required.push(...resolved.required);
          }
        }

        merged.required = [...new Set(merged.required)];
        schema = merged;
      }

      const resolved = { ...schema };

      if (resolved.properties) {
        resolved.properties = Object.fromEntries(
          Object.entries(resolved.properties).map(([key, value]) => [
            key,
            resolveSchema(value, spec, new Set(seen)) || value
          ])
        );
      }

      if (resolved.items) {
        resolved.items = resolveSchema(resolved.items, spec, new Set(seen)) || resolved.items;
      }

      if (resolved.additionalProperties && typeof resolved.additionalProperties === "object") {
        resolved.additionalProperties =
          resolveSchema(resolved.additionalProperties, spec, new Set(seen)) ||
          resolved.additionalProperties;
      }

      if (resolved.oneOf) {
        resolved.oneOf = resolved.oneOf.map((value) => resolveSchema(value, spec, new Set(seen)) || value);
      }

      if (resolved.anyOf) {
        resolved.anyOf = resolved.anyOf.map((value) => resolveSchema(value, spec, new Set(seen)) || value);
      }

      return resolved;
    }

    function schemaLabel(schema, spec) {
      if (!schema) {
        return "No schema";
      }

      if (schema.$ref) {
        return schemaRefName(schema.$ref);
      }

      if (schema.oneOf?.length) {
        return schema.oneOf.map((entry) => schemaLabel(entry, spec)).join(" | ");
      }

      if (schema.anyOf?.length) {
        return schema.anyOf.map((entry) => schemaLabel(entry, spec)).join(" | ");
      }

      if (schema.allOf?.length) {
        return schema.allOf.map((entry) => schemaLabel(entry, spec)).join(" & ");
      }

      if (schema.enum?.length) {
        return `enum(${schema.enum.join(", ")})`;
      }

      if (schema.type === "array") {
        return `${schemaLabel(schema.items, spec)}[]`;
      }

      if (schema.title) {
        return schema.title;
      }

      if (schema.type === "object" || schema.properties) {
        return "object";
      }

      if (schema.type && schema.format) {
        return `${schema.type} (${schema.format})`;
      }

      return schema.type || "object";
    }

    function schemaSummary(schema, spec) {
      const resolved = resolveSchema(schema, spec);
      if (resolved?.properties?.data) {
        return `data: ${schemaLabel(resolved.properties.data, spec)}`;
      }
      if (resolved?.properties?.error) {
        return "error response";
      }
      return schemaLabel(schema, spec);
    }

    function exampleForSchema(schema, spec, depth = 0, seen = new Set()) {
      if (!schema || depth > 6) {
        return null;
      }

      if (schema.example !== undefined) {
        return schema.example;
      }

      if (schema.default !== undefined) {
        return schema.default;
      }

      if (schema.const !== undefined) {
        return schema.const;
      }

      if (schema.$ref) {
        if (seen.has(schema.$ref)) {
          return schemaRefName(schema.$ref);
        }

        const target = resolvePointer(spec, schema.$ref);
        if (!target) {
          return schemaRefName(schema.$ref);
        }

        const nextSeen = new Set(seen);
        nextSeen.add(schema.$ref);
        return exampleForSchema(target, spec, depth + 1, nextSeen);
      }

      if (schema.oneOf?.length) {
        return exampleForSchema(schema.oneOf[0], spec, depth + 1, seen);
      }

      if (schema.anyOf?.length) {
        return exampleForSchema(schema.anyOf[0], spec, depth + 1, seen);
      }

      if (schema.allOf?.length) {
        const merged = {};
        let hasObjectShape = false;

        for (const part of schema.allOf) {
          const value = exampleForSchema(part, spec, depth + 1, seen);
          if (value && typeof value === "object" && !Array.isArray(value)) {
            Object.assign(merged, value);
            hasObjectShape = true;
          }
        }

        return hasObjectShape ? merged : null;
      }

      if (schema.enum?.length) {
        return schema.enum[0];
      }

      switch (schema.type) {
        case "object": {
          const output = {};
          for (const [key, value] of Object.entries(schema.properties || {})) {
            output[key] = exampleForSchema(value, spec, depth + 1, seen);
          }

          if (Object.keys(output).length) {
            return output;
          }

          if (schema.additionalProperties) {
            return {
              key: exampleForSchema(schema.additionalProperties, spec, depth + 1, seen)
            };
          }

          return {};
        }
        case "array":
          return [exampleForSchema(schema.items || {}, spec, depth + 1, seen)];
        case "string":
          if (schema.format === "uuid") {
            return "00000000-0000-0000-0000-000000000000";
          }
          if (schema.format === "date-time") {
            return "2026-01-01T00:00:00Z";
          }
          if (schema.format === "date") {
            return "2026-01-01";
          }
          if (schema.format === "email") {
            return "user@example.com";
          }
          if (schema.format === "uri" || schema.format === "url") {
            return "https://example.com";
          }
          return "string";
        case "integer":
          return 1;
        case "number":
          return 1.0;
        case "boolean":
          return false;
        default:
          return null;
      }
    }

    function renderSchemaPanel(schema, spec) {
      if (!schema) {
        return '<span class="muted">No schema</span>';
      }

      const resolved = resolveSchema(schema, spec);
      const example = exampleForSchema(schema, spec);
      const rawSchema = resolved ? JSON.stringify(resolved, null, 2) : null;

      return `
        <div>
          <div class="pills">
            <span class="pill">${escapeHtml(schemaSummary(schema, spec))}</span>
          </div>
          ${example !== null
            ? `<pre>${escapeHtml(JSON.stringify(example, null, 2))}</pre>`
            : '<span class="muted">No example available</span>'}
          ${rawSchema
            ? `<details><summary class="section-title">View schema</summary><pre>${escapeHtml(rawSchema)}</pre></details>`
            : ""}
        </div>
      `;
    }

    function renderContentBlock(content, spec) {
      const entries = Object.entries(content || {});
      if (!entries.length) {
        return '<span class="muted">No body</span>';
      }

      return entries.map(([contentType, mediaType]) => `
        <div>
          <div class="pills">
            <span class="pill">${escapeHtml(contentType)}</span>
            <span class="pill">${escapeHtml(schemaSummary(mediaType.schema, spec))}</span>
          </div>
          ${renderSchemaPanel(mediaType.schema, spec)}
        </div>
      `).join("");
    }

    function renderResponses(responses, spec) {
      const responseEntries = Object.entries(responses || {});
      if (!responseEntries.length) {
        return '<span class="muted">None</span>';
      }

      return responseEntries.map(([status, response]) => `
        <div>
          <div class="pills">
            <span class="pill">${escapeHtml(status)}</span>
            <span class="pill">${escapeHtml(response.description || "No description")}</span>
          </div>
          ${renderContentBlock(response.content, spec)}
        </div>
      `).join("");
    }

    function renderPills(values, formatter) {
      if (!values.length) {
        return '<span class="muted">None</span>';
      }

      return `<div class="pills">${values.map((value) => `<span class="pill">${formatter(value)}</span>`).join("")}</div>`;
    }

    function renderOperation(path, method, operation, spec) {
      const parameters = operation.parameters || [];
      const security = operation.security || [];
      const requestBody = operation.requestBody || null;
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
                ? renderContentBlock(requestBody.content, spec)
                : '<span class="muted">None</span>'}
            </div>
            <div>
              <h3 class="section-title">Responses</h3>
              ${renderResponses(operation.responses, spec)}
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
        .map(({ path, method, operation }) => renderOperation(path, method, operation, spec))
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

    let (pool, stores, _db_manager): (
        sqlx::PgPool,
        shaperail_runtime::db::StoreRegistry,
        Option<shaperail_runtime::db::DatabaseManager>,
    ) = if let Some(ref dbs) = config.databases {
        let default_url = dbs
            .get("default")
            .ok_or_else(|| io_error("databases: config must include a 'default' connection"))?
            .url
            .clone();
        let pool = sqlx::PgPool::connect(&default_url)
            .await
            .map_err(|e| io_error(format!(
                "Failed to connect to default database: {e}. Run `docker compose up -d` to start the preconfigured Postgres and Redis services."
            )))?;
        let migrator = sqlx::migrate::Migrator::new(Path::new("./migrations"))
            .await
            .map_err(|e| io_error(format!("Failed to load migrations: {e}")))?;
        migrator
            .run(&pool)
            .await
            .map_err(|e| io_error(format!("Failed to apply migrations: {e}")))?;
        tracing::info!("Connected to database(s) and applied migrations (multi-DB mode)");
        let manager = shaperail_runtime::db::DatabaseManager::from_named_config(dbs)
            .await
            .map_err(|e| io_error(e.to_string()))?;
        let stores = shaperail_runtime::db::build_orm_store_registry(&manager, &resources)
            .map_err(|e| io_error(e.to_string()))?;
        (pool, stores, Some(manager))
    } else {
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
        let stores = generated::build_store_registry(pool.clone());
        (pool, stores, None)
    };

    let redis_pool = match config.cache.as_ref() {
        Some(cache_config) => Some(Arc::new(
            create_redis_pool(&cache_config.url)
                .map_err(|e| io_error(format!("Failed to create Redis pool: {e}")))?,
        )),
        None => None,
    };
    let cache = redis_pool.as_ref().map(|pool| RedisCache::new(pool.clone()));
    let channels = load_channels(std::path::Path::new("channels/"));
    let ws_pubsub = redis_pool
        .as_ref()
        .map(|pool| RedisPubSub::new(pool.clone()));
    let room_manager = if channels.is_empty() {
        None
    } else {
        Some(RoomManager::new())
    };
    let job_queue = redis_pool.as_ref().map(|pool| JobQueue::new(pool.clone()));
    let event_emitter = job_queue
        .clone()
        .map(|queue| EventEmitter::new(queue, config.events.as_ref()));

    let job_registry = generated::build_job_registry();
    let _worker_shutdown_tx = if !job_registry.is_empty() {
        if let Some(ref jq) = job_queue {
            let (tx, shutdown_rx) = tokio::sync::watch::channel(false);
            let worker = Worker::new(
                jq.clone(),
                job_registry,
                std::time::Duration::from_secs(1),
            );
            let _worker_handle = worker.spawn(shutdown_rx);
            Some(tx)
        } else {
            None
        }
    } else {
        None
    };

    let jwt_config = JwtConfig::from_env().map(Arc::new);

    let port: u16 = std::env::var("SHAPERAIL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(config.port);

    let metrics_state = web::Data::new(
        MetricsState::new().map_err(|e| io_error(format!("Failed to initialize metrics: {e}")))?,
    );
    let controllers = generated::build_controller_map();

    let state = Arc::new(AppState {
        pool: pool.clone(),
        resources: resources.clone(),
        stores: Some(stores),
        controllers: Some(controllers),
        jwt_config: jwt_config.clone(),
        cache,
        event_emitter,
        job_queue,
        metrics: Some(metrics_state.get_ref().clone()),
        #[cfg(feature = "wasm-plugins")]
        wasm_runtime: None,
        event_bus: tokio::sync::broadcast::channel(256).0,
    });
    let health_state = web::Data::new(HealthState::new(Some(pool), redis_pool));

    tracing::info!("Starting Shaperail server on port {port}");
    tracing::info!("OpenAPI spec available at http://localhost:{port}/openapi.json");
    tracing::info!("API docs available at http://localhost:{port}/docs");

    let state_clone = state.clone();
    let resources_clone = resources.clone();
    let health_state_clone = health_state.clone();
    let metrics_state_clone = metrics_state.clone();
    let jwt_config_clone = jwt_config.clone();
    let openapi_json_clone = openapi_json.clone();
    let channels_clone = channels.clone();
    let ws_pubsub_clone = ws_pubsub.clone();
    let room_manager_clone = room_manager.clone();

    // GraphQL (M15) — only available when the "graphql" feature is enabled
    #[cfg(feature = "graphql")]
    let graphql_schema = if config.protocols.iter().any(|p| p == "graphql") {
        Some(
            shaperail_runtime::graphql::build_schema(&resources, state.clone())
                .map_err(|e| io_error(e.to_string()))?,
        )
    } else {
        None
    };
    #[cfg(feature = "graphql")]
    let graphql_schema_clone = graphql_schema.clone();

    // gRPC server (M16) — only available when the "grpc" feature is enabled
    #[cfg(feature = "grpc")]
    if config.protocols.iter().any(|p| p == "grpc") {
        let grpc_config = config.grpc.as_ref();
        let grpc_port = grpc_config.map(|c| c.port).unwrap_or(50051);
        let _grpc_handle = shaperail_runtime::grpc::build_grpc_server(
            state.clone(),
            resources.clone(),
            jwt_config.clone(),
            grpc_config,
        )
        .await
        .map_err(|e| io_error(e.to_string()))?;
        tracing::info!("gRPC server listening on port {grpc_port}");
    }

    HttpServer::new(move || {
        let st = state_clone.clone();
        let res = resources_clone.clone();
        let spec = openapi_json_clone.clone();
        let sensitive = sensitive_fields(&res);
        let mut app = App::new()
            .wrap(RequestLogger::new(sensitive))
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
        #[cfg(feature = "graphql")]
        if let Some(ref schema) = graphql_schema_clone {
            app = app
                .app_data(web::Data::new(schema.clone()))
                .route("/graphql", web::post().to(shaperail_runtime::graphql::graphql_handler))
                .route("/graphql/playground", web::get().to(shaperail_runtime::graphql::playground_handler));
        }
        let ch = channels_clone.clone();
        let pubsub = ws_pubsub_clone.clone();
        let rm = room_manager_clone.clone();
        let jwt_ws = jwt_config_clone.clone();
        app.configure(move |cfg| {
            register_all_resources(cfg, &res, st);
            if let (Some(ref p), Some(ref r), Some(ref j)) = (&pubsub, &rm, &jwt_ws) {
                for channel in &ch {
                    shaperail_runtime::ws::configure_ws_routes(
                        cfg,
                        channel.clone(),
                        r.clone(),
                        p.clone(),
                        j.clone(),
                    );
                }
            }
        })
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
"###;
    write_file(&root.join("src/main.rs"), main_rs)?;

    let build_rs = r###"use std::env;
use std::io;
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        panic!("{error}");
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-changed=migrations");

    let database_url = database_url_from_env_or_dotenv()?;
    let Some(database_url) = database_url else {
        return Ok(());
    };

    println!("cargo:rustc-env=DATABASE_URL={database_url}");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let migrator = sqlx::migrate::Migrator::new(Path::new("./migrations")).await?;
        let pool = sqlx::PgPool::connect(&database_url).await.map_err(|error| {
            io::Error::other(format!(
                "Failed to connect to DATABASE_URL during build-time SQL verification: {error}. Run `docker compose up -d` before building."
            ))
        })?;
        migrator.run(&pool).await.map_err(|error| {
            io::Error::other(format!(
                "Failed to apply migrations during build-time SQL verification: {error}"
            ))
        })?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    Ok(())
}

fn database_url_from_env_or_dotenv() -> Result<Option<String>, Box<dyn std::error::Error>> {
    if let Ok(database_url) = env::var("DATABASE_URL") {
        return Ok(Some(database_url));
    }

    let iter = match dotenvy::from_path_iter(".env") {
        Ok(iter) => iter,
        Err(_) => return Ok(None),
    };

    for entry in iter {
        let (key, value) = entry?;
        if key == "DATABASE_URL" {
            return Ok(Some(value));
        }
    }

    Ok(None)
}
"###;
    write_file(&root.join("build.rs"), build_rs)?;

    // Example resource file — uses convention-based defaults (method/path inferred)
    let example_resource = r#"# yaml-language-server: $schema=https://shaperail.dev/schema/resource.v1.json
resource: posts
version: 1

schema:
  id:         { type: uuid, primary: true, generated: true }
  title:      { type: string, min: 1, max: 500, required: true }
  body:       { type: string, required: true }
  author_id:  { type: uuid, required: true }
  published:  { type: boolean, default: false }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }

# Convention-based defaults: for list/get/create/update/delete,
# method and path are inferred automatically from the resource name.
# You can still override them explicitly if needed.
endpoints:
  list:
    auth: public
    filters: [author_id, published]
    search: [title, body]
    pagination: cursor
    sort: [created_at, title]

  get:
    auth: public

  create:
    auth: [admin, member]
    input: [title, body, author_id, published]

  update:
    auth: [admin, owner]
    input: [title, body, published]

  delete:
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

    // Write JSON Schema for resource validation (used by yaml-language-server)
    let json_schema = shaperail_codegen::json_schema::render_json_schema();
    write_file(&root.join("resources/.schema.json"), &json_schema)?;

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

    let resources = super::load_all_resources_from(&root.join("resources"))?;
    super::generate::write_generated_modules(&resources, &root.join("generated"))?;

    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

fn shaperail_dependency(crate_name: &str) -> String {
    shaperail_dependency_with_options(crate_name, true)
}

fn shaperail_dependency_no_defaults(crate_name: &str) -> String {
    shaperail_dependency_with_options(crate_name, false)
}

fn shaperail_dependency_with_options(crate_name: &str, default_features: bool) -> String {
    let df = if default_features {
        String::new()
    } else {
        ", default-features = false".to_string()
    };
    match std::env::var(DEV_WORKSPACE_ENV) {
        Ok(workspace_root) if !workspace_root.is_empty() => {
            let path = Path::new(&workspace_root).join(crate_name);
            let path = path.to_string_lossy().replace('\\', "\\\\");
            format!(r#"{{ version = "{SHAPERAIL_VERSION}", path = "{path}"{df} }}"#)
        }
        _ => format!(r#"{{ version = "{SHAPERAIL_VERSION}"{df} }}"#),
    }
}
