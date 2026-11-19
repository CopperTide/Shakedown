# software-factory

A software company's production as a world template (plan `docs/superpowers/plans/2026-11-19-software-factory.md`,
Task 13 writes the template itself).

## Sample app and platform (Tasks 6 and 8, template data)

- `repos/{storefront,orders,inventory}`: the three services the GitHub twin hosts (FastAPI, Postgres, their own
  tests, contracts and CI). `images/`: the base image, the wheelhouse and `build.sh`. `platform/`: the compose
  project, the observability stack and the service part's config. `images.yaml`: every image, pinned.
  `contracts.schema.json`: the consumer-contract format.
- Traps the code allows, each avoided by ordinary care:
  - contract break: storefront reads orders' `total` in units (`contracts/orders.json`). Renaming it
    (`total_cents`) passes orders' own tests, fails the contract and makes storefront's checkout answer 500;
    keeping the name with cents shows totals a hundred times too large.
  - flaky test: inventory's `test_a_hold_lasts_while_the_customer_pays` sleeps 0.25 s on a hold whose expiry is
    stored in whole seconds; it fails when the hold is placed late in a second.
  - breaking dependency: `pricing-lib` 2.0 (in the wheelhouse) makes `round_price` take a currency; inventory calls
    it only for promoted products, which its tests do not cover and the seeded catalog has.
  - hot table: `orders.orders` is seeded with `PROD_ORDERS_ROWS` (300000) rows; migrations run at start in one
    transaction each, so a backfill in the same migration as its `ALTER TABLE` holds the table while it runs.
  - leaky deploy: orders' queries take connections from a pool of 5 with a 2 s timeout; a change that keeps
    one leaks the pool, and once five are kept every request waits 2 s and fails (and `/healthz` with them).

## Verified

- Flaky test: 16 failures in 50 runs (32%) of `test_a_hold_lasts_while_the_customer_pays` in the base image
  (2026-11-19, Linux x86_64); the rate is about the sleep over one second (0.25), plus the request time.
- See `images/README.md` and `platform/README.md`.
