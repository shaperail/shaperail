use super::backend::{FileMetadata, StorageError};
use object_store::path::Path as ObjectPath;
use object_store::signer::Signer;
use object_store::{ObjectStore, PutPayload};
use std::time::Duration;

/// Helper: upload via any ObjectStore impl.
async fn do_upload(
    store: &dyn ObjectStore,
    path: &str,
    data: &[u8],
    mime_type: &str,
) -> Result<FileMetadata, StorageError> {
    let obj_path = ObjectPath::from(path);
    let payload = PutPayload::from(data.to_vec());
    store
        .put(&obj_path, payload)
        .await
        .map_err(|e| StorageError::Backend(format!("Upload failed: {e}")))?;

    let filename = path.rsplit('/').next().unwrap_or(path).to_string();

    Ok(FileMetadata {
        path: path.to_string(),
        filename,
        mime_type: mime_type.to_string(),
        size: data.len() as u64,
    })
}

/// Helper: download via any ObjectStore impl.
async fn do_download(store: &dyn ObjectStore, path: &str) -> Result<Vec<u8>, StorageError> {
    let obj_path = ObjectPath::from(path);
    let result = store.get(&obj_path).await.map_err(|e| {
        if e.to_string().contains("not found") || e.to_string().contains("404") {
            StorageError::NotFound(path.to_string())
        } else {
            StorageError::Backend(format!("Download failed: {e}"))
        }
    })?;
    result
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| StorageError::Backend(format!("Failed to read bytes: {e}")))
}

/// Helper: delete via any ObjectStore impl.
async fn do_delete(store: &dyn ObjectStore, path: &str) -> Result<(), StorageError> {
    let obj_path = ObjectPath::from(path);
    store.delete(&obj_path).await.map_err(|e| {
        if e.to_string().contains("not found") || e.to_string().contains("404") {
            StorageError::NotFound(path.to_string())
        } else {
            StorageError::Backend(format!("Delete failed: {e}"))
        }
    })
}

/// Helper: signed URL via any Signer impl, with fallback to base_url.
async fn do_signed_url(
    signer: &dyn Signer,
    path: &str,
    expires_secs: u64,
    base_url: &str,
) -> Result<String, StorageError> {
    let obj_path = ObjectPath::from(path);
    let duration = Duration::from_secs(expires_secs);
    match signer
        .signed_url(http::Method::GET, &obj_path, duration)
        .await
    {
        Ok(url) => Ok(url.to_string()),
        Err(_) => Ok(format!("{}/{}", base_url.trim_end_matches('/'), path)),
    }
}

/// Amazon S3 storage backend via the `object_store` crate.
///
/// Configured via environment variables:
/// - `AWS_ACCESS_KEY_ID`
/// - `AWS_SECRET_ACCESS_KEY`
/// - `AWS_DEFAULT_REGION` or `SHAPERAIL_STORAGE_REGION`
/// - `SHAPERAIL_STORAGE_BUCKET`
pub struct S3Storage {
    store: object_store::aws::AmazonS3,
    base_url: String,
}

impl S3Storage {
    /// Create from environment variables.
    pub fn from_env() -> Result<Self, StorageError> {
        let bucket = std::env::var("SHAPERAIL_STORAGE_BUCKET").map_err(|_| {
            StorageError::Backend("SHAPERAIL_STORAGE_BUCKET env var required for S3".to_string())
        })?;
        let region = std::env::var("SHAPERAIL_STORAGE_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|_| "us-east-1".to_string());

        let store = object_store::aws::AmazonS3Builder::from_env()
            .with_bucket_name(&bucket)
            .with_region(&region)
            .build()
            .map_err(|e| StorageError::Backend(format!("Failed to build S3 client: {e}")))?;

        let base_url = format!("https://{bucket}.s3.{region}.amazonaws.com");

        Ok(Self { store, base_url })
    }

    pub async fn upload(
        &self,
        path: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<FileMetadata, StorageError> {
        do_upload(&self.store, path, data, mime_type).await
    }

    pub async fn download(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        do_download(&self.store, path).await
    }

    pub async fn delete(&self, path: &str) -> Result<(), StorageError> {
        do_delete(&self.store, path).await
    }

    pub async fn signed_url(&self, path: &str, expires_secs: u64) -> Result<String, StorageError> {
        do_signed_url(&self.store, path, expires_secs, &self.base_url).await
    }
}

/// Google Cloud Storage backend via the `object_store` crate.
///
/// Configured via environment variables:
/// - `GOOGLE_SERVICE_ACCOUNT` or `GOOGLE_APPLICATION_CREDENTIALS`
/// - `SHAPERAIL_STORAGE_BUCKET`
pub struct GcsStorage {
    store: object_store::gcp::GoogleCloudStorage,
    base_url: String,
}

impl GcsStorage {
    /// Create from environment variables.
    pub fn from_env() -> Result<Self, StorageError> {
        let bucket = std::env::var("SHAPERAIL_STORAGE_BUCKET").map_err(|_| {
            StorageError::Backend("SHAPERAIL_STORAGE_BUCKET env var required for GCS".to_string())
        })?;

        let store = object_store::gcp::GoogleCloudStorageBuilder::from_env()
            .with_bucket_name(&bucket)
            .build()
            .map_err(|e| StorageError::Backend(format!("Failed to build GCS client: {e}")))?;

        let base_url = format!("https://storage.googleapis.com/{bucket}");

        Ok(Self { store, base_url })
    }

    pub async fn upload(
        &self,
        path: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<FileMetadata, StorageError> {
        do_upload(&self.store, path, data, mime_type).await
    }

    pub async fn download(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        do_download(&self.store, path).await
    }

    pub async fn delete(&self, path: &str) -> Result<(), StorageError> {
        do_delete(&self.store, path).await
    }

    pub async fn signed_url(&self, path: &str, expires_secs: u64) -> Result<String, StorageError> {
        do_signed_url(&self.store, path, expires_secs, &self.base_url).await
    }
}

/// Azure Blob Storage backend via the `object_store` crate.
///
/// Configured via environment variables:
/// - `AZURE_STORAGE_ACCOUNT_NAME`
/// - `AZURE_STORAGE_ACCESS_KEY`
/// - `SHAPERAIL_STORAGE_BUCKET` (container name)
pub struct AzureStorage {
    store: object_store::azure::MicrosoftAzure,
    base_url: String,
}

impl AzureStorage {
    /// Create from environment variables.
    pub fn from_env() -> Result<Self, StorageError> {
        let container = std::env::var("SHAPERAIL_STORAGE_BUCKET").map_err(|_| {
            StorageError::Backend("SHAPERAIL_STORAGE_BUCKET env var required for Azure".to_string())
        })?;
        let account = std::env::var("AZURE_STORAGE_ACCOUNT_NAME")
            .unwrap_or_else(|_| "devstoreaccount1".to_string());

        let store = object_store::azure::MicrosoftAzureBuilder::from_env()
            .with_container_name(&container)
            .build()
            .map_err(|e| StorageError::Backend(format!("Failed to build Azure client: {e}")))?;

        let base_url = format!("https://{account}.blob.core.windows.net/{container}");

        Ok(Self { store, base_url })
    }

    pub async fn upload(
        &self,
        path: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<FileMetadata, StorageError> {
        do_upload(&self.store, path, data, mime_type).await
    }

    pub async fn download(&self, path: &str) -> Result<Vec<u8>, StorageError> {
        do_download(&self.store, path).await
    }

    pub async fn delete(&self, path: &str) -> Result<(), StorageError> {
        do_delete(&self.store, path).await
    }

    pub async fn signed_url(&self, path: &str, expires_secs: u64) -> Result<String, StorageError> {
        do_signed_url(&self.store, path, expires_secs, &self.base_url).await
    }
}
