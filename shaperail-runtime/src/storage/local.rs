use super::backend::{FileMetadata, StorageError};
use std::path::PathBuf;

/// Local filesystem storage backend (default for development).
///
/// Files are stored under `SHAPERAIL_STORAGE_LOCAL_DIR` (default: `./uploads`).
pub struct LocalStorage {
    root: PathBuf,
}

impl LocalStorage {
    /// Create a new local storage backend at the given root directory.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Create from env var `SHAPERAIL_STORAGE_LOCAL_DIR`, defaulting to `./uploads`.
    pub fn from_env() -> Self {
        let dir = std::env::var("SHAPERAIL_STORAGE_LOCAL_DIR")
            .unwrap_or_else(|_| "./uploads".to_string());
        Self::new(PathBuf::from(dir))
    }

    fn full_path(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }

    pub async fn upload(
        &self,
        path: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<FileMetadata, StorageError> {
        let full = self.full_path(path);
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::Backend(format!("Failed to create directory: {e}")))?;
        }
        tokio::fs::write(&full, data)
            .await
            .map_err(|e| StorageError::Backend(format!("Failed to write file: {e}")))?;

        let filename = full
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        Ok(FileMetadata {
            path: path.to_string(),
            filename,
            mime_type: mime_type.to_string(),
            size: data.len() as u64,
        })
    }

    pub async fn download(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        let full = self.full_path(path);
        tokio::fs::read(&full).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(path.to_string())
            } else {
                StorageError::Backend(format!("Failed to read file: {e}"))
            }
        })
    }

    pub async fn delete(&self, path: &str) -> Result<(), StorageError> {
        let full = self.full_path(path);
        tokio::fs::remove_file(&full).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(path.to_string())
            } else {
                StorageError::Backend(format!("Failed to delete file: {e}"))
            }
        })
    }

    /// For local storage, signed URLs return a `file://` path.
    /// In production, use S3/GCS/Azure for proper signed URLs.
    pub async fn signed_url(&self, path: &str, _expires_secs: u64) -> Result<String, StorageError> {
        let full = self.full_path(path);
        if !full.exists() {
            return Err(StorageError::NotFound(path.to_string()));
        }
        Ok(format!("file://{}", full.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_storage() -> (LocalStorage, TempDir) {
        let dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(dir.path().to_path_buf());
        (storage, dir)
    }

    #[tokio::test]
    async fn upload_and_download() {
        let (storage, _dir) = test_storage();
        let data = b"hello world";

        let meta = storage
            .upload("test/file.txt", data, "text/plain")
            .await
            .unwrap();

        assert_eq!(meta.path, "test/file.txt");
        assert_eq!(meta.filename, "file.txt");
        assert_eq!(meta.mime_type, "text/plain");
        assert_eq!(meta.size, 11);

        let downloaded = storage.download("test/file.txt").await.unwrap();
        assert_eq!(downloaded, data);
    }

    #[tokio::test]
    async fn delete_file() {
        let (storage, _dir) = test_storage();

        storage
            .upload("to_delete.txt", b"data", "text/plain")
            .await
            .unwrap();

        storage.delete("to_delete.txt").await.unwrap();

        let result = storage.download("to_delete.txt").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn download_not_found() {
        let (storage, _dir) = test_storage();
        let result = storage.download("nonexistent.txt").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn signed_url_local() {
        let (storage, _dir) = test_storage();

        storage
            .upload("signed.txt", b"data", "text/plain")
            .await
            .unwrap();

        let url = storage.signed_url("signed.txt", 3600).await.unwrap();
        assert!(url.starts_with("file://"));
    }

    #[tokio::test]
    async fn signed_url_not_found() {
        let (storage, _dir) = test_storage();
        let result = storage.signed_url("missing.txt", 3600).await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }
}
