//! Integration tests for the multi_tenant_rls recipe.
//!
//! Tests are marked `#[ignore]` because:
//!
//! 1. `TestServer::with_role_and_org(role, org_id)` does not yet exist in
//!    `shaperail-runtime/src/test_support.rs`. This helper would attach a JWT
//!    with the given role and org_id to all requests made through that handle.
//!
//! 2. Without org-scoped auth, we cannot demonstrate the 404-not-403 cross-
//!    tenant invariant in an automated test.
//!
//! The current TestServer API only provides:
//! - `spawn_with_listener(listener, factory)` — spawn a server
//! - `server.url(path)` — build a URL
//! - `server.address()` / `server.port()` / `server.shutdown()`
//!
//! TODO: un-ignore once `TestServer::with_role_and_org` lands in test_support.rs.
//!
//! Compile-check: `cargo check -p recipe-multi-tenant-rls --tests`

#[cfg(test)]
mod tests {
    /// Asserts that a caller from org A receives 404 when accessing a document
    /// that belongs to org B — not 403, which would leak the record's existence.
    ///
    /// This is the core tenant-isolation invariant enforced by `tenant_key: org_id`.
    ///
    /// Ignored: requires `TestServer::with_role_and_org` helper not yet in test_support.
    #[tokio::test]
    #[ignore = "TODO: requires TestServer::with_role_and_org helper; tracking in shaperail-runtime/src/test_support.rs"]
    async fn cross_tenant_access_is_invisible() {
        // When the helpers land, the test body will look roughly like:
        //
        // let org_a = Uuid::new_v4();
        // let org_b = Uuid::new_v4();
        //
        // let server = TestServer::start().await.unwrap();
        //
        // // Create a document in org_b
        // let doc_id = server
        //     .with_role_and_org("member", org_b)
        //     .post("/v1/documents")
        //     .json(&serde_json::json!({ "title": "Org B Doc", "body": "secret", "created_by": Uuid::new_v4() }))
        //     .send()
        //     .await
        //     .unwrap()
        //     .json::<serde_json::Value>()
        //     .await
        //     .unwrap()["id"]
        //     .as_str()
        //     .unwrap()
        //     .to_string();
        //
        // // Org A caller tries to access org B's document — must get 404, not 403
        // let resp = server
        //     .with_role_and_org("member", org_a)
        //     .get(&format!("/v1/documents/{doc_id}"))
        //     .send()
        //     .await
        //     .unwrap();
        //
        // assert_eq!(
        //     resp.status().as_u16(),
        //     404,
        //     "cross-tenant access must return 404, not 403 — leaking record existence is forbidden"
        // );
        todo!("implement once TestServer::with_role_and_org is available")
    }

    /// Asserts that a tenant's list endpoint only returns that tenant's documents.
    ///
    /// Ignored: requires `TestServer::with_role_and_org` helper not yet in test_support.
    #[tokio::test]
    #[ignore = "TODO: requires TestServer::with_role_and_org helper; tracking in shaperail-runtime/src/test_support.rs"]
    async fn list_is_scoped_to_caller_org() {
        // When the helpers land, the test body will look roughly like:
        //
        // let server = TestServer::start().await.unwrap();
        // let org_a = Uuid::new_v4();
        // let org_b = Uuid::new_v4();
        //
        // // Create one doc in each org
        // server.with_role_and_org("member", org_a)
        //     .post("/v1/documents")
        //     .json(&serde_json::json!({ "title": "Org A Doc", "body": "a", "created_by": Uuid::new_v4() }))
        //     .send().await.unwrap();
        // server.with_role_and_org("member", org_b)
        //     .post("/v1/documents")
        //     .json(&serde_json::json!({ "title": "Org B Doc", "body": "b", "created_by": Uuid::new_v4() }))
        //     .send().await.unwrap();
        //
        // // Org A caller should only see org A's document
        // let list = server
        //     .with_role_and_org("member", org_a)
        //     .get("/v1/documents")
        //     .send().await.unwrap()
        //     .json::<serde_json::Value>().await.unwrap();
        //
        // let items = list["data"].as_array().unwrap();
        // assert_eq!(items.len(), 1);
        // assert_eq!(items[0]["title"].as_str().unwrap(), "Org A Doc");
        todo!("implement once TestServer::with_role_and_org is available")
    }
}
