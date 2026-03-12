use std::process::Command;

/// Check system dependencies and report status.
pub fn run() -> i32 {
    println!("Shaperail Doctor");
    println!("===============");
    println!();

    let mut all_ok = true;

    // Check Rust
    all_ok &= check_command(
        "Rust compiler",
        "rustc",
        &["--version"],
        "Install Rust: https://rustup.rs/",
    );

    // Check Cargo
    all_ok &= check_command(
        "Cargo",
        "cargo",
        &["--version"],
        "Install Rust: https://rustup.rs/",
    );

    // Check Docker (required for the default local workflow)
    all_ok &= check_command(
        "Docker",
        "docker",
        &["--version"],
        "Install Docker: https://docs.docker.com/get-docker/",
    );

    // Local service CLIs are optional because docker compose provides Postgres + Redis.
    let postgres_ok = check_command(
        "PostgreSQL (psql, optional)",
        "psql",
        &["--version"],
        "Install PostgreSQL: https://www.postgresql.org/download/",
    );
    if !postgres_ok {
        println!("  (Optional: useful for manual database inspection)");
        println!();
    }

    let redis_ok = check_command(
        "Redis (redis-cli, optional)",
        "redis-cli",
        &["--version"],
        "Install Redis: https://redis.io/download/",
    );
    if !redis_ok {
        println!("  (Optional: useful for manual cache inspection)");
        println!();
    }

    // sqlx-cli is only needed for the explicit `shaperail migrate` workflow.
    let sqlx_ok = check_command(
        "sqlx-cli (optional)",
        "sqlx",
        &["--version"],
        "Install: cargo install sqlx-cli",
    );
    if !sqlx_ok {
        println!("  (Optional: required only for `shaperail migrate`)");
        println!();
    }

    // Check cargo-watch (optional)
    let watch_ok = check_cargo_subcommand("cargo-watch (optional)", "watch");
    if !watch_ok {
        println!("  (cargo-watch is optional, enables hot reload with `shaperail serve`)");
        println!("  Install: cargo install cargo-watch");
        println!();
    }

    println!();
    if all_ok {
        println!("All required dependencies found.");
        0
    } else {
        println!("Some required dependencies are missing. See instructions above.");
        1
    }
}

fn check_command(name: &str, cmd: &str, args: &[&str], fix: &str) -> bool {
    match Command::new(cmd).args(args).output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let version = version.trim();
            println!("\u{2713} {name}: {version}");
            true
        }
        _ => {
            println!("\u{2717} {name}: NOT FOUND");
            println!("  {fix}");
            println!();
            false
        }
    }
}

fn check_cargo_subcommand(name: &str, subcmd: &str) -> bool {
    match Command::new("cargo").args([subcmd, "--version"]).output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let version = version.trim();
            println!("\u{2713} {name}: {version}");
            true
        }
        _ => {
            println!("\u{2717} {name}: NOT FOUND");
            false
        }
    }
}
