# Recipe: File Upload

## WHEN to use this

Use this recipe when you need **multipart/form-data file uploads** with server-enforced size and MIME-type validation:
- Users upload files (images, PDFs, documents) that are stored in object storage (S3, GCS, Azure Blob, or local).
- You need to reject oversized uploads (413) or disallowed MIME types (415) before the file reaches your storage backend.
- The upload result should be a persisted record (attachment) with metadata queryable via list/get.

## What this gives you

```
GET    /v1/attachments              → list (member, admin)
GET    /v1/attachments/:id          → get (member, admin, owner)
POST   /v1/attachments              → multipart upload (member, admin) — max 10 MB, PNG/JPEG/PDF only
DELETE /v1/attachments/:id          → delete (admin, owner)
```

### Upload configuration

```yaml
upload: { field: file, storage: s3, max_size: 10mb, types: [image/png, image/jpeg, application/pdf] }
```

- `field`: the schema field of type `file` that receives the uploaded URL.
- `storage`: one of `local`, `s3`, `gcs`, `azure`. Configured via environment at runtime.
- `max_size`: string with unit (`10mb`, `5mb`, `100kb`). The runtime enforces this before hitting the storage backend; oversized requests receive **413 Payload Too Large**.
- `types`: MIME type allowlist. Requests with a disallowed `Content-Type` on the part receive **415 Unsupported Media Type**.

### Why `method: POST` is explicit here

The `upload:` key requires the endpoint method to be POST, PATCH, or PUT. For the `create` action, POST is the default — but the validator checks the method *before* applying defaults, so you must declare `method: POST` explicitly when using `upload:` on a create endpoint to avoid a parse error.

### Companion fields

When a schema has a `type: file` field named `file`, the runtime expects companion fields `filename` (string), `content_type` (string), and `size_bytes` (integer) to be writable from the multipart form. Include them in `input:` so the runtime can populate them from the multipart headers automatically.

## When NOT to use this

- **Large binary processing** (video transcoding, ML inference): use a background job that reads from storage; don't block the HTTP handler.
- **Resumable uploads**: the `upload:` key generates a single-shot multipart endpoint. For resumable uploads (chunked), write a custom handler.
- **Base64-encoded files in JSON**: don't. Use `type: file` + multipart. Base64 inflates payload size by ~33% and bypasses the runtime's size/type checks.
- **Storing files in the database**: the `type: file` field stores a URL to the storage backend, not the raw bytes. Use a managed storage backend; don't store binary in Postgres.

## Key design notes for LLM authors

1. The `file` field is `type: file` in the schema — not `type: string`. This is important: the validator checks that the `upload.field` reference points at a `file`-typed field, and will reject the resource if it doesn't.
2. `uploader_id` is in `input:` so the caller provides it. If you want the runtime to inject it from the authenticated user's subject claim, use a `before:` controller to set `ctx.input["uploader_id"]` from `ctx.subject`.
3. `owner` in `get.auth` and `delete.auth` is a Shaperail built-in role sentinel that resolves to the record's owner at runtime. It is not a role you declare in your auth config.
4. The `deleted_at` field is not needed here because `delete` does not declare `soft_delete: true`. A hard delete is appropriate for attachments — the object storage record should also be removed (via an `after:` controller or a cleanup job).
