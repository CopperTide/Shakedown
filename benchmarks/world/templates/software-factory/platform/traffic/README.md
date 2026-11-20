# Traffic

Seeded synthetic users against production, on the world clock, and the probes scoring reads. Standard library
only, so it runs on the pinned `python:3.12.16-slim` image as is (`python -m traffic`, this folder mounted
read-only at `/app/traffic`, the contracts at `/app/contracts`, the clock at `/shakedown`), within 64 MiB.

- `journeys.json` (data): each journey's rate (`rpm` at a day-shape value of 1.0), the day's shape (`hourly`, 24
  values in company time; the run starts at `start_hour`), the journeys (browse the storefront, place an order,
  check stock) as steps, the services they call, the SLO window and objective, the contract probe's period and
  the events (`traffic-peak`: 3× for 30 minutes).
- `schedule.py`: journey starts as a Poisson process per journey, drawn per world minute from
  `Random("{seed}:{minute}:{journey}")` on the clock (world ms since the world's epoch): the same seed gives the
  same starts however time is split, whenever the run is started, and in a fork from the save onwards.
- `runner.py`: every `tick_ms` reads the clock (`clock.py`, read as the apps' platform_lib reads it: at most 1 s
  past the last advance, so a paused world sends nothing), plans (last tick, now] and runs the journeys on 16
  workers (a journey that finds 256 waiting is dropped and counted). A journey's choices come from
  `Random("{seed}:{start}:{journey}")`; what it does next depends on production's answers.
- `otlp.py`: OTLP/HTTP JSON to the collector like the apps (wall stamps; the gateway restamps): a trace per
  journey whose client spans' `traceparent` the apps continue, the histogram `http.client.request.duration`, and
  a warning log per failed request.
- `server.py` on 9100: `/stats` (totals since start; `?window_s=&time=` adds a world-time window), `/slo?time=`
  (`error_rate_30m`, `p95_ms`, `requests_30m` over the last 30 world minutes: the `slo/production` doc),
  `/contracts` (`{"<consumer>-<provider>": {passed, failures, at_world_ms}}`; `passed` is null before the first
  run), `/healthz`.
- Errors are server errors (5xx), timeouts (2 s) and failed connections, as a customer sees them; 4xx (an item
  sold out) are not. Requests are kept for 6 world hours; totals count everything since the container started
  (it is not saved: after a restore its counts start again).
- `contracts.py`: every 60 world seconds each `<consumer>/<provider>.json` under `../contracts/` (an unchanged copy
  of the repos' `contracts/`, checked by `test_traffic.py`) runs against the provider in production; failure lines
  are worded as `platform_lib.contracts` words them, plus no answer, a non-2xx status or a body that is not JSON.
- Environment: `TRAFFIC_EVENTS` (`traffic-peak` or `traffic-peak@<minute>`, set by the template's
  `events/traffic-peak.yaml`), `TRAFFIC_SEED` (the template passes the world's seed as `world-<seed>`; a number
  or text, empty for the data's), `OTEL_EXPORTER_OTLP_ENDPOINT`, and for tests `TRAFFIC_CONFIG`,
  `TRAFFIC_SERVICES`, `TRAFFIC_CONTRACTS`, `SHAKEDOWN_CLOCK_FILE`, `TRAFFIC_PORT`.

## Verified (2026-11-19, Linux x86_64, Docker Compose 5.0.2)

- `tests/` (`uv run --no-project --with pytest pytest -q traffic/tests` from `platform/`): the schedule, clock,
  journeys, client, stats, OTLP bodies, contract probe, probe server and `python -m traffic` against fake servers.
- `tests/software_factory/test_live_traffic.py` (live): from the platform's compose file, against the seed images
  of storefront, orders and inventory on seeded Postgres 17 and the pinned collector, with a clock written at 4×
  wall speed: 60 journeys (browse, order, check stock), 84 requests, no errors; the three contracts pass; `/slo`
  read 84 requests, error rate 0, p95 27 ms; the collector took every OTLP export; nothing was sent while the
  clock stood still; the container used 20.6 MiB of its 64.

Not verified: the stores receiving the traffic's telemetry (no gateway image yet), a whole day's shape, a
traffic peak in a live run.
