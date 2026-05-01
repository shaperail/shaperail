use std::env;
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
