use std::process::Command;

/// Run generated and custom tests via cargo test.
pub fn run(args: &[String]) -> i32 {
    println!("Running tests...");

    let mut cmd = Command::new("cargo");
    cmd.arg("test");

    for arg in args {
        cmd.arg(arg);
    }

    match cmd.status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("Failed to run cargo test: {e}");
            1
        }
    }
}
