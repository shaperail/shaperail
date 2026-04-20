---
title: Blog API example
parent: Examples
nav_order: 1
---

This example shows the files a Shaperail user actually authors:

- `resources/*.yaml`
- `resources/*.controller.rs` by convention for controller modules
- `migrations/*.sql`
- `shaperail.config.yaml`
- `.env`
- `docker-compose.yml`

Current limitation: controller modules still need manual registration in the
controller map; they are not auto-discovered by the scaffolded app today.

You can find the source files in the repository under
`examples/blog-api/`, but this page explains the full example without sending
you out of the docs site.

## What the example covers

- versioned API endpoints (`/v1/posts`, `/v1/comments`)
- public blog post reads
- protected post creation with a before-controller (`prepare_post`)
- protected post updates with business-rule enforcement (`enforce_edit_rules`)
- post-deletion bookkeeping with an after-controller (`cleanup_comments`)
- comment creation with post-status gating, XSS stripping, and rate limiting (`validate_comment`)
- comment update with ownership and 15-minute edit window enforcement (`check_comment_ownership`)
- owner-based post and comment updates through `created_by`
- post/comment relations
- cursor pagination on posts
- offset pagination on comments
- soft delete on posts
- single-database setup; for multi-database use `databases:` in config and optional `db:` on each resource (see [Configuration reference]({{ '/configuration/' | relative_url }}#databases-multi-database))

## Quick start

```bash
shaperail init blog-api
cd blog-api
docker compose up -d
shaperail serve
```

Then replace the scaffolded files with the example files from the repo if you
want the exact sample project layout.

## Posts resource

```yaml
resource: posts
version: 1
# db: default   # optional; omit for default connection (see multi-DB in docs)

schema:
  id:           { type: uuid, primary: true, generated: true }
  title:        { type: string, min: 1, max: 200, required: true }
  slug:         { type: string, min: 1, max: 200, required: true, unique: true }
  body:         { type: string, required: true }
  status:       { type: enum, values: [draft, published, archived], default: draft }
  created_by:   { type: uuid, required: true }
  published_at: { type: timestamp, nullable: true }
  deleted_at:   { type: timestamp, nullable: true }
  created_at:   { type: timestamp, generated: true }
  updated_at:   { type: timestamp, generated: true }

endpoints:
  list:
    auth: public
    filters: [status, created_by]
    search: [title, body]
    pagination: cursor
    sort: [created_at, title]

  get:
    auth: public

  create:
    auth: [admin, member]
    input: [title, slug, body, status, created_by, published_at]
    controller:
      before: prepare_post

  update:
    auth: [admin, owner]
    input: [title, body, status, published_at]
    controller:
      before: enforce_edit_rules

  delete:
    auth: [admin]
    soft_delete: true
    controller:
      after: cleanup_comments

relations:
  comments: { resource: comments, type: has_many, foreign_key: post_id }

indexes:
  - { fields: [slug], unique: true }
  - { fields: [created_at], order: desc }
```

This resource demonstrates:

- convention-based defaults: CRUD endpoints omit `method:` and `path:` because the framework infers them from the action name
- public read endpoints (served at `/v1/posts` thanks to `version: 1`)
- owner-aware updates through `created_by`
- cursor pagination
- soft delete on the delete route
- three controllers covering create, update, and delete

## Posts controllers

The file `resources/posts.controller.rs` contains three functions referenced by the YAML above.

```rust
use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

/// Before-controller for **create**: prepares a new post for insertion.
///
/// 1. Auto-fills `created_by` from the authenticated user's JWT.
/// 2. Generates a URL-safe `slug` from the title (lowercase, hyphens, no special chars).
/// 3. Defaults `status` to `"draft"` when the client omits it.
/// 4. Validates that `body` is not empty or whitespace-only.
pub async fn prepare_post(ctx: &mut Context) -> ControllerResult {
    // --- 1. Auto-fill created_by from JWT ---
    let user = ctx
        .user
        .as_ref()
        .ok_or(ShaperailError::Unauthorized)?;

    ctx.input.insert(
        "created_by".into(),
        serde_json::json!(user.id),
    );

    // --- 2. Generate slug from title ---
    let title = ctx
        .input
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-");

    if slug.is_empty() {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "title".into(),
            message: "Title must produce a non-empty slug".into(),
            code: "invalid_title".into(),
        }]));
    }

    ctx.input.insert("slug".into(), serde_json::json!(slug));

    // --- 3. Default status to "draft" ---
    if !ctx.input.contains_key("status") {
        ctx.input.insert("status".into(), serde_json::json!("draft"));
    }

    // --- 4. Validate body is not empty/whitespace ---
    let body = ctx
        .input
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if body.trim().is_empty() {
        return Err(ShaperailError::Validation(vec![FieldError {
            field: "body".into(),
            message: "Post body cannot be empty".into(),
            code: "required".into(),
        }]));
    }

    Ok(())
}

/// Before-controller for **update**: enforces editing rules on existing posts.
///
/// 1. Only draft or published posts can be edited (not archived).
/// 2. Non-admin users cannot change `status` to `"published"`.
/// 3. Changing from published to draft requires an `X-Edit-Reason` header.
/// 4. Auto-updates `slug` when the title changes.
pub async fn enforce_edit_rules(ctx: &mut Context) -> ControllerResult {
    let user = ctx
        .user
        .as_ref()
        .ok_or(ShaperailError::Unauthorized)?;

    let is_admin = user.role == "admin";

    // Fetch the current post from the database to check its status.
    let post_id = ctx
        .input
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShaperailError::Internal("Missing post ID in update context".into()))?;

    let row = sqlx::query_as::<_, (String,)>("SELECT status FROM posts WHERE id = $1")
        .bind(post_id)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error: {e}")))?
        .ok_or(ShaperailError::NotFound)?;

    let current_status = row.0.as_str();

    // --- 1. Block edits to archived posts ---
    if current_status == "archived" {
        return Err(ShaperailError::Forbidden);
    }

    // --- 2. Non-admins cannot publish ---
    if let Some(new_status) = ctx.input.get("status").and_then(|v| v.as_str()) {
        if new_status == "published" && !is_admin {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "status".into(),
                message: "Only admins can set status to published".into(),
                code: "forbidden_status".into(),
            }]));
        }

        // --- 3. Published -> draft requires a reason header ---
        if current_status == "published" && new_status == "draft" {
            if !ctx.headers.contains_key("x-edit-reason") {
                return Err(ShaperailError::Validation(vec![FieldError {
                    field: "status".into(),
                    message: "Reverting a published post to draft requires an X-Edit-Reason header".into(),
                    code: "reason_required".into(),
                }]));
            }
        }
    }

    // --- 4. Auto-update slug when title changes ---
    if let Some(new_title) = ctx.input.get("title").and_then(|v| v.as_str()) {
        let slug: String = new_title
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join("-");

        if !slug.is_empty() {
            ctx.input.insert("slug".into(), serde_json::json!(slug));
        }
    }

    Ok(())
}

/// After-controller for **delete**: logs orphaned comments and sets a response header.
///
/// 1. Queries the count of comments belonging to the deleted post.
/// 2. Adds an `X-Comments-Archived` response header with the count.
pub async fn cleanup_comments(ctx: &mut Context) -> ControllerResult {
    let post_id = ctx
        .data
        .as_ref()
        .and_then(|d| d.get("id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShaperailError::Internal("Missing post ID in delete context".into()))?;

    let row = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM comments WHERE post_id = $1")
        .bind(post_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error: {e}")))?;

    let comment_count = row.0;

    tracing::info!(
        post_id = post_id,
        comment_count = comment_count,
        "Post deleted; archived associated comments"
    );

    ctx.response_headers.push((
        "X-Comments-Archived".into(),
        comment_count.to_string(),
    ));

    Ok(())
}
```

**`prepare_post`** (before create) -- Prepares a new post for insertion:
- Auto-fills `created_by` from the authenticated user's JWT so the client never sends it.
- Generates a URL-safe `slug` from the title (lowercase, hyphens, special characters stripped).
- Defaults `status` to `"draft"` when the client omits it.
- Validates that `body` is not empty or whitespace-only.

**`enforce_edit_rules`** (before update) -- Guards post editing with business rules:
- Fetches the current post status from the database.
- Blocks edits to archived posts entirely (returns 403).
- Non-admin users cannot change `status` to `"published"`.
- Reverting a published post to draft requires an `X-Edit-Reason` request header.
- Auto-updates the `slug` when the title changes.

**`cleanup_comments`** (after delete) -- Post-deletion bookkeeping:
- Reads the deleted post ID from `ctx.data` (populated by the framework after the delete executes).
- Queries the count of comments that belonged to the deleted post.
- Adds an `X-Comments-Archived` response header with the count.
- Logs the post ID and comment count via `tracing::info!` with structured fields.

## Comments resource

```yaml
resource: comments
version: 1
# db: default   # optional; omit for default connection (see multi-DB in docs)

schema:
  id:         { type: uuid, primary: true, generated: true }
  post_id:    { type: uuid, ref: posts.id, required: true }
  body:       { type: string, min: 1, required: true }
  author_name: { type: string, min: 1, max: 100, required: true }
  created_by: { type: uuid, required: true }
  created_at: { type: timestamp, generated: true }
  updated_at: { type: timestamp, generated: true }

endpoints:
  list:
    auth: public
    filters: [post_id, created_by]
    pagination: offset
    sort: [created_at]

  get:
    auth: public

  create:
    auth: [admin, member]
    input: [post_id, body, author_name, created_by]
    controller:
      before: validate_comment

  update:
    auth: [admin, owner]
    input: [body]
    controller:
      before: check_comment_ownership

  delete:
    auth: [admin, owner]

relations:
  post: { resource: posts, type: belongs_to, key: post_id }

indexes:
  - { fields: [post_id] }
  - { fields: [created_at], order: desc }
```

This complements `posts` by showing:

- a `belongs_to` relationship to `posts`
- offset pagination
- owner-based updates and deletes
- controllers on create and update that enforce cross-resource validation and edit windows

## Comments controllers

The file `resources/comments.controller.rs` contains two controller functions plus a private helper.

```rust
use shaperail_core::{FieldError, ShaperailError};
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

/// Before-controller for **create**: validates a new comment before insertion.
///
/// 1. Checks that the referenced post exists and is published (not draft/archived).
/// 2. Auto-fills `created_by` from the JWT if the user is authenticated.
/// 3. Strips HTML tags from `body` as basic XSS prevention.
/// 4. Rate-limits to 10 comments per user per hour via a DB query.
pub async fn validate_comment(ctx: &mut Context) -> ControllerResult {
    let post_id = ctx
        .input
        .get("post_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ShaperailError::Validation(vec![FieldError {
                field: "post_id".into(),
                message: "post_id is required".into(),
                code: "required".into(),
            }])
        })?
        .to_owned();

    // --- 1. Verify the referenced post exists and is published ---
    let row = sqlx::query_as::<_, (String,)>("SELECT status FROM posts WHERE id = $1")
        .bind(&post_id)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error: {e}")))?;

    match row {
        None => {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "post_id".into(),
                message: "Referenced post does not exist".into(),
                code: "invalid_reference".into(),
            }]));
        }
        Some((status,)) if status != "published" => {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "post_id".into(),
                message: format!("Cannot comment on a {status} post; only published posts accept comments"),
                code: "post_not_published".into(),
            }]));
        }
        _ => {}
    }

    // --- 2. Auto-fill created_by from JWT ---
    if let Some(user) = &ctx.user {
        ctx.input.insert(
            "created_by".into(),
            serde_json::json!(user.id),
        );
    }

    // --- 3. Strip HTML tags from body (basic XSS prevention) ---
    if let Some(body) = ctx.input.get("body").and_then(|v| v.as_str()) {
        let stripped = strip_html_tags(body);
        if stripped.trim().is_empty() {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "body".into(),
                message: "Comment body cannot be empty after removing HTML".into(),
                code: "required".into(),
            }]));
        }
        ctx.input.insert("body".into(), serde_json::json!(stripped));
    }

    // --- 4. Rate limit: max 10 comments per user per hour ---
    if let Some(user) = &ctx.user {
        let user_id = user.id.clone();
        let row = sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM comments WHERE created_by = $1 AND created_at > NOW() - INTERVAL '1 hour'",
        )
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| ShaperailError::Internal(format!("DB error: {e}")))?;

        if row.0 >= 10 {
            return Err(ShaperailError::RateLimited);
        }
    }

    Ok(())
}

/// Before-controller for **update**: checks ownership and edit window.
///
/// 1. Verifies the user owns the comment OR has the admin role.
/// 2. Disallows editing comments older than 15 minutes (except for admins).
pub async fn check_comment_ownership(ctx: &mut Context) -> ControllerResult {
    let user = ctx
        .user
        .as_ref()
        .ok_or(ShaperailError::Unauthorized)?;

    let is_admin = user.role == "admin";

    let comment_id = ctx
        .input
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShaperailError::Internal("Missing comment ID in update context".into()))?;

    let row = sqlx::query_as::<_, (String, chrono::NaiveDateTime)>(
        "SELECT created_by, created_at FROM comments WHERE id = $1",
    )
    .bind(comment_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| ShaperailError::Internal(format!("DB error: {e}")))?
    .ok_or(ShaperailError::NotFound)?;

    let (owner_id, created_at) = row;

    // --- 1. Ownership check ---
    if owner_id != user.id && !is_admin {
        return Err(ShaperailError::Forbidden);
    }

    // --- 2. 15-minute edit window (admins exempt) ---
    if !is_admin {
        let now = chrono::Utc::now().naive_utc();
        let age = now - created_at;
        if age > chrono::Duration::minutes(15) {
            return Err(ShaperailError::Validation(vec![FieldError {
                field: "id".into(),
                message: "Comments can only be edited within 15 minutes of creation".into(),
                code: "edit_window_expired".into(),
            }]));
        }
    }

    Ok(())
}

/// Strips HTML tags from a string using a simple state machine.
/// This is a basic defense; production apps should use a dedicated sanitizer.
fn strip_html_tags(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut inside_tag = false;

    for ch in input.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => result.push(ch),
            _ => {}
        }
    }

    result
}
```

**`validate_comment`** (before create) -- Validates a new comment before insertion:
- Checks that the referenced post exists and has `status: published` (rejects draft and archived posts).
- Auto-fills `created_by` from the JWT if the user is authenticated.
- Strips HTML tags from the comment body using a simple state-machine parser as basic XSS prevention. If the body is empty after stripping, the request is rejected.
- Rate-limits users to 10 comments per hour via a DB count query; returns `ShaperailError::RateLimited` (HTTP 429) if exceeded.

**`check_comment_ownership`** (before update) -- Enforces ownership and edit windows:
- Fetches `created_by` and `created_at` from the database for the target comment.
- Verifies the user owns the comment or has the `admin` role; returns 403 otherwise.
- Non-admin users cannot edit comments older than 15 minutes (`edit_window_expired` error).

**`strip_html_tags`** -- A private helper that strips HTML tags using a character-by-character state machine. This is a basic defense; production apps should use a dedicated sanitizer.

## Matching migrations

The example also checks in the SQL that corresponds to the resource files.

Posts table:

```sql
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE IF NOT EXISTS "posts" (
  "id" UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  "title" VARCHAR(200) NOT NULL,
  "slug" VARCHAR(200) NOT NULL UNIQUE,
  "body" TEXT NOT NULL,
  "status" TEXT DEFAULT 'draft',
  "created_by" UUID NOT NULL,
  "published_at" TIMESTAMPTZ,
  "created_at" TIMESTAMPTZ DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ DEFAULT NOW(),
  "deleted_at" TIMESTAMPTZ,
  CONSTRAINT "chk_posts_status" CHECK ("status" IN ('draft', 'published'))
);

CREATE UNIQUE INDEX IF NOT EXISTS "idx_posts_0" ON "posts" ("slug");
CREATE INDEX IF NOT EXISTS "idx_posts_1" ON "posts" ("created_at" DESC);
```

Comments table:

```sql
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE IF NOT EXISTS "comments" (
  "id" UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  "post_id" UUID NOT NULL,
  "body" TEXT NOT NULL,
  "author_name" VARCHAR(100) NOT NULL,
  "created_by" UUID NOT NULL,
  "created_at" TIMESTAMPTZ DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ DEFAULT NOW(),
  CONSTRAINT "fk_comments_post_id" FOREIGN KEY ("post_id") REFERENCES "posts"("id")
);

CREATE INDEX IF NOT EXISTS "idx_comments_0" ON "comments" ("post_id");
CREATE INDEX IF NOT EXISTS "idx_comments_1" ON "comments" ("created_at" DESC);
```

## Patterns demonstrated

| Pattern                  | Controller                  | How                                                        |
|--------------------------|-----------------------------|------------------------------------------------------------|
| Auto-fill from JWT       | `prepare_post`              | `ctx.user.id` into `ctx.input["created_by"]`               |
| Derived fields           | `prepare_post`              | Slug generated from title                                  |
| Default values           | `prepare_post`              | Status defaults to `"draft"`                               |
| Input validation         | `prepare_post`              | Body cannot be whitespace-only                             |
| DB lookups in controller | `enforce_edit_rules`        | Fetches current post status from DB                        |
| Role-based logic         | `enforce_edit_rules`        | Only admins can publish                                    |
| Required headers         | `enforce_edit_rules`        | `X-Edit-Reason` for status revert                          |
| Auto-update derived field| `enforce_edit_rules`        | Slug re-generated when title changes                       |
| Cross-resource checks    | `validate_comment`          | Verifies referenced post is published                      |
| Post status gating       | `validate_comment`          | Rejects comments on draft and archived posts               |
| XSS prevention           | `validate_comment`          | Strips HTML tags from body via state machine               |
| Rate limiting            | `validate_comment`          | Max 10 comments per user per hour via DB query             |
| Ownership enforcement    | `check_comment_ownership`   | Owner or admin check                                       |
| Time-based edit window   | `check_comment_ownership`   | 15-minute edit window for non-admins                       |
| Response headers         | `cleanup_comments`          | `X-Comments-Archived` header                               |
| After-controller logging | `cleanup_comments`          | `tracing::info!` with structured fields                    |

## Why this example matters

This example is intentionally small, but it hits the framework behaviors most
new users need to trust:

- generation from explicit schema only
- role plus owner auth combinations
- declared relations
- checked-in migrations
- browser docs and OpenAPI that match the live routes
- real-world controller patterns: slug generation, edit rules, post-status gating, XSS stripping, rate limiting, ownership checks, and time-based edit windows
