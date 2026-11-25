# Platform

The software company's production and staging as one Docker Compose project (`compose.yaml`), run by the world's
`platform` part on the [`live-service`](../../../../../integrations/live-service/README.md) integration.
`service.yaml` is that part's config. The world copies this folder under `files/platform/`.

| | |
|---|---|
| Production | `prod-storefront`, `prod-orders`, `prod-inventory`, `prod-db`, `prod-seed`, and `traffic` (the synthetic users and the scoring probes) |
| Staging | `staging-storefront`, `staging-orders`, `staging-inventory`, `staging-db`, `staging-seed` |
| Observability | `otel-collector`, `worldclock-gateway`, `prometheus`, `alertmanager`, `loki`, `tempo`, `grafana`, `glitchtip`, `glitchtip-db`, and `glitchtip-worker` (full profile only) |
| Images | passed by the part as `IMAGE_<SERVICE>` from `service.yaml`'s `images`, each pinned in [`../images.yaml`](../images.yaml). Never pulled at run time. |
| Memory | every container has a `mem_limit`: `MEM_<SERVICE>` and each program's own knob from `profiles/lean.yaml` or `profiles/full.yaml` |

## What's here

- **Databases**: one Postgres 17 per environment, database `shop`, a schema per service. The psql seeds load the
  catalog (`seed/catalog.csv`, stock for more than a world week of the traffic's orders) and the shop's past orders
  once the services have migrated: `PROD_ORDERS_ROWS` in production (20000 in `service.yaml`, or the hot-table trap's
  `rows`, default 300000) and `STAGING_ORDERS_ROWS` (1000) in staging. A database that already has orders
  is left as it is.
- **Telemetry**: apps send OTLP/HTTP to `otel-collector` (4318), which forwards to `worldclock-gateway`. The gateway
  restamps every time to world time and sends on to Prometheus (`/api/v1/otlp/v1/metrics`, out-of-order window 30
  minutes), Loki (`/otlp/v1/logs`) and Tempo (OTLP). While the world has no world time of its own (a base or gold
  save being made), the gateway drops what it receives, so saved stores hold only world time. See
  [`tools/worldclock-gateway`](../../../../../tools/worldclock-gateway/README.md).
- **Dashboards** (`grafana/dashboards/`): Service overview, Production SLO, Deploys and Database. Grafana's three
  datasources read through the gateway's query face (4319) with `X-Shakedown-Shift: wall`. Every query reads only
  metrics listed in `metrics.txt`, the names `platform-lib` emits.
- **Alerts** (`prometheus/rules.yml`): `HighErrorRate` (page), `HighLatency` (ticket) and `ServiceDown` (page).
  Alertmanager has no notifier. Prometheus evaluates the rules on the wall clock, where world-time samples are not,
  so `service.yaml`'s `alerts/paging` watch asks the gateway for the paging rules' expressions at world time every
  15 s and turns the answers into `alert_firing` and `alert_resolved` world events. The template links those to
  PagerDuty.
- **GlitchTip** 6.2.6, without Valkey: web and worker in one process (lean) or two containers (full).
  `glitchtip/init.sh` creates the organisation, the three projects with the apps' DSN keys and the probes' API token,
  then starts GlitchTip.
- **Traffic** (`traffic/`): seeded synthetic users against production on the world clock, and the probes
  `slo/production`, `traffic/stats` and `contracts` on port 9100. The contract probe runs `contracts/`, a copy of
  the consumers' contracts that agents cannot change. See [`traffic/README.md`](traffic/README.md).

### The `platform` part's config (`service.yaml`)

| Key | What it holds |
|---|---|
| `compose_file`, `env` | the copied compose file, and its environment (row counts, URLs, the traffic's events and seed). The observability system merges the chosen profile into `env`. |
| `volumes` | how each volume is saved: `postgres` for `prod-db`, `staging-db` and `glitchtip-db`, `hardlink` (with a live copy) for Prometheus, Loki and Tempo, and `skip` for Grafana and Alertmanager |
| `http` | the hosts agents reach: `api.` and `staging-api.` (the storefronts), `grafana.`, `prometheus.`, `loki.` and `tempo.` (the gateway's query face), `alertmanager.`, `glitchtip.` |
| `probes` | the docs the checks read: `version/<service>`, `alerts`, `alerts/paging`, `glitchtip/issues`, `slo/production`, `traffic/stats`, `contracts` |
| `watches` | `alerts/paging`, every 15000 ms |

### Memory budget

`budget.yaml` holds the lean profile's budget: each container's limit in MiB and the knob that keeps the program
under it. The compose containers and the CI job's 384 MiB add up to 2560 MiB, under the 2600 MiB the tests allow. The
psql seeds (32 MiB each) exit before traffic starts and are counted apart. Host processes for the twins and people
are 300 MiB, measured, not capped.

## How it's tested

```bash
cd benchmarks/world
uv run --with pytest pytest -q tests/software_factory/test_platform_files.py
uv run --with pytest pytest -q -m live tests/software_factory/test_live_platform_configs.py \
  tests/software_factory/test_live_platform_stack.py tests/software_factory/test_live_traffic.py
```

- `test_platform_files.py` (needs `docker compose` for one test): `docker compose config` accepts the file under
  both profiles. The lean limits match `budget.yaml` and sum under 2600 MiB. Each Go program gets a soft limit
  below its hard one. Dashboards and alert rules read only emitted metrics. Telemetry goes collector, then gateway,
  then stores. Every image is pinned in `images.yaml`. Volumes are saved as listed above. The `alerts/paging` probe
  encodes the paging rules of `rules.yml`. Every app reports to the GlitchTip project `init.sh` makes.
- `test_live_platform_configs.py` (live, pinned images present): `promtool check config` and promtool's unit tests
  of the rules (`prometheus/rules.test.yml`: a 10% error share pages, failing health checks alone do not, a service
  that stops reporting while the others go on is down, a paused world is not an outage, slow answers open a
  ticket), `amtool check-config`, `otelcol validate`, and `loki -verify-config` under both profiles.
- `test_live_platform_stack.py` (live): Prometheus, Alertmanager, Loki, Tempo, Grafana and GlitchTip with its
  database, from this compose file under the lean profile, without apps or gateway. Each runs under its limit,
  Prometheus has the three rules, Grafana the three datasources and four dashboards, GlitchTip's init made the
  projects storefront, orders and inventory (ids 1 to 3), and an event sent with orders' DSN is taken.
- `test_live_traffic.py` (live): production's apps, `prod-db`, `prod-seed`, the collector and the traffic from this
  compose file. See [`traffic/README.md`](traffic/README.md).

These ran on 2026-11-19 (Linux x86_64, Docker Compose 5.0.2). The whole platform, gateway included, ran in the
template's live world test on 2026-11-20 (see the [template README](../README.md#how-its-tested)).

One measurement has no test that repeats it. On 2026-11-19, the three services' migrations and `seed/seed.sql` with
300000 rows ran on Postgres 17 (160 MiB, `shared_buffers` 32 MB) in 14 s, made 54 MB of orders, and a second run
changed nothing. A one-transaction `ALTER TABLE orders ADD COLUMN …; UPDATE orders …` backfill of those rows held
the table for about 6 s while a reader waited.

## Limits

- The observability services always run: the compose file has no switch for them.
- The full profile's containers were never run. Only its compose file and Loki config are checked.
- No test checks that logs and traces reach Loki and Tempo. Metrics reaching Prometheus are shown only by the live
  world test, where paging alerts fired.
- `budget.yaml`'s idle figures were measured once, without apps or traffic. The containers' memory under traffic
  was not measured one by one.
