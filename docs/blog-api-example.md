---
title: Blog API example
parent: Examples
nav_order: 1
---

This example shows the files a Shaperail user actually authors:

- `resources/*.yaml`
- `migrations/*.sql`
- `shaperail.config.yaml`
- `.env`
- `docker-compose.yml`

You can find the source files in the repository under
`examples/blog-api/`, but this page explains the full example without sending
you out of the docs site.

## What the example covers

- public blog post reads
- protected post creation and updates
- owner-based post and comment updates through `created_by`
- post/comment relations
- cursor pagination on posts
- offset pagination on comments
- soft delete on posts

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

schema:
  id:           { type: uuid, primary: true, generated: true }
  title:        { type: string, min: 1, max: 200, required: true }
  slug:         { type: string, min: 1, max: 200, required: true, unique: true }
  body:         { type: string, required: true }
  status:       { type: enum, values: [draft, published], default: draft }
  created_by:   { type: uuid, required: true }
  published_at: { type: timestamp, nullable: true }
  created_at:   { type: timestamp, generated: true }
  updated_at:   { type: timestamp, generated: true }

endpoints:
  list:
    method: GET
    path: /posts
    auth: public
    filters: [status, created_by]
    search: [title, body]
    pagination: cursor
    sort: [created_at, title]

  get:
    method: GET
    path: /posts/:id
    auth: public

  create:
    method: POST
    path: /posts
    auth: [admin, member]
    input: [title, slug, body, status, created_by, published_at]

  update:
    method: PATCH
    path: /posts/:id
    auth: [admin, owner]
    input: [title, body, status, published_at]

  delete:
    method: DELETE
    path: /posts/:id
    auth: [admin]
    soft_delete: true
```

This resource demonstrates:

- public read endpoints
- owner-aware updates through `created_by`
- cursor pagination
- soft delete on the delete route

## Comments resource

```yaml
resource: comments
version: 1

schema:
  id:          { type: uuid, primary: true, generated: true }
  post_id:     { type: uuid, ref: posts.id, required: true }
  body:        { type: string, min: 1, required: true }
  author_name: { type: string, min: 1, max: 100, required: true }
  created_by:  { type: uuid, required: true }
  created_at:  { type: timestamp, generated: true }
  updated_at:  { type: timestamp, generated: true }

endpoints:
  list:
    method: GET
    path: /comments
    auth: public
    filters: [post_id, created_by]
    pagination: offset
    sort: [created_at]

  get:
    method: GET
    path: /comments/:id
    auth: public

  create:
    method: POST
    path: /comments
    auth: [admin, member]
    input: [post_id, body, author_name, created_by]

  update:
    method: PATCH
    path: /comments/:id
    auth: [admin, owner]
    input: [body]

  delete:
    method: DELETE
    path: /comments/:id
    auth: [admin, owner]
```

This complements `posts` by showing:

- a `belongs_to` relationship to `posts`
- offset pagination
- owner-based updates and deletes

## Matching migrations

The example also checks in the SQL that corresponds to the resource files.

Posts table:

```sql
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
```

Comments table:

```sql
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
```

## Why this example matters

This example is intentionally small, but it hits the framework behaviors most
new users need to trust:

- generation from explicit schema only
- role plus owner auth combinations
- declared relations
- checked-in migrations
- browser docs and OpenAPI that match the live routes
