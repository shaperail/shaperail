---
title: Docker deployment
parent: Guides
nav_order: 5
---

Shaperail is Docker-first for local development. Generated apps should boot
their local dependencies with Compose and should not require manual database
setup for the first run.

## Local development contract

Generated apps include a `docker-compose.yml` that starts:

- PostgreSQL
- Redis

The generated `.env` matches that compose file, so the default path is:

```bash
docker compose up -d
shaperail serve
```

## Standard local URLs

- App: `http://localhost:3000`
- Docs: `http://localhost:3000/docs`
- OpenAPI: `http://localhost:3000/openapi.json`
- Health: `http://localhost:3000/health`

## If ports are already in use

Change the host-side port mapping in `docker-compose.yml` and update `.env` to
match. Example:

```yaml
ports:
  - "5434:5432"
```

Then update:

```text
DATABASE_URL=postgresql://shaperail:shaperail@localhost:5434/my_app
```

## Release image

Build a release image for a user app with:

```bash
shaperail build --docker
```

The framework target is a scratch-based runtime image for the final app.

## Deployment advice

For first deployments:

- keep Postgres and Redis as managed services or separate containers
- use `/health` and `/health/ready` for liveness and readiness checks
- keep `JWT_SECRET` out of source control
- review the exported OpenAPI spec before exposing the app publicly

## Recommended production checklist

| Item | Why it matters |
| --- | --- |
| `JWT_SECRET` comes from environment or secret manager | Prevents checked-in production credentials |
| Database and Redis URLs point at real services, not local containers | Separates production runtime from dev wiring |
| Health checks are wired into your platform | Makes rollout and restart behavior predictable |
| OpenAPI has been exported and reviewed | Confirms the public contract before launch |
