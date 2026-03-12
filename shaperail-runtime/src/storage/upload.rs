use super::backend::{FileMetadata, StorageBackend, StorageError};
use std::sync::Arc;

/// Parses a human-readable size string (e.g., "5mb", "100kb") into bytes.
pub fn parse_max_size(size_str: &str) -> Result<u64, StorageError> {
    let s = size_str.trim().to_lowercase();
    let (num_part, multiplier) = if let Some(n) = s.strip_suffix("gb") {
        (n, 1024 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("mb") {
        (n, 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("kb") {
        (n, 1024)
    } else if let Some(n) = s.strip_suffix('b') {
        (n, 1)
    } else {
        // Assume bytes if no suffix
        (s.as_str(), 1)
    };

    num_part
        .trim()
        .parse::<u64>()
        .map(|n| n * multiplier)
        .map_err(|_| StorageError::Backend(format!("Invalid size format: '{size_str}'")))
}

/// Validates a MIME type against an allowed list of extensions/types.
///
/// The `allowed` list can contain:
/// - Extensions: "jpg", "png", "pdf"
/// - MIME types: "image/png", "application/pdf"
/// - Wildcards: "image/*"
pub fn validate_mime_type(mime_type: &str, allowed: &[String]) -> Result<(), StorageError> {
    if allowed.is_empty() {
        return Ok(());
    }

    for pattern in allowed {
        // Direct MIME type match
        if pattern == mime_type {
            return Ok(());
        }
        // Wildcard match (e.g., "image/*")
        if let Some(prefix) = pattern.strip_suffix("/*") {
            if mime_type.starts_with(prefix) {
                return Ok(());
            }
        }
        // Extension-based match
        let ext_mime = extension_to_mime(pattern);
        if ext_mime == mime_type {
            return Ok(());
        }
    }

    Err(StorageError::InvalidMimeType {
        mime_type: mime_type.to_string(),
        allowed: allowed.to_vec(),
    })
}

/// Maps common file extensions to MIME types.
fn extension_to_mime(ext: &str) -> &str {
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "csv" => "text/csv",
        "txt" => "text/plain",
        "zip" => "application/zip",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        _ => "",
    }
}

/// Handles file uploads with validation, storage, and optional image processing.
pub struct UploadHandler {
    backend: Arc<StorageBackend>,
}

impl UploadHandler {
    /// Create a new upload handler with the given storage backend.
    pub fn new(backend: Arc<StorageBackend>) -> Self {
        Self { backend }
    }

    /// Process an upload: validate size and MIME type, store the file.
    ///
    /// Returns the file metadata on success.
    pub async fn process_upload(
        &self,
        filename: &str,
        data: &[u8],
        mime_type: &str,
        max_size: Option<u64>,
        allowed_types: Option<&[String]>,
        storage_prefix: &str,
    ) -> Result<FileMetadata, StorageError> {
        // Validate file size
        if let Some(max) = max_size {
            if data.len() as u64 > max {
                return Err(StorageError::FileTooLarge {
                    max_bytes: max,
                    actual_bytes: data.len() as u64,
                });
            }
        }

        // Validate MIME type
        if let Some(types) = allowed_types {
            validate_mime_type(mime_type, types)?;
        }

        // Generate storage path: prefix/uuid-filename
        let file_id = uuid::Uuid::new_v4();
        let safe_filename = sanitize_filename(filename);
        let path = format!("{storage_prefix}/{file_id}-{safe_filename}");

        self.backend.upload(&path, data, mime_type).await
    }

    /// Generate a thumbnail for an image file.
    ///
    /// Returns the metadata of the stored thumbnail.
    pub async fn create_thumbnail(
        &self,
        original_path: &str,
        max_width: u32,
        max_height: u32,
        storage_prefix: &str,
    ) -> Result<FileMetadata, StorageError> {
        let data = self.backend.download(original_path).await?;

        let img = image::load_from_memory(&data)
            .map_err(|e| StorageError::Backend(format!("Failed to decode image: {e}")))?;

        let thumbnail = img.thumbnail(max_width, max_height);

        let mut buf = std::io::Cursor::new(Vec::new());
        thumbnail
            .write_to(&mut buf, image::ImageFormat::Png)
            .map_err(|e| StorageError::Backend(format!("Failed to encode thumbnail: {e}")))?;
        let thumb_data = buf.into_inner();

        let file_id = uuid::Uuid::new_v4();
        let thumb_path = format!("{storage_prefix}/thumb-{file_id}.png");

        self.backend
            .upload(&thumb_path, &thumb_data, "image/png")
            .await
    }

    /// Resize an image to fit within the given dimensions.
    pub async fn resize_image(
        &self,
        original_path: &str,
        width: u32,
        height: u32,
        storage_prefix: &str,
    ) -> Result<FileMetadata, StorageError> {
        let data = self.backend.download(original_path).await?;

        let img = image::load_from_memory(&data)
            .map_err(|e| StorageError::Backend(format!("Failed to decode image: {e}")))?;

        let resized = img.resize(width, height, image::imageops::FilterType::Lanczos3);

        let mut buf = std::io::Cursor::new(Vec::new());
        resized
            .write_to(&mut buf, image::ImageFormat::Png)
            .map_err(|e| StorageError::Backend(format!("Failed to encode resized image: {e}")))?;
        let resized_data = buf.into_inner();

        let file_id = uuid::Uuid::new_v4();
        let resized_path = format!("{storage_prefix}/resized-{file_id}.png");

        self.backend
            .upload(&resized_path, &resized_data, "image/png")
            .await
    }

    /// Generate a time-limited signed URL for a file.
    pub async fn signed_url(&self, path: &str, expires_secs: u64) -> Result<String, StorageError> {
        self.backend.signed_url(path, expires_secs).await
    }

    /// Delete a file from storage (used for orphan cleanup).
    pub async fn delete(&self, path: &str) -> Result<(), StorageError> {
        self.backend.delete(path).await
    }

    /// Returns a reference to the underlying storage backend.
    pub fn backend(&self) -> &StorageBackend {
        &self.backend
    }
}

/// Sanitize a filename to prevent directory traversal and other issues.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_mb() {
        assert_eq!(parse_max_size("5mb").unwrap(), 5 * 1024 * 1024);
    }

    #[test]
    fn parse_size_kb() {
        assert_eq!(parse_max_size("100kb").unwrap(), 100 * 1024);
    }

    #[test]
    fn parse_size_gb() {
        assert_eq!(parse_max_size("1gb").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_size_bytes() {
        assert_eq!(parse_max_size("1024b").unwrap(), 1024);
        assert_eq!(parse_max_size("512").unwrap(), 512);
    }

    #[test]
    fn parse_size_invalid() {
        assert!(parse_max_size("abc").is_err());
    }

    #[test]
    fn validate_mime_extension_match() {
        let allowed = vec!["jpg".to_string(), "png".to_string()];
        assert!(validate_mime_type("image/jpeg", &allowed).is_ok());
        assert!(validate_mime_type("image/png", &allowed).is_ok());
        assert!(validate_mime_type("application/pdf", &allowed).is_err());
    }

    #[test]
    fn validate_mime_wildcard() {
        let allowed = vec!["image/*".to_string()];
        assert!(validate_mime_type("image/jpeg", &allowed).is_ok());
        assert!(validate_mime_type("image/png", &allowed).is_ok());
        assert!(validate_mime_type("application/pdf", &allowed).is_err());
    }

    #[test]
    fn validate_mime_direct_match() {
        let allowed = vec!["application/pdf".to_string()];
        assert!(validate_mime_type("application/pdf", &allowed).is_ok());
        assert!(validate_mime_type("image/png", &allowed).is_err());
    }

    #[test]
    fn validate_mime_empty_allows_all() {
        assert!(validate_mime_type("anything/here", &[]).is_ok());
    }

    #[test]
    fn sanitize_filename_safe() {
        assert_eq!(sanitize_filename("file.txt"), "file.txt");
        assert_eq!(sanitize_filename("my-file_v2.jpg"), "my-file_v2.jpg");
    }

    #[test]
    fn sanitize_filename_unsafe() {
        assert_eq!(
            sanitize_filename("../../../etc/passwd"),
            ".._.._.._etc_passwd"
        );
        assert_eq!(sanitize_filename("file name.txt"), "file_name.txt");
    }

    #[tokio::test]
    async fn upload_handler_process() {
        let dir = tempfile::TempDir::new().unwrap();
        let local = super::super::LocalStorage::new(dir.path().to_path_buf());
        let backend = Arc::new(StorageBackend::Local(local));
        let handler = UploadHandler::new(backend);

        let meta = handler
            .process_upload(
                "test.txt",
                b"hello world",
                "text/plain",
                Some(1024 * 1024),
                None,
                "uploads",
            )
            .await
            .unwrap();

        assert_eq!(meta.mime_type, "text/plain");
        assert_eq!(meta.size, 11);
        assert!(meta.path.starts_with("uploads/"));
    }

    #[tokio::test]
    async fn upload_handler_rejects_too_large() {
        let dir = tempfile::TempDir::new().unwrap();
        let local = super::super::LocalStorage::new(dir.path().to_path_buf());
        let backend = Arc::new(StorageBackend::Local(local));
        let handler = UploadHandler::new(backend);

        let result = handler
            .process_upload(
                "big.txt",
                &[0u8; 2000],
                "text/plain",
                Some(1000),
                None,
                "uploads",
            )
            .await;

        assert!(matches!(result, Err(StorageError::FileTooLarge { .. })));
    }

    #[tokio::test]
    async fn upload_handler_rejects_invalid_mime() {
        let dir = tempfile::TempDir::new().unwrap();
        let local = super::super::LocalStorage::new(dir.path().to_path_buf());
        let backend = Arc::new(StorageBackend::Local(local));
        let handler = UploadHandler::new(backend);

        let allowed = vec!["jpg".to_string(), "png".to_string()];
        let result = handler
            .process_upload(
                "doc.pdf",
                b"pdf data",
                "application/pdf",
                None,
                Some(&allowed),
                "uploads",
            )
            .await;

        assert!(matches!(result, Err(StorageError::InvalidMimeType { .. })));
    }

    #[tokio::test]
    async fn upload_handler_signed_url() {
        let dir = tempfile::TempDir::new().unwrap();
        let local = super::super::LocalStorage::new(dir.path().to_path_buf());
        let backend = Arc::new(StorageBackend::Local(local));
        let handler = UploadHandler::new(backend);

        let meta = handler
            .process_upload(
                "sign_test.txt",
                b"data",
                "text/plain",
                None,
                None,
                "uploads",
            )
            .await
            .unwrap();

        let url = handler.signed_url(&meta.path, 3600).await.unwrap();
        assert!(url.starts_with("file://"));
    }

    #[tokio::test]
    async fn upload_handler_delete() {
        let dir = tempfile::TempDir::new().unwrap();
        let local = super::super::LocalStorage::new(dir.path().to_path_buf());
        let backend = Arc::new(StorageBackend::Local(local));
        let handler = UploadHandler::new(backend);

        let meta = handler
            .process_upload(
                "delete_me.txt",
                b"data",
                "text/plain",
                None,
                None,
                "uploads",
            )
            .await
            .unwrap();

        handler.delete(&meta.path).await.unwrap();

        // Downloading after delete should fail
        let result = handler.backend().download(&meta.path).await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }
}
