# storefront

The shop's public API. Products and stock come from **inventory**, orders from **orders**; carts are kept here.

| Route | What it does |
|---|---|
| `GET /products` | the catalog with price and availability (inventory `GET /stock`) |
| `POST /cart`, `GET /cart/{id}`, `POST /cart/{id}/items` | a cart and its items |
| `POST /checkout` | reserves the cart's stock (inventory `POST /reservations`), places the order (orders `POST /orders`), empties the cart |
| `GET /orders/{id}` | an order's status, total and when it was placed (orders `GET /orders/{id}`) |
| `GET /healthz`, `GET /version` | health (database reachable) and the running commit and dependency versions |

What storefront reads from the services it calls is written down in `contracts/` (one file per provider). The
tests run against fakes of those services, and `tests/test_contracts.py` holds the fakes to the contracts.

## Run

```sh
pip install --no-index --find-links /opt/wheels -r requirements-dev.txt   # in the base image
python -m pytest -q
```

Configuration is from the environment: `DATABASE_URL`, `DB_SCHEMA` (default `storefront`), `INVENTORY_URL`,
`ORDERS_URL`, `DEPLOY_ENV`, `OTEL_EXPORTER_OTLP_ENDPOINT`, `SENTRY_DSN`. Migrations in `migrations/` run at start.
Set `TEST_DATABASE_URL` to an empty database to run the tests against Postgres as well.

CI (`.github/workflows/ci.yml`) runs the tests on pull requests and pushes to `main`; on `main` it builds the
image and deploys it to staging, then to production.
