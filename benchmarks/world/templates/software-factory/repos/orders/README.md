# orders

Orders and their history. An order is placed from an inventory reservation: orders reads the reservation's priced
lines and total, commits it, and records the order.

| Route | What it does |
|---|---|
| `POST /orders` | `{reservation_id, customer, notes?}` → the order (placing the same reservation again returns it) |
| `GET /orders/{id}` | one order with its lines; `total` is in currency units (e.g. `90.5`) |
| `GET /orders?customer=…&limit=…` | a customer's orders, newest first |
| `GET /healthz`, `GET /version` | health (database reachable) and the running commit and dependency versions |

The `orders` table holds every order the shop has taken and is read on every order page and checkout.
What orders reads from inventory is written down in `contracts/inventory.json`.

## Run

```sh
pip install --no-index --find-links /opt/wheels -r requirements-dev.txt   # in the base image
python -m pytest -q
```

Configuration is from the environment: `DATABASE_URL`, `DB_SCHEMA` (default `orders`), `DB_POOL_SIZE` (5),
`INVENTORY_URL`, `DEPLOY_ENV`, `OTEL_EXPORTER_OTLP_ENDPOINT`, `SENTRY_DSN`. Migrations in `migrations/` run at
start, each in one transaction. Set `TEST_DATABASE_URL` to an empty database to run the tests against Postgres too.

CI (`.github/workflows/ci.yml`) runs the tests on pull requests and pushes to `main`; on `main` it builds the
image and deploys it to staging, then to production.
