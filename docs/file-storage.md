---
title: File storage
parent: Guides
nav_order: 8
---

# File storage

Shaperail provides built-in file upload, storage, and retrieval backed by the
`object_store` crate. Files are validated on upload, stored through a pluggable
backend, and tracked as metadata in your database.

## Schema: declaring file fields

Use `type: file` on any schema field that should hold a stored file reference:

```yaml
resource: avatars
version: 1

schema:
  id:         { type: uuid, primary: true, generated: true }
  user_id:    { type: uuid, ref: users.id, required: true }
  avatar_url: { type: file, required: true }
  created_at: { type: timestamp, generated: true }
```

A `file` field stores the storage path string in the database. The actual binary
data lives in the configured storage backend.

## Upload configuration on endpoints

Add an `upload:` block to any endpoint that accepts file uploads:

```yaml
endpoints:
  create:
    method: POST
    path: /avatars
    auth: [member, admin]
    input: [user_id, avatar_url]
    upload:
      field: avatar_url
      storage: s3
      max_size: 5mb
      types: [jpg, png, webp]
```

| Key | Required | Description |
|-----|----------|-------------|
| `field` | Yes | Schema field that stores the file reference. |
| `storage` | Yes | Backend name: `s3`, `gcs`, `azure`, or `local`. |
| `max_size` | Yes | Maximum upload size. Accepts `kb`, `mb`, `gb`, or plain bytes (e.g., `5mb`, `100kb`, `1gb`, `1024`). |
| `types` | No | Allowed file types. Accepts extensions (`jpg`, `png`, `pdf`), full MIME types (`image/png`, `application/pdf`), or wildcards (`image/*`). Omit to allow all types. |

When a request exceeds `max_size` or sends a disallowed MIME type, Shaperail
returns a `422 Validation` error with a structured `FieldError` identifying the
problem.

## Storage backends

Shaperail supports four backends. All implement the same interface: `upload`,
`download`, `delete`, and `signed_url`.

| Backend | Value | Use case |
|---------|-------|----------|
| Local filesystem | `local` | Development and testing. Default when no env var is set. |
| Amazon S3 | `s3` | Production object storage on AWS. |
| Google Cloud Storage | `gcs` | Production object storage on GCP. |
| Azure Blob Storage | `azure` | Production object storage on Azure. |

## Backend selection

Set the `SHAPERAIL_STORAGE_BACKEND` environment variable:

```bash
# Development (default if unset)
SHAPERAIL_STORAGE_BACKEND=local

# Production examples
SHAPERAIL_STORAGE_BACKEND=s3
SHAPERAIL_STORAGE_BACKEND=gcs
SHAPERAIL_STORAGE_BACKEND=azure
```

The backend is resolved once at startup via `StorageBackend::from_env()`. An
unrecognized value causes an immediate startup error.

## Configuration in shaperail.config.yaml

Configure backend-specific settings in the `storage` section of your project
config:

```yaml
storage:
  backend: s3
  local:
    root_dir: ./uploads
  s3:
    bucket: my-app-uploads
    region: us-east-1
    # Credentials from AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY env vars
  gcs:
    bucket: my-app-uploads
    # Credentials from GOOGLE_APPLICATION_CREDENTIALS env var
  azure:
    container: my-app-uploads
    account: mystorageaccount
    # Credentials from AZURE_STORAGE_KEY env var
```

The `backend` key in the config file is overridden by
`SHAPERAIL_STORAGE_BACKEND` if set, allowing per-environment control without
changing the config file.

## Signed URL generation

Generate time-limited download URLs for stored files:

```rust
let url = upload_handler.signed_url("uploads/abc-photo.jpg", 3600).await?;
// Returns a pre-signed URL valid for 3600 seconds
```

All backends support signed URLs. The `local` backend returns `file://` URLs
(useful for development). S3, GCS, and Azure return standard pre-signed HTTPS
URLs.

The `expires_secs` parameter controls how long the URL remains valid.

## File metadata in the database

Every uploaded file produces a `FileMetadata` record with four fields:

| Column | Type | Description |
|--------|------|-------------|
| `path` | `String` | Storage key relative to the backend root (e.g., `uploads/uuid-filename.jpg`). |
| `filename` | `String` | Original filename as uploaded by the client. |
| `mime_type` | `String` | Detected MIME type (e.g., `image/png`). |
| `size` | `u64` | File size in bytes. |

The `path` value is what gets stored in the schema field marked `type: file`.
Use it to generate signed URLs or perform storage operations later.

Filenames are sanitized on upload: only alphanumeric characters, dots, hyphens,
and underscores are kept. Everything else is replaced with `_`. Directory
traversal attempts like `../../../etc/passwd` become `.._.._.._etc_passwd`.

## Orphan cleanup on resource deletion

When a resource with file fields is deleted, Shaperail automatically deletes
the associated files from storage. The `UploadHandler::delete` method removes
the file at the stored path:

```rust
upload_handler.delete(&file_metadata.path).await?;
```

For soft-deleted resources (`soft_delete: true`), files are retained until the
record is permanently purged. This ensures soft-deleted records can still be
restored with their files intact.

## Image processing

Shaperail includes built-in image processing for resize and thumbnail
generation, powered by the `image` crate.

### Thumbnails

Generate a thumbnail that fits within the given dimensions while preserving
aspect ratio:

```rust
let thumb = upload_handler.create_thumbnail(
    "uploads/uuid-photo.jpg",  // original file path
    200,                        // max width
    200,                        // max height
    "thumbnails",              // storage prefix for output
).await?;
// thumb.path => "thumbnails/thumb-<uuid>.png"
```

### Resizing

Resize an image to specific dimensions using Lanczos3 filtering:

```rust
let resized = upload_handler.resize_image(
    "uploads/uuid-photo.jpg",  // original file path
    800,                        // target width
    600,                        // target height
    "resized",                 // storage prefix for output
).await?;
// resized.path => "resized/resized-<uuid>.png"
```

Both operations download the original from storage, process it in memory, and
upload the result as a new PNG file. Each returns a `FileMetadata` with the new
path, size, and MIME type.
