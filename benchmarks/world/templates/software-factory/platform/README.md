# Platform

One compose project run by the `service` part (`service.yaml` is its config): production and staging of the three
services, each environment with one Postgres (database `shop`, a schema per service), psql seeds of the shop's
history, and the observability stack. Every container has a `mem_limit`; `profiles/lean.yaml` and
`profiles/full.yaml` hold the limits and each program's own knob, and `budget.yaml` the lean budget (2560 MiB of
containers with the CI job and the traffic service, under 2600).

- Telemetry: apps → `otel-collector` (OTLP/HTTP 4318) → `worldclock-gateway` (restamps to world time) →
  Prometheus (`/api/v1/otlp/v1/metrics`, out-of-order window 30 m), Loki (`/otlp/v1/logs`), Tempo (OTLP). Grafana's
  datasources read through the gateway's query face with `X-Shakedown-Shift: wall`.
- Dashboards (`grafana/dashboards/`): Service overview, Production SLO, Deploys, Database; every query reads only
  metrics in `metrics.txt`, which platform-lib's tests check against what a service emits.
- Alerts (`prometheus/rules.yml`): HighErrorRate (page), HighLatency (ticket), ServiceDown (page). Alertmanager has
  no notifier; the `alerts/paging` watch turns paging alerts into world events.
- GlitchTip 6.2.6, web and worker in one process (lean) or two containers (full), no Valkey; `glitchtip/init.sh`
  makes the organisation, the three projects with the apps' DSN keys and the probes' API token, then starts it.
- `seed/`: the catalog and `PROD_ORDERS_ROWS` (default 300000) past orders, loaded once the services have migrated.
- `traffic/`: seeded synthetic users against production on the world clock, and the probes `slo/production`,
  `traffic/stats` and `contracts` (port 9100) run on `contracts/`, a copy of the consumers' contracts agents
  cannot change (see `traffic/README.md`).

## Verified (2026-11-19, Linux x86_64, Docker Compose 5.0.2)

- `tests/software_factory/test_platform_files.py`: `docker compose config` accepts the file in both profiles; the
  lean limits match budget.yaml and sum under 2600 MiB; dashboards and alert rules read only emitted metrics.
- Migrations of the three services and `seed/seed.sql` with 300000 rows on Postgres 17 (160 MiB, shared_buffers
  32 MB): 14 s, 54 MB of orders, a second run changes nothing. A one-transaction `ALTER TABLE orders ADD COLUMN …;
  UPDATE orders …` backfill of those rows held the table for about 6 s while a reader waited.

- `test_live_platform_configs.py` (live): `promtool check config`, promtool's unit tests of the rules
  (`prometheus/rules.test.yml`: a 10% error share pages, failing health checks alone do not, a service that stops
  reporting while the others go on is down, a paused world is not an outage, slow answers open a ticket),
  `amtool check-config`, `otelcol validate`, `loki -verify-config` (both profiles) in the pinned images.
- `test_live_platform_stack.py` (live): Prometheus, Alertmanager, Loki, Tempo, Grafana and GlitchTip (with its
  database) from this compose file under the lean profile all run under their limits (idle MiB in budget.yaml),
  Prometheus has the three rules, Grafana the three datasources and four dashboards, GlitchTip's init made the
  projects storefront, orders and inventory (ids 1 to 3) and an event sent with orders' DSN is taken.

- `test_live_traffic.py` (live): production's apps, prod-db, prod-seed, the collector and the traffic from this
  compose file (see `traffic/README.md`).

Not verified: the whole platform together (with the observability stack and the world-time gateway, whose image
Task 7 builds), the full profile's containers, telemetry reaching the stores, and the stack's memory under
traffic.
