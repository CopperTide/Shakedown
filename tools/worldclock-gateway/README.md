# worldclock-gateway

Puts the platform's telemetry in world time. Services export OTLP stamped with the wall clock; the gateway restamps
every time to world time on the way to the stores, and answers the stores' queries in world time. It reads the world
clock from the `clock.json` the `live-service` part writes at each advance (`integrations/live-service/`).

- **Ingest** (`INGEST_ADDR`, default `0.0.0.0:4318`): `POST /v1/traces`, `/v1/logs`, `/v1/metrics`, OTLP/HTTP in
  protobuf or JSON, uncompressed (a `Content-Encoding` other than `identity` is answered 415; the platform's collector
  sends `compression: none`). Every span's start, end and event time, every log record's time and observed time,
  and every metric point's start and time and its exemplars' are restamped (0, a time not set, stays 0); the export
  goes on in the same encoding to `TRACES_URL`, `LOGS_URL` or `METRICS_URL`, and the store's status, type and body
  come back. A store that can't be reached is a 502.
- **World time** (`CLOCK_FILE`, default `/shakedown/clock.json`, checked every 100 ms): each clock file is a point
  of a timeline; a stamp is mapped by the point in effect at its own wall time (so the spans of one batch keep their
  order and durations): world = `epoch_ms + clock_ms + (stamp − wall_ms) × speed`, at most `MAX_LEAD_MS` (1 s, the
  kits' lead) of wall time past the point, so stamps stand still while a branch is paused. A point behind where the
  one before had run to (a resume) starts from there: world time never goes back. A file with another epoch starts
  the timeline again. Without a clock file, world time is wall time.
- **Queries** (`QUERY_ADDR`, default `0.0.0.0:4319`): `ANY /q/{prometheus|loki|tempo}/{*path}` to `PROMETHEUS_URL`,
  `LOKI_URL`, `TEMPO_URL`, or any path at the root of a host whose first label names the store
  (`prometheus.<domain>`; `X-Forwarded-Host`, as arenad's proxy sends it, wins over `Host`). Parameters may be in the
  URL or, for a POST, a form.
  - Without `X-Shakedown-Shift` (agents): Prometheus and Loki instant queries without `time` are asked at world now;
    ranges without `end` end at world now (Loki and Tempo ranges without `start` too start an hour before).
  - With `X-Shakedown-Shift: wall` (Grafana's datasources): `start`, `end` and `time` move by world − wall (seconds
    for Prometheus and Tempo, nanoseconds for Loki integers) and a JSON answer's times move back: Prometheus' and
    Loki's `[t, v]` pairs and Tempo's `startTimeUnixNano`/`endTimeUnixNano`. Protobuf answers (Tempo's trace by id
    when asked for protobuf) are passed on as they are.
  - `Accept-Encoding` is not passed on, so answers come uncompressed and can be read.

Build: `build-image.sh` builds the release binary (`cargo build --release -p worldclock-gateway`), checks it needs no
glibc newer than Debian 12's 2.36, and makes `shakedown/worldclock-gateway:<version>` `FROM debian:bookworm-slim`
(never pulled), user `65534`. The template's `images.yaml` lists it.

## Verified

- `cargo test -p worldclock-gateway`: the clock (pause, piecewise points, resume, new epoch, file re-read), the
  restamping of spans, events, log records, metric points and exemplars, ingest of protobuf and JSON with the store's
  answer passed back and compressed bodies refused, agents' queries at world now, Grafana's queries shifted both ways
  (URL and form; Prometheus seconds, Loki nanoseconds, Tempo search), and routing by host, against a fake store.
- The image was built on Linux x86_64 (Docker 29) with no glibc symbol newer than 2.36.
- Live, inside the platform (`integrations/service/tests/live_platform.rs`, run once on 2026-11-19): the services'
  OTLP through the collector and the gateway reached Prometheus, Loki and Tempo, and read back through the query face
  their newest metric sample, log line and trace were stamped in world time (10 minutes behind the wall), not wall
  time. Tempo needs `ingestion_time_range_slack` raised (the template's `tempo/*.yaml` set 168h): with its 2 m default
  it kept world-stamped traces but its searches left out every trace more than 2 m from its own clock.
