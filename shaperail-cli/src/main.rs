mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(
    name = "shaperail",
    about = "Shaperail — AI-Native Rust Backend Framework",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold a new Shaperail project
    Init {
        /// Project name
        name: String,
    },
    /// Run codegen for all resource files
    Generate,
    /// Start dev server with hot reload
    Serve {
        /// Port override
        #[arg(short, long)]
        port: Option<u16>,
        /// Validate the project and print the resolved serve command without starting it
        #[arg(long)]
        check: bool,
        /// Start all services declared in shaperail.workspace.yaml
        #[arg(long)]
        workspace: bool,
    },
    /// Build release binary
    Build {
        /// Generate Dockerfile and build Docker image
        #[arg(long)]
        docker: bool,
    },
    /// Validate all resource files
    Validate {
        /// Path to a resource file or directory of resource files
        #[arg(default_value = "resources")]
        path: PathBuf,
    },
    /// Run generated and custom tests
    Test {
        /// Additional arguments passed to cargo test
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Generate and apply SQL migrations from resource files
    Migrate {
        /// Rollback the last migration batch
        #[arg(long)]
        rollback: bool,
    },
    /// Load fixture YAML files into the database
    Seed {
        /// Path to seed data directory
        #[arg(default_value = "seeds")]
        path: PathBuf,
    },
    /// Export OpenAPI spec or SDK
    Export {
        #[command(subcommand)]
        format: ExportFormat,
    },
    /// Dry-run: show what a resource file will produce (routes, table, relations)
    Explain {
        /// Path to a resource YAML file
        path: PathBuf,
    },
    /// Validate with structured fix suggestions (JSON output for LLM consumption)
    Check {
        /// Path to a resource file or directory of resource files
        #[arg(default_value = "resources")]
        path: PathBuf,
        /// Output as JSON (machine-readable fix suggestions)
        #[arg(long)]
        json: bool,
    },
    /// Show what codegen would change (dry-run diff)
    Diff,
    /// Check system dependencies
    Doctor,
    /// Print all routes with auth requirements
    Routes,
    /// Show job queue depth and recent failures
    #[command(name = "jobs:status")]
    JobsStatus {
        /// Optional job ID to inspect
        job_id: Option<String>,
    },
    /// Dump a project-aware context summary for LLM consumption
    #[command(name = "llm-context")]
    LlmContext {
        /// Filter to a single resource by name
        #[arg(short, long)]
        resource: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Manage resource files
    Resource {
        #[command(subcommand)]
        action: ResourceAction,
    },
}

#[derive(Subcommand)]
enum ResourceAction {
    /// Scaffold a new resource YAML file and initial migration
    Create {
        /// Resource name (plural, lowercase, e.g., "blog_posts")
        name: String,
        /// Archetype template: basic (default), user, content, tenant, lookup
        #[arg(short, long, default_value = "basic")]
        archetype: String,
    },
}

#[derive(Subcommand)]
enum ExportFormat {
    /// Output OpenAPI 3.1 spec
    Openapi {
        /// Write to file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate client SDK
    Sdk {
        /// Target language (e.g., ts)
        #[arg(short, long)]
        lang: String,
        /// Output directory
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Output JSON Schema for resource YAML files
    JsonSchema {
        /// Write to file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Commands::Init { name } => commands::init::run(&name),
        Commands::Generate => commands::generate::run(),
        Commands::Serve {
            port,
            check,
            workspace,
        } => {
            if workspace {
                commands::workspace::run_serve()
            } else {
                commands::serve::run(port, check)
            }
        }
        Commands::Build { docker } => commands::build::run(docker),
        Commands::Validate { path } => commands::validate::run(&path),
        Commands::Test { args } => commands::test::run(&args),
        Commands::Migrate { rollback } => commands::migrate::run(rollback),
        Commands::Seed { path } => commands::seed::run(&path),
        Commands::Export { format } => match format {
            ExportFormat::Openapi { output } => commands::export::run_openapi(output.as_deref()),
            ExportFormat::Sdk { lang, output } => {
                commands::export::run_sdk(&lang, output.as_deref())
            }
            ExportFormat::JsonSchema { output } => {
                commands::export::run_json_schema(output.as_deref())
            }
        },
        Commands::Diff => commands::diff::run(),
        Commands::Explain { path } => commands::explain::run(&path),
        Commands::Check { path, json } => commands::check::run(&path, json),
        Commands::Doctor => commands::doctor::run(),
        Commands::Routes => commands::routes::run(),
        Commands::JobsStatus { job_id } => commands::jobs_status::run(job_id.as_deref()),
        Commands::LlmContext { resource, json } => {
            commands::llm_context::run(resource.as_deref(), json)
        }
        Commands::Resource { action } => match action {
            ResourceAction::Create { name, archetype } => {
                commands::resource::run_create(&name, &archetype)
            }
        },
    };

    process::exit(exit_code);
}
