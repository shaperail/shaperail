# Auth and Ownership

Shaperail supports declarative endpoint auth rules in resource files.

## Public Endpoints

```yaml
auth: public
```

No token is required.

## Role-Based Endpoints

```yaml
auth: [admin, member]
```

The request must carry a JWT or API key that maps to one of those roles.

## Owner-Based Endpoints

```yaml
auth: owner
```

or:

```yaml
auth: [admin, owner]
```

This is important:

- `owner` checks the authenticated user against the record’s `created_by` field
- if the record does not have `created_by`, the ownership check fails

Recommended schema pattern:

```yaml
schema:
  created_by: { type: uuid, required: true }
```

Recommended endpoint pattern:

```yaml
endpoints:
  create:
    input: [title, body, created_by]

  update:
    auth: [admin, owner]
    input: [title, body]
```

## Headers

JWT:

```http
Authorization: Bearer <token>
```

API key:

```http
X-API-Key: <key>
```

## What Shaperail Does Not Do Automatically

Shaperail does not currently auto-fill `created_by` from the token for you.

You must choose one of these approaches:

- send `created_by` explicitly in the create payload
- write a hook that sets or validates it before insert

## Practical Recommendation

For first projects:

- make read endpoints `public`
- use `[admin, member]` for create endpoints
- use `[admin, owner]` for update and delete only if the resource includes
  `created_by`

See [examples/blog-api/resources/posts.yaml](../examples/blog-api/resources/posts.yaml)
for a concrete pattern.
