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

## Verified (2026-11-19, Linux x86_64, Docker Compose 5.0.2)

- `tests/software_factory/test_platform_files.py`: `docker compose config` accepts the file in both profiles; the
  lean limits match budget.yaml and sum under 2600 MiB; dashboards and alert rules read only emitted metrics.
- Migrations of the three services and `seed/seed.sql` with 300000 rows on Postgres 17 (160 MiB, shared_buffers
  32 MB): 14 s, 54 MB of orders, a second run changes nothing. A one-transaction `ALTER TABLE orders ADD COLUMN …;
  UPDATE orders …` backfill of those rows held the table for about 6 s while a reader waited.

Not verified: the stack has not been started (no observability image was pulled: the disk is nearly full), so
the Loki, Tempo, collector, Alertmanager and Grafana files and GlitchTip's init are checked only as data;
`test_live_platform_configs.py` (each program's own config check and promtool's unit tests of the rules in
`prometheus/rules.test.yml`) is written and has not run. Measured MiB per container: none yet.
