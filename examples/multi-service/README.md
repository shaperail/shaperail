# Multi-service workspace example

Demonstrates a Shaperail workspace with two services and a saga.

## Structure

```
ecommerce/
├── shaperail.workspace.yaml    # Workspace definition
├── sagas/
│   └── create_order.saga.yaml  # Distributed saga
└── services/
    ├── users-api/              # User management service (port 3001)
    └── orders-api/             # Order management service (port 3002)
```

## Running

```bash
docker compose up -d            # Start shared Postgres + Redis
shaperail serve --workspace     # Start all services in dependency order
```

The `orders-api` depends on `users-api`, so the users service starts first.

## Service registry

Both services register in Redis on startup. Use `redis-cli` to inspect:

```bash
redis-cli KEYS "shaperail:services:*"
redis-cli GET "shaperail:services:users-api"
```

## Saga

The `create_order` saga validates the user exists before creating an order.
If order creation fails, no compensating action is needed for the read-only
user validation step.
