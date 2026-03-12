# Docker Deployment

Shaperail is Docker-first for local development.

## Local Development

Generated apps include a `docker-compose.yml` that starts:

- PostgreSQL
- Redis

The generated `.env` matches that compose file, so the normal path is:

```bash
docker compose up -d
shaperail serve
```

No manual `CREATE DATABASE` step should be required.

## Common Local URLs

- App: `http://localhost:3000`
- Docs: `http://localhost:3000/docs`
- OpenAPI: `http://localhost:3000/openapi.json`

## If Ports Are Already In Use

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

## Production Image

Build a release image for a user app with:

```bash
shaperail build --docker
```

The framework’s target is a scratch-based image for the final runtime layer.

## Deployment Advice

For first deployments:

- keep Postgres and Redis as managed services or separate containers
- use the generated app’s `/health` and `/health/ready` routes
- keep `JWT_SECRET` out of source control
- review the exported OpenAPI spec before exposing the app publicly
