# Traffic

The software company's customers: seeded synthetic users who browse, order and check stock against production, on
the world clock. The same service runs the probes the checks read: production's SLO, the traffic's totals, and each
consumer's contracts against its providers. It uses the Python standard library only, so it runs as is on the
pinned `python:3.12.16-slim` image, within 64 MiB.

| | |
|---|---|
| Program | `python -m traffic` (compose service `traffic`, user 10001) |
| Mounts | this folder read-only at `/app/traffic`, `../contracts` at `/app/contracts`, the world clock at `/shakedown` |
| Port | 9100: `/stats`, `/slo`, `/contracts`, `/healthz` |
| Starts | once production's three services are healthy and `prod-seed` has finished |

## How it works

- **Data** (`journeys.json`): each journey's rate (`rpm` when the day's shape is 1.0), the day's shape (`hourly`, 24
  values in company time, the run starting at `start_hour` 9), the journeys as steps with the services they call,
  the SLO window (30 minutes) and objective (0.01), the contract probe's period (60 s) and the events
  (`traffic-peak`: 3 times the traffic for 30 minutes from minute 90).

  | Journey | `rpm` | Steps |
  |---|---|---|
  | `browse` | 30 | storefront `GET /products` |
  | `order` | 3 | storefront: products, a cart, an item, checkout, the order |
  | `check-stock` | 6 | inventory `GET /stock`, then one item |

- **Schedule** (`schedule.py`): journey starts are a Poisson process per journey, drawn for each world minute from
  `Random("{seed}:{minute}:{journey}")`. The same seed gives the same starts however time is split, whenever the run
  starts, and in a fork from its save onwards.
- **Runner** (`runner.py`): every `tick_ms` (200) it reads the clock (`clock.py`, read as `platform_lib` reads it:
  at most 1 s past the last advance, so a paused world sends nothing), plans the journeys due since the last tick,
  and runs them on 16 workers. A journey that finds 256 waiting is dropped and counted. A journey's choices come from
  `Random("{seed}:{start}:{journey}")`. What it does next depends on production's answers.
- **Telemetry** (`otlp.py`): OTLP/HTTP JSON to the collector, stamped with wall time like the apps (the gateway
  restamps). A trace per journey whose client spans' `traceparent` the apps continue, the histogram
  `http.client.request.duration`, and a warning log per failed request. Exported every 15 s.
- **Probes** (`server.py`):
  - `/stats`: totals since the container started. `?window_s=&time=` adds a world-time window.
  - `/slo?time=`: `error_rate_30m`, `p95_ms` and `requests_30m` over the last 30 world minutes (the
    `slo/production` doc).
  - `/contracts`: `{"<consumer>-<provider>": {passed, failures, at_world_ms}}`. `passed` is null before the first
    run.
- **Errors** are what a customer sees: server errors (5xx), timeouts (2 s) and failed connections. A 4xx (an item
  sold out) is not an error. Requests are kept for 6 world hours.
- **Contract probe** (`contracts.py`): every 60 world seconds, each `<consumer>/<provider>.json` under
  `/app/contracts` runs against the provider in production. Failure lines are worded as `platform_lib.contracts`
  words them, plus no answer, a non-2xx status, or a body that is not JSON.

## Environment

| Variable | Default | Purpose |
|---|---|---|
| `TRAFFIC_SEED` | the data's `seed` (1) | the seed. The template passes `world-<seed>`. A number stays a number, other text is used as text. |
| `TRAFFIC_EVENTS` | empty | events to turn on, comma-separated: `traffic-peak` or `traffic-peak@<minute>`. The template's `traffic-peak` event sets it. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | unset (nothing exported) | the collector |
| `TRAFFIC_CONFIG` | `journeys.json` here | another data file |
| `TRAFFIC_SERVICES` | the data's `services` | `name=url,...` overrides |
| `TRAFFIC_CONTRACTS` | `/app/contracts` | the contracts folder |
| `SHAKEDOWN_CLOCK_FILE` | `/shakedown/clock.json` | the world clock |
| `TRAFFIC_PORT` | `9100` | the probes' port |

## How it's tested

```bash
cd benchmarks/world/templates/software-factory/platform
uv run --no-project --with pytest pytest -q traffic/tests
cd ../../.. && uv run --with pytest pytest -q tests/software_factory/test_traffic.py
uv run --with pytest pytest -q -m live tests/software_factory/test_live_traffic.py
```

- `traffic/tests`: the schedule, clock, journeys, client, stats, OTLP bodies, contract probe, probe server and
  `python -m traffic` against fake servers.
- `test_traffic.py`: the compose service (pinned image, 64 MiB, start order, OTLP endpoint), the probes in
  `service.yaml`, `../contracts` an unchanged copy of the repositories' `contracts/`, the traffic sending only to
  production, and the traffic-peak event the builder toggles.
- `test_live_traffic.py` (live, Docker): from the platform's compose file, against the seed images of the three
  services on seeded Postgres 17 and the pinned collector, with a clock the test writes at 4 times wall speed.

On 2026-11-19 (Linux x86_64, Docker Compose 5.0.2) the live test ran 60 journeys and 84 requests with no errors.
The three contracts passed, `/slo` read 84 requests with error rate 0 and p95 27 ms, the collector took every OTLP
export, nothing was sent while the clock stood still, and the container used 20.6 MiB of its 64.

## Limits

- The totals are not saved: after a restore, `/stats` counts from zero again. The SLO window is rebuilt from the
  requests the container has seen since it started.
- A whole day's shape and a traffic peak were never run live.
- No test checks that the traffic's telemetry reaches Loki and Tempo.
