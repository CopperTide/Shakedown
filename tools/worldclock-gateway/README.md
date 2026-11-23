# worldclock-gateway

Puts a simulated platform's telemetry on world time. Services export OTLP stamped with the wall clock. The
gateway restamps every time to world time on the way to the stores, and answers the stores' queries in world
time. It reads the world clock from the `clock.json` that the `live-service` integration writes at each advance
([integrations/live-service](../../integrations/live-service)). The software-factory world template runs it
(`benchmarks/world/templates/software-factory/images.yaml`).

## Settings

| Variable | Default | What it is |
|---|---|---|
| `INGEST_ADDR` | `0.0.0.0:4318` | Where OTLP/HTTP arrives |
| `QUERY_ADDR` | `0.0.0.0:4319` | Where the stores' queries arrive |
| `CLOCK_FILE` | `/shakedown/clock.json` | The world clock, checked every 100 ms |
| `TRACES_URL`, `LOGS_URL`, `METRICS_URL` | none, required | Where restamped traces, logs and metrics go |
| `PROMETHEUS_URL`, `LOKI_URL`, `TEMPO_URL` | none, required | The stores queries go to |

## How it works

- **Ingest:** `POST /v1/traces`, `/v1/logs` and `/v1/metrics`, OTLP/HTTP in protobuf or JSON, uncompressed. A
  `Content-Encoding` other than `identity` is answered 415 (the platform's collector sends
  `compression: none`). The gateway restamps every span's start, end and event times, every log record's time
  and observed time, and every metric point's start and time and its exemplars' times. A time of 0 (not set)
  stays 0. The export goes on in the same encoding, and the store's status, type and body come back. A store
  that can't be reached is a 502.
- **World time:** each clock file is a point of a timeline. A stamp is mapped by the point in effect at its own
  wall time, so the spans of one batch keep their order and durations:
  `world = epoch_ms + clock_ms + (stamp − wall_ms) × speed`, at most `MAX_LEAD_MS` (1 s, the kits' lead) of wall
  time past the point. Stamps stand still while a branch is paused. A point behind where the previous one had
  run to (a resume) starts from there, so world time never goes back. A file with another epoch starts the
  timeline again. Without a clock file, world time is wall time.
- **Queries:** `ANY /q/{prometheus|loki|tempo}/{*path}`, or any path at the root of a host whose first label
  names the store (`prometheus.<domain>`). `X-Forwarded-Host`, as shaked's proxy sends it, wins over `Host`.
  Parameters may be in the URL or, for a POST, a form.
  - Without `X-Shakedown-Shift` (agents): Prometheus and Loki instant queries without `time` are asked at world
    now. Ranges without `end` end at world now. Loki and Tempo ranges without `start` start an hour before.
  - With `X-Shakedown-Shift: wall` (Grafana's data sources): `start`, `end` and `time` move by world minus wall
    (seconds for Prometheus and Tempo, nanoseconds for Loki integers). A JSON answer's times move back:
    Prometheus' and Loki's `[t, v]` pairs and Tempo's `startTimeUnixNano` and `endTimeUnixNano`. Protobuf answers
    (Tempo's trace by id, when asked for protobuf) pass through unchanged.
  - `Accept-Encoding` is not passed on, so answers come back uncompressed and readable.

## Build

```bash
tools/worldclock-gateway/build-image.sh
```

The script builds the release binary (`cargo build --release -p worldclock-gateway`), checks that it needs no
glibc newer than Debian 12's 2.36, and builds the image `shakedown/worldclock-gateway:<version>` on
`debian:bookworm-slim`, as user `65534`. It never pulls: run `docker pull debian:bookworm-slim` first.

## Tests

```bash
cargo test -p worldclock-gateway
```

The tests cover, against a fake store: the clock (pause, piecewise points, resume, new epoch, file re-read), the
restamping of spans, events, log records, metric points and exemplars, ingest of protobuf and JSON with the
store's answer passed back and compressed bodies refused, agents' queries at world now, Grafana's queries shifted
both ways (URL and form, Prometheus seconds, Loki nanoseconds, Tempo search), and routing by host.

Also verified:

- The image was built on Linux x86_64 (Docker 29) with no glibc symbol newer than 2.36.
- Live, inside the platform (`integrations/service/tests/live_platform.rs`, run once on 2026-11-19): the
  services' OTLP passed through the collector and the gateway to Prometheus, Loki and Tempo. Read back through
  the query side, their newest metric sample, log line and trace were stamped in world time (10 minutes behind
  the wall), not wall time.

## Limits

- Compressed OTLP is refused, not decoded.
- Tempo needs `ingestion_time_range_slack` raised (the software-factory template's `platform/tempo/*.yaml` set 168h). With its 2-minute
  default, Tempo keeps world-stamped traces but its searches leave out every trace more than 2 minutes from its
  own clock.
