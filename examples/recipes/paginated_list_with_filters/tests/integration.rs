//! Integration tests for the paginated_list_with_filters recipe.
//!
//! These tests require a running server built from the generated `orders` resource.
//! They are marked `#[ignore]` because:
//!
//! 1. `TestServer` (shaperail-runtime test-support) provides `spawn_with_listener`
//!    but no built-in role/auth helpers (`.with_role()`, `.with_role_and_org()`)
//!    or method shortcuts (`.get(path).send()`). Those helpers are not yet
//!    implemented in `shaperail-runtime/src/test_support.rs`.
//!
//! 2. Tests that exercise auth-guarded endpoints need a JWT or session token
//!    that carries the caller's role. Generating one requires access to the
//!    signing key and the configured auth middleware — both live in the generated
//!    application binary, not in the test-support library.
//!
//! TODO: un-ignore once `TestServer::with_role(role)` lands in test_support.rs.
//!
//! Compile-check: `cargo check -p recipe-paginated-list-with-filters --tests`

#[cfg(test)]
mod tests {
    /// Asserts that the orders list endpoint returns HTTP 200 for an authenticated
    /// caller with the `admin` role.
    ///
    /// Ignored: requires `TestServer::with_role` helper not yet in test_support.
    #[tokio::test]
    #[ignore = "TODO: requires TestServer::with_role helper; tracking in shaperail-runtime/src/test_support.rs"]
    async fn list_returns_200_for_admin() {
        // When the helper lands, the test body will look roughly like:
        //
        // let server = TestServer::start().await.unwrap();
        // let resp = server
        //     .with_role("admin")
        //     .get("/v1/orders")
        //     .send()
        //     .await
        //     .unwrap();
        // assert_eq!(resp.status().as_u16(), 200);
        todo!("implement once TestServer::with_role is available")
    }

    /// Asserts that the orders list endpoint rejects unauthenticated callers
    /// with 401 or 403.
    ///
    /// Ignored: requires a running server with auth middleware wired up.
    #[tokio::test]
    #[ignore = "TODO: requires TestServer::with_role helper; tracking in shaperail-runtime/src/test_support.rs"]
    async fn unauthenticated_list_is_401_or_403() {
        // When the helper lands, the test body will look roughly like:
        //
        // let server = TestServer::start().await.unwrap();
        // let resp = reqwest::get(server.url("/v1/orders")).await.unwrap();
        // let status = resp.status().as_u16();
        // assert!(
        //     status == 401 || status == 403,
        //     "expected 401 or 403, got {status}"
        // );
        todo!("implement once TestServer::with_role is available")
    }
}
