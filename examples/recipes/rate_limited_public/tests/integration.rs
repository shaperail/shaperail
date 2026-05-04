//! Integration tests for the rate_limited_public recipe.
//!
//! Tests are marked `#[ignore]` because they require a Redis-backed rate limiter
//! running in the server, which is not available in the current test-support setup.
//!
//! The current TestServer API (`shaperail-runtime/src/test_support.rs`) provides:
//! - `spawn_with_listener(listener, factory)` — spawn a server
//! - `server.url(path)` — build a URL
//! - `server.address()` / `server.port()` / `server.shutdown()`
//!
//! It does not provide:
//! - `.post(path).json(...).send()` method shortcuts
//! - Redis setup/teardown
//! - Request-count assertions
//!
//! TODO: un-ignore once Redis is wired into the test harness.
//!
//! Compile-check: `cargo check -p recipe-rate-limited-public --tests`

#[cfg(test)]
mod tests {
    /// Asserts that the sixth request within the 60-second window returns 429.
    ///
    /// The first five requests must succeed (2xx); the sixth must be rejected
    /// with 429 Too Many Requests.
    ///
    /// Ignored: requires Redis-backed rate limiting in test harness and
    /// TestServer method shortcuts not yet in test_support.
    #[tokio::test]
    #[ignore = "TODO: requires Redis in test harness and TestServer method shortcuts; tracking in shaperail-runtime/src/test_support.rs"]
    async fn rate_limit_kicks_in_at_sixth_request() {
        // When the helpers land, the test body will look roughly like:
        //
        // let server = TestServer::start().await.unwrap();
        // let payload = serde_json::json!({
        //     "email": "test@example.com",
        //     "name": "Test User",
        //     "message": "Hello from the integration test"
        // });
        //
        // // First 5 requests from the same IP must succeed
        // for i in 1..=5 {
        //     let resp = server
        //         .post("/v1/contact_requests")
        //         .json(&payload)
        //         .send()
        //         .await
        //         .unwrap();
        //     assert!(
        //         resp.status().is_success(),
        //         "request {i} should succeed, got {}",
        //         resp.status()
        //     );
        // }
        //
        // // 6th request must be rate-limited
        // let resp = server
        //     .post("/v1/contact_requests")
        //     .json(&payload)
        //     .send()
        //     .await
        //     .unwrap();
        // assert_eq!(
        //     resp.status().as_u16(),
        //     429,
        //     "6th request in 60s window must return 429 Too Many Requests"
        // );
        todo!("implement once Redis is wired into the test harness")
    }

    /// Asserts that a public create request without authentication succeeds (2xx).
    ///
    /// Ignored: requires TestServer method shortcuts not yet in test_support.
    #[tokio::test]
    #[ignore = "TODO: requires TestServer method shortcuts; tracking in shaperail-runtime/src/test_support.rs"]
    async fn public_create_requires_no_auth() {
        // When the helpers land, the test body will look roughly like:
        //
        // let server = TestServer::start().await.unwrap();
        // // No .with_role() — unauthenticated request
        // let resp = server
        //     .post("/v1/contact_requests")
        //     .json(&serde_json::json!({
        //         "email": "anon@example.com",
        //         "name": "Anonymous",
        //         "message": "Public message"
        //     }))
        //     .send()
        //     .await
        //     .unwrap();
        // assert!(resp.status().is_success(), "public create must not require auth");
        todo!("implement once TestServer method shortcuts are available")
    }
}
