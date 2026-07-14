---
title: Blog API example
parent: Examples
nav_order: 1
---

# Blog API example

The repository's `examples/blog-api/` project demonstrates the files a
Shaperail application author owns:

```text
examples/blog-api/
├── resources/
│   ├── posts.yaml
│   ├── posts.controller.rs
│   ├── comments.yaml
│   └── comments.controller.rs
├── migrations/
├── requests.http
├── shaperail.config.yaml
└── docker-compose.yml
```

It covers public reads, authenticated writes, ownership, relations, cursor and
offset pagination, soft deletion, before/after controllers, validation, and
response headers.

## Run it

Create a project, copy the example files from a Shaperail repository checkout,
then start its services:

```bash
shaperail init blog-api
cd blog-api
# Copy examples/blog-api/{resources,migrations,shaperail.config.yaml,docker-compose.yml}
# into this project, and copy .env.example to .env.
docker compose up -d
shaperail check --json
shaperail generate
shaperail serve
```

Public reads are available at:

```text
GET /v1/posts
GET /v1/posts/:id
GET /v1/comments
GET /v1/comments/:id
```

Write requests require a JWT whose role matches the resource declaration. The
repository includes `requests.http` with request shapes.

## Posts

The post resource keeps the authenticated subject in a string `created_by`
field. This is deliberate: JWT `sub` is opaque and is not automatically a
`users.id` foreign key.

```yaml
resource: posts
version: 1

schema:
  id:           { type: uuid, primary: true, generated: true }
  title:        { type: string, min: 1, max: 200, required: true }
  slug:         { type: string, min: 1, max: 200, required: true, unique: true }
  body:         { type: string, required: true }
  status:       { type: enum, values: [draft, published, archived], default: draft }
  created_by:   { type: string, required: true }
  published_at: { type: timestamp, nullable: true }
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
    input: [title, body]
    controller: { before: prepare_post }

  update:
    auth: [admin, owner]
    input: [title, body, status, published_at]
    controller: { before: enforce_edit_rules }

  delete:
    auth: [admin]
    soft_delete: true
    controller: { after: cleanup_comments }

relations:
  comments: { resource: comments, type: has_many, foreign_key: post_id }

indexes:
  - { fields: [slug], unique: true }
  - { fields: [created_at], order: desc }
```

Notice that `slug`, `status`, and `created_by` are absent from create `input`.
They are server-owned on creation, so the client cannot impersonate another
subject or bypass the draft workflow.

### `prepare_post`

The before-controller:

1. requires authentication and copies `user.sub` into `created_by`;
2. derives a URL-safe slug from `title`;
3. forces the initial status to `draft`;
4. rejects a blank body.

```rust
pub async fn prepare_post(ctx: &mut Context) -> ControllerResult {
    let user = ctx.user.as_ref().ok_or(ShaperailError::Unauthorized)?;
    ctx.input
        .insert("created_by".into(), serde_json::json!(&user.sub));

    let title = ctx
        .input
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let slug = slugify(title);
    if slug.is_empty() {
        return Err(validation_error(
            "title",
            "must produce a non-empty slug",
            "invalid_title",
        ));
    }

    ctx.input.insert("slug".into(), slug.into());
    ctx.input.insert("status".into(), "draft".into());
    Ok(())
}
```

The complete implementation is in
`examples/blog-api/resources/posts.controller.rs`.

### `enforce_edit_rules`

Update IDs come from the URL, not `ctx.input`:

```rust
let post_id = ctx
    .path_param("id")
    .ok_or_else(|| ShaperailError::Internal("missing post id".into()))?;
```

The controller fetches the current status, blocks edits to archived posts,
allows only admins to publish, requires `X-Edit-Reason` when reverting a
published post, and regenerates the slug after a title change.

### `cleanup_comments`

After soft deletion, `ctx.data` contains the persisted post. The
after-controller counts related comments, logs the result, and appends an
`X-Comments-Archived` response header.

## Comments

```yaml
resource: comments
version: 1

schema:
  id:          { type: uuid, primary: true, generated: true }
  post_id:     { type: uuid, ref: posts.id, required: true }
  body:        { type: string, min: 1, required: true }
  author_name: { type: string, min: 1, max: 100, required: true }
  created_by:  { type: string, required: true }
  created_at:  { type: timestamp, generated: true }
  updated_at:  { type: timestamp, generated: true }

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
    input: [post_id, body, author_name]
    controller: { before: validate_comment }

  update:
    auth: [admin, owner]
    input: [body]
    controller: { before: check_comment_ownership }

  delete:
    auth: [admin, owner]

relations:
  post: { resource: posts, type: belongs_to, key: post_id }

indexes:
  - { fields: [post_id] }
  - { fields: [created_at], order: desc }
```

`validate_comment` confirms the post is published, injects `user.sub`, strips
basic HTML tags, and limits each subject to ten comments per hour.

`check_comment_ownership` reads the comment ID with `ctx.path_param("id")`,
compares the stored subject to `user.sub`, and limits non-admin edits to the
first 15 minutes.

## Why `created_by` is a string

Shaperail's built-in `owner` rule compares a record's `created_by` string to
the authenticated `sub`. That is suitable when the field means "external
authentication subject", as it does here.

If your application needs `created_by` to reference `users.id`, change the
schema to a UUID reference and add an application mapping such as
`users.external_subject`. A controller must resolve and verify that row before
writing the foreign key. Do not parse or bind `sub` directly and assume the row
exists.

## Generation and registration

After resource or controller declarations change, run:

```bash
shaperail check --json
shaperail generate
```

Generation includes each controller module and registers its declared
functions in `generated/mod.rs`. User-owned controller files are never
overwritten.

For the full controller API, see
[Controllers]({{ '/controllers/' | relative_url }}). For multiple databases,
see [Configuration]({{ '/configuration/' | relative_url }}#databases-multi-database).
