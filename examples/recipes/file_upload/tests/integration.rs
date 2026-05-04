//! Integration tests for the file_upload recipe.
//!
//! All tests are marked `#[ignore]` because they require multipart HTTP helpers
//! that are not yet implemented in `shaperail-runtime/src/test_support.rs`.
//!
//! Specifically, these tests need:
//! - `TestServer::post_multipart(path)` — send a multipart/form-data request
//! - `.file_part(field, bytes, mime)` — attach a file part
//! - `.form_field(name, value)` — attach a text field part
//!
//! None of these exist in the current TestServer API, which only exposes:
//! - `spawn_with_listener(listener, factory)` — spawn a server
//! - `server.url(path)` — build a URL
//! - `server.address()` / `server.port()` / `server.shutdown()`
//!
//! TODO: un-ignore once multipart helpers land in test_support.rs.
//!
//! Compile-check: `cargo check -p recipe-file-upload --tests`

#[cfg(test)]
mod tests {
    /// Asserts that uploading a file exceeding the 10 MB limit returns 413.
    ///
    /// Ignored: requires TestServer multipart helpers not yet in test_support.
    #[tokio::test]
    #[ignore = "TODO: requires TestServer::post_multipart helper; tracking in shaperail-runtime/src/test_support.rs"]
    async fn oversized_upload_returns_413() {
        // When the helpers land, the test body will look roughly like:
        //
        // let server = TestServer::start().await.unwrap();
        // let big_file = vec![0u8; 11 * 1024 * 1024]; // 11 MB
        // let resp = server
        //     .with_role("member")
        //     .post_multipart("/v1/attachments")
        //     .file_part("file", big_file, "image/png")
        //     .form_field("filename", "huge.png")
        //     .send()
        //     .await
        //     .unwrap();
        // assert_eq!(resp.status().as_u16(), 413);
        todo!("implement once TestServer::post_multipart is available")
    }

    /// Asserts that uploading a disallowed MIME type returns 415.
    ///
    /// Ignored: requires TestServer multipart helpers not yet in test_support.
    #[tokio::test]
    #[ignore = "TODO: requires TestServer::post_multipart helper; tracking in shaperail-runtime/src/test_support.rs"]
    async fn disallowed_mime_type_returns_415() {
        // When the helpers land, the test body will look roughly like:
        //
        // let server = TestServer::start().await.unwrap();
        // let gif_bytes = b"GIF89a\x01\x00\x01\x00\x00\x00\x00\x3b".to_vec();
        // let resp = server
        //     .with_role("member")
        //     .post_multipart("/v1/attachments")
        //     .file_part("file", gif_bytes, "image/gif") // not in allowlist
        //     .form_field("filename", "animation.gif")
        //     .send()
        //     .await
        //     .unwrap();
        // assert_eq!(resp.status().as_u16(), 415);
        todo!("implement once TestServer::post_multipart is available")
    }
}
