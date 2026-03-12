use serde::{Deserialize, Serialize};
use std::fmt;

/// Metadata associated with a stored file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Storage path (relative key within the backend).
    pub path: String,
    /// Original filename as uploaded.
    pub filename: String,
    /// MIME type (e.g., "image/png").
    pub mime_type: String,
    /// File size in bytes.
    pub size: u64,
}

/// Errors that can occur during storage operations.
#[derive(Debug)]
pub enum StorageError {
    /// The requested file was not found.
    NotFound(String),
    /// File exceeds the maximum allowed size.
    FileTooLarge { max_bytes: u64, actual_bytes: u64 },
    /// MIME type is not in the allowed list.
    InvalidMimeType {
        mime_type: String,
        allowed: Vec<String>,
    },
    /// Backend I/O or configuration error.
    Backend(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "File not found: {path}"),
            Self::FileTooLarge {
                max_bytes,
                actual_bytes,
            } => {
                write!(
                    f,
                    "File too large: {actual_bytes} bytes exceeds limit of {max_bytes} bytes"
                )
            }
            Self::InvalidMimeType { mime_type, allowed } => {
                write!(f, "Invalid MIME type '{mime_type}', allowed: {allowed:?}")
            }
            Self::Backend(msg) => write!(f, "Storage backend error: {msg}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<StorageError> for shaperail_core::ShaperailError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::NotFound(_) => shaperail_core::ShaperailError::NotFound,
            StorageError::FileTooLarge {
                max_bytes,
                actual_bytes,
            } => shaperail_core::ShaperailError::Validation(vec![shaperail_core::FieldError {
                field: "file".to_string(),
                message: format!(
                    "File too large: {actual_bytes} bytes exceeds limit of {max_bytes} bytes"
                ),
                code: "file_too_large".to_string(),
            }]),
            StorageError::InvalidMimeType { mime_type, allowed } => {
                shaperail_core::ShaperailError::Validation(vec![shaperail_core::FieldError {
                    field: "file".to_string(),
                    message: format!("Invalid MIME type '{mime_type}', allowed: {allowed:?}"),
                    code: "invalid_mime_type".to_string(),
                }])
            }
            StorageError::Backend(msg) => shaperail_core::ShaperailError::Internal(msg),
        }
    }
}

/// Storage backend selected via `SHAPERAIL_STORAGE_BACKEND` env var.
///
/// Uses enum dispatch to avoid async trait object complexity.
pub enum StorageBackend {
    Local(super::LocalStorage),
    S3(super::S3Storage),
    Gcs(super::GcsStorage),
    Azure(super::AzureStorage),
}

impl StorageBackend {
    /// Create a storage backend from the `SHAPERAIL_STORAGE_BACKEND` env var.
    ///
    /// Supported values: `local`, `s3`, `gcs`, `azure`.
    /// Defaults to `local` if not set.
    pub fn from_env() -> Result<Self, StorageError> {
        let backend =
            std::env::var("SHAPERAIL_STORAGE_BACKEND").unwrap_or_else(|_| "local".to_string());
        match backend.as_str() {
            "local" => Ok(Self::Local(super::LocalStorage::from_env())),
            "s3" => super::S3Storage::from_env().map(Self::S3),
            "gcs" => super::GcsStorage::from_env().map(Self::Gcs),
            "azure" => super::AzureStorage::from_env().map(Self::Azure),
            other => Err(StorageError::Backend(format!(
                "Unknown storage backend: '{other}'. Supported: local, s3, gcs, azure"
            ))),
        }
    }

    /// Upload file data to storage under the given `path`.
    pub async fn upload(
        &self,
        path: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<FileMetadata, StorageError> {
        match self {
            Self::Local(s) => s.upload(path, data, mime_type).await,
            Self::S3(s) => s.upload(path, data, mime_type).await,
            Self::Gcs(s) => s.upload(path, data, mime_type).await,
            Self::Azure(s) => s.upload(path, data, mime_type).await,
        }
    }

    /// Download file data from storage at the given `path`.
    pub async fn download(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        match self {
            Self::Local(s) => s.download(path).await,
            Self::S3(s) => s.download(path).await,
            Self::Gcs(s) => s.download(path).await,
            Self::Azure(s) => s.download(path).await,
        }
    }

    /// Delete a file from storage at the given `path`.
    pub async fn delete(&self, path: &str) -> Result<(), StorageError> {
        match self {
            Self::Local(s) => s.delete(path).await,
            Self::S3(s) => s.delete(path).await,
            Self::Gcs(s) => s.delete(path).await,
            Self::Azure(s) => s.delete(path).await,
        }
    }

    /// Generate a time-limited signed URL for downloading the file.
    pub async fn signed_url(&self, path: &str, expires_secs: u64) -> Result<String, StorageError> {
        match self {
            Self::Local(s) => s.signed_url(path, expires_secs).await,
            Self::S3(s) => s.signed_url(path, expires_secs).await,
            Self::Gcs(s) => s.signed_url(path, expires_secs).await,
            Self::Azure(s) => s.signed_url(path, expires_secs).await,
        }
    }
}
