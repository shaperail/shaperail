---
title: Shaperail Documentation
nav_order: 1
---

Shaperail is a framework for teams that want a small source of truth,
predictable generation, and a runtime that behaves exactly like the schema says
it should.

## The shortest correct path

```bash
cargo install shaperail-cli
shaperail init my-app
cd my-app
docker compose up -d
shaperail serve
```

Open the generated app:

- `http://localhost:3000/docs`
- `http://localhost:3000/openapi.json`
- `http://localhost:3000/health`

## What Shaperail is optimized for

- Explicit resource definitions with no hidden route generation
- Flat abstractions where the resource file maps directly to runtime behavior
- Deterministic OpenAPI output and route registration
- Docker-first local development with Postgres and Redis already wired
- Generated apps that expose docs, health checks, and observability from day one

## What you actually author

These are the files a Shaperail user edits in day-to-day work:

| File | Why it matters |
| --- | --- |
| `resources/*.yaml` | Defines schema, endpoints, auth rules, relations, filters, pagination, and indexes |
| `migrations/*.sql` | Stores the SQL that changes the running database |
| `shaperail.config.yaml` | Holds service-level settings such as port, DB, cache, and auth config |
| `.env` | Connects the app to local or deployed services |
| `docker-compose.yml` | Boots Postgres and Redis for development |

## Start here

1. Follow [Getting started]({{ '/getting-started/' | relative_url }}) until you have a running app.
2. Read [Resource guide]({{ '/resource-guide/' | relative_url }}) to learn the schema contract.
3. Review [CLI reference]({{ '/cli-reference/' | relative_url }}) for the day-to-day command set.
4. Use the [Blog API example]({{ '/blog-api-example/' | relative_url }}) as a complete sample app.

## Documentation map

- [Getting started]({{ '/getting-started/' | relative_url }})
- [Guides]({{ '/guides/' | relative_url }})
- [Reference]({{ '/reference/' | relative_url }})
- [Examples]({{ '/examples/' | relative_url }})
