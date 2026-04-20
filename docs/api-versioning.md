---
title: API versioning
parent: Guides
nav_order: 11
---

# API versioning

Shaperail uses URL-prefix versioning driven by the `version` field in each
resource YAML file. There is one canonical way to version APIs: the `version`
integer maps directly to a `/v{N}/` path prefix.

## How it works

Every resource file declares a version:

```yaml
resource: users
version: 1

schema:
  id:    { type: uuid, primary: true, generated: true }
  email: { type: string, format: email, unique: true, required: true }
  name:  { type: string, min: 1, max: 200, required: true }
```

All generated endpoints for this resource are prefixed with `/v1/`:

| Endpoint | Generated path |
| --- | --- |
| list | `GET /v1/users` |
| get | `GET /v1/users/:id` |
| create | `POST /v1/users` |
| update | `PATCH /v1/users/:id` |
| delete | `DELETE /v1/users/:id` |

Change `version: 2` and the prefix becomes `/v2/`:

```
GET /v2/users
GET /v2/users/:id
POST /v2/users
...
```

## Running multiple versions side by side

To serve both v1 and v2 of a resource at the same time, create separate resource
files:

```
resources/
  users_v1.yaml    # version: 1
  users_v2.yaml    # version: 2
```

### users_v1.yaml

```yaml
resource: users
version: 1

schema:
  id:    { type: uuid, primary: true, generated: true }
  email: { type: string, format: email, unique: true, required: true }
  name:  { type: string, min: 1, max: 200, required: true }
  created_at: { type: timestamp, generated: true }

endpoints:
  list:
    auth: public
    pagination: cursor

  get:
    auth: public

  create:
    auth: [admin]
    input: [email, name]
```

### users_v2.yaml

```yaml
resource: users
version: 2

schema:
  id:           { type: uuid, primary: true, generated: true }
  email:        { type: string, format: email, unique: true, required: true }
  first_name:   { type: string, min: 1, max: 100, required: true }
  last_name:    { type: string, min: 1, max: 100, required: true }
  display_name: { type: string, max: 200 }
  role:         { type: enum, values: [admin, member, viewer], default: member }
  deleted_at:   { type: timestamp, nullable: true }
  created_at:   { type: timestamp, generated: true }
  updated_at:   { type: timestamp, generated: true }

endpoints:
  list:
    auth: public
    pagination: cursor
    filters: [role]
    search: [first_name, last_name, email]

  get:
    auth: public

  create:
    auth: [admin]
    input: [email, first_name, last_name, display_name, role]

  update:
    auth: [admin, owner]
    input: [first_name, last_name, display_name, role]

  delete:
    auth: [admin]
    soft_delete: true
```

Both versions run simultaneously. Clients calling `/v1/users` get the v1
response shape; clients calling `/v2/users` get the v2 shape.

### Shared database table

Both versions read from and write to the same `users` table. The v1 endpoints
expose a subset of columns (e.g., a single `name` field), while v2 exposes the
expanded schema. Use controllers to handle the mapping if schemas diverge:

```yaml
# users_v1.yaml
endpoints:
  get:
    auth: public
    controller: { after: map_v1_response }
```

In `resources/users_v1.controller.rs` using the same manual registration pattern
described in the [Controllers guide]({{ '/controllers/' | relative_url }}):

```rust
use shaperail_runtime::handlers::controller::{Context, ControllerResult};

pub async fn map_v1_response(ctx: &mut Context) -> ControllerResult {
    // Combine first_name + last_name into a single "name" field for v1 clients
    if let Some(data) = &mut ctx.data {
        if let Some(obj) = data.as_object_mut() {
            let first = obj.get("first_name").and_then(|v| v.as_str()).unwrap_or("");
            let last = obj.get("last_name").and_then(|v| v.as_str()).unwrap_or("");
            obj.insert("name".into(), serde_json::json!(format!("{first} {last}")));
            obj.remove("first_name");
            obj.remove("last_name");
        }
    }
    Ok(())
}
```

## Deprecation patterns

### Announce deprecation in response headers

Use a controller to add deprecation headers to v1 responses:

```rust
pub async fn deprecation_header(ctx: &mut Context) -> ControllerResult {
    ctx.response_headers.push(("Deprecation".into(), "true".into()));
    ctx.response_headers.push(("Sunset".into(), "2026-06-01".into()));
    ctx.response_headers.push((
        "Link".into(),
        "</v2/users>; rel=\"successor-version\"".into(),
    ));
    Ok(())
}
```

Attach it to every v1 endpoint:

```yaml
# users_v1.yaml
endpoints:
  list:
    auth: public
    controller: { after: deprecation_header }
  get:
    auth: public
    controller: { after: deprecation_header }
```

### Log deprecated version usage

Add a `before` controller that logs a warning when v1 is hit:

```rust
pub async fn log_v1_usage(ctx: &mut Context) -> ControllerResult {
    tracing::warn!(
        request_id = ?ctx.headers.get("x-request-id"),
        user_agent = ?ctx.headers.get("user-agent"),
        "Deprecated v1 API called"
    );
    Ok(())
}
```

### Sunset a version

When you are ready to remove v1:

1. Delete `resources/users_v1.yaml`
2. Run `shaperail generate` to remove v1 routes
3. Deploy

Clients still calling `/v1/users` will receive a 404.

## Migration strategies for clients

### Strategy 1: Parallel versions (recommended)

Run v1 and v2 side by side for a transition period:

1. Deploy v2 alongside v1
2. Notify clients of the deprecation timeline
3. Add deprecation headers to v1 (see above)
4. Monitor v1 traffic — when it drops to zero, remove v1
5. Delete the v1 resource file and redeploy

Timeline example:

| Phase | Duration | v1 status | v2 status |
| --- | --- | --- | --- |
| Launch v2 | Week 0 | Active | Active |
| Deprecation notice | Week 0 | Deprecated | Active |
| Migration window | Weeks 1-8 | Deprecated | Active |
| Sunset v1 | Week 9 | Removed | Active |

### Strategy 2: Redirect outside Shaperail

HTTP redirects between API versions are not exposed through the current
controller `Context`. If you want `/v1/...` to 301/302 to `/v2/...`, add that
behavior in your reverse proxy or custom Actix route layer instead of a
Shaperail controller.

### Strategy 3: Single version, additive changes only

If you never make breaking changes, keep `version: 1` and only add new fields.
Existing clients ignore fields they do not recognize. This avoids versioning
complexity entirely but requires discipline:

- Never remove a field
- Never rename a field
- Never change a field's type
- New required fields must have defaults so existing create calls still work

## Best practices

1. **Increment the version only for breaking changes.** Additive changes (new
   optional fields, new endpoints) do not require a version bump.
2. **Keep the version integer, not semver.** The resource `version` field is a
   single integer, not a dotted version string.
3. **Document the change.** Export the OpenAPI spec for each version so clients
   can diff: `shaperail export openapi > openapi-v2.json`
4. **Set a sunset date.** Do not keep old versions alive indefinitely. Announce
   a sunset date and stick to it.
5. **Test both versions.** When running v1 and v2 in parallel, test that both
   produce correct responses for their respective schemas.
