# agentledger

```bash
docker compose up -d
shaperail serve
```

Local development is Docker-first. No manual database creation is required:
the included `docker-compose.yml` starts Postgres and Redis with credentials
that already match `.env`, and Postgres creates the `agentledger` database
automatically on first boot.

- App: http://localhost:3000
- Docs: http://localhost:3000/docs
- OpenAPI: http://localhost:3000/openapi.json

When you change resource schemas later:

```bash
shaperail migrate
shaperail serve
```
