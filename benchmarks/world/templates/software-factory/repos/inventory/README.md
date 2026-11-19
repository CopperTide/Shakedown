# inventory

Stock levels, prices and holds. Checkout places a hold (a reservation) on stock for `HOLD_SECONDS` (900); the
order commits it, or it expires and the stock is free again. Prices are list prices less any promotion, rounded
with the shared `pricing-lib`.

| Route | What it does |
|---|---|
| `GET /stock`, `GET /stock/{sku}` | products with unit price, currency and what is available (on hand less active holds) |
| `POST /reservations` | `{items: [{sku, qty}]}` → a held reservation with priced lines and total; 409 lists what is short |
| `GET /reservations/{id}` | `held`, `expired` or `committed` |
| `POST /reservations/{id}/commit` | takes the stock (409 once expired) |
| `GET /healthz`, `GET /version` | health (database reachable) and the running commit and dependency versions |

## Run

```sh
pip install --no-index --find-links /opt/wheels -r requirements-dev.txt   # in the base image
python -m pytest -q
```

Configuration is from the environment: `DATABASE_URL`, `DB_SCHEMA` (default `inventory`), `HOLD_SECONDS`,
`DEPLOY_ENV`, `OTEL_EXPORTER_OTLP_ENDPOINT`, `SENTRY_DSN`. Migrations in `migrations/` run at start. Set
`TEST_DATABASE_URL` to an empty database to run the tests against Postgres as well.

CI (`.github/workflows/ci.yml`) runs the tests on pull requests and pushes to `main`; on `main` it builds the
image and deploys it to staging, then to production.
