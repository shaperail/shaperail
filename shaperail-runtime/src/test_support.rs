//! In-process server spawning helpers for integration tests.
//!
//! Enabled via the `test-support` cargo feature.
//!
//! # Example
//!
//! ```no_run
//! # use std::net::TcpListener;
//! # async fn run() -> std::io::Result<()> {
//! // Provide a factory that builds your `actix_web::dev::Server` from a listener.
//! let listener = TcpListener::bind("127.0.0.1:0")?;
//! let server = shaperail_runtime::test_support::spawn_with_listener(
//!     listener,
//!     |listener| {
//!         // Replace with your project's `build_server(listener)`.
//!         unimplemented!()
//!     },
//! )
//! .await?;
//! // Hit it via reqwest, etc.
//! drop(server); // shuts the server down
//! # Ok(()) }
//! ```

use std::net::{SocketAddr, TcpListener};

use actix_web::dev::Server;

/// Handle to an in-process server bound to an ephemeral port.
///
/// Drops the underlying spawn handle on drop, terminating the server.
pub struct TestServer {
    addr: SocketAddr,
    handle: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
}

impl TestServer {
    /// Bound socket address (host + port).
    pub fn address(&self) -> SocketAddr {
        self.addr
    }

    /// Bound port.
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Convenience helper: build `http://<addr><path>`.
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    /// Aborts the spawned server task and returns its result if it had already finished.
    pub async fn shutdown(mut self) -> std::io::Result<()> {
        let Some(handle) = self.handle.take() else {
            return Ok(());
        };
        handle.abort();
        match handle.await {
            Ok(res) => res,
            Err(join_err) if join_err.is_cancelled() => Ok(()),
            Err(join_err) => Err(std::io::Error::other(join_err.to_string())),
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

/// Spawns the server returned by `factory` on `listener` and returns a `TestServer`.
///
/// The factory closure receives the listener (consumed) and must return the configured
/// `actix_web::dev::Server`. Typical usage: pass your project's `build_server(listener)`
/// function directly.
pub async fn spawn_with_listener<F>(
    listener: TcpListener,
    factory: F,
) -> std::io::Result<TestServer>
where
    F: FnOnce(TcpListener) -> std::io::Result<Server> + Send + 'static,
{
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;
    let server = factory(listener)?;
    let handle = tokio::spawn(server);
    Ok(TestServer {
        addr,
        handle: Some(handle),
    })
}

/// Runs database migrations exactly once per process, regardless of how many tests
/// invoke this helper concurrently.
///
/// Tests running in parallel typically all want migrations applied before any of
/// them touches the database. Calling `sqlx::migrate!()` from each test wastes
/// time and serializes on the migration advisory lock; calling it once via this
/// helper is O(1) for every subsequent caller.
///
/// The migration source is the workspace `migrations/` directory, the same
/// directory the runtime uses in production.
pub async fn ensure_migrations_run(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    use tokio::sync::OnceCell;
    static MIGRATED: OnceCell<()> = OnceCell::const_new();
    MIGRATED
        .get_or_try_init(|| async {
            sqlx::migrate!("../migrations").run(pool).await?;
            Ok::<_, sqlx::Error>(())
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{web, App, HttpResponse, HttpServer};

    fn trivial_factory(listener: TcpListener) -> std::io::Result<Server> {
        let server = HttpServer::new(|| {
            App::new().route(
                "/health",
                web::get().to(|| async { HttpResponse::Ok().finish() }),
            )
        })
        .listen(listener)?
        .run();
        Ok(server)
    }

    #[tokio::test]
    async fn spawn_with_listener_returns_bound_address() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let original_port = listener.local_addr().unwrap().port();
        let server = spawn_with_listener(listener, trivial_factory)
            .await
            .unwrap();
        assert_eq!(server.port(), original_port);
        assert!(server.url("/health").starts_with("http://127.0.0.1:"));
    }

    #[tokio::test]
    async fn spawn_with_listener_serves_requests() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = spawn_with_listener(listener, trivial_factory)
            .await
            .unwrap();
        let resp = reqwest::get(server.url("/health")).await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
    }
}
