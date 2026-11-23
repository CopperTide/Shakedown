# PagerDuty twin (`pagerduty-twin`)

Puts a PagerDuty-compatible pager in a Shakedown world: a subset of the REST API v2 (incidents, notes, services,
on-call, users, schedules) and the Events API v2 (`POST /v2/enqueue`). Agents use their own clients (the
`pagerduty` Python client, plain REST) and reach it through `shaked`'s recording proxy, on the host names its
config gives. Alerts reach it through a link to its world tool. The software-factory world template uses it to
page on-call engineers when production's error rate spikes.

| | |
|---|---|
| Program | `python -m pagerduty_twin` (Python, run with `uv`) |
| Stands in for | PagerDuty: the eleven REST and Events operations listed below, not the whole product |
| Restore | exact, from a save (one SQLite store). Saved at save points. No approximate restore. |
| World clock | used: every time it stamps, and who is on call, is world time. Notices are reported at `advance`. |
| Agent tools | none. Agents call it over HTTP. `pagerduty_act` and `pagerduty_event` are world tools that links call. |

## What it needs

- `uv`. No services, no Docker, no GPU and no fixed ports: each instance listens on its own port on `127.0.0.1`,
  and `shaked` routes the world's host names to it ([`docs/protocol.md`](../../docs/protocol.md), "Serving HTTP").

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | `saves` (in the working directory) | Where saves and the live instances (`.live/`) are kept. `shaked` always passes its own. |

## Use it

A world names it as a part, for example `{part: "pagerduty", integration: "pagerduty-twin", config: {...}}`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `hosts` | array | required | `[{host, env?}]`: the REST host, then the Events host (one server answers both). Each route's `env` gets `PAGERDUTY_API_URL`, `PAGERDUTY_EVENTS_URL` (`.../v2/enqueue` on the second host) and `PAGERDUTY_TOKEN`. `env` adds more. |
| `token` | string | required | The one REST token. |
| `default_user` | string | required | Who `users/me` is for a request without an agent. |
| `email_domain` | string | `example.test` | Users' addresses are `<name>@<email_domain>` unless given. |
| `users` | array | none | `[{name, display_name, email?}]`. Users in a rotation are made too. |
| `schedules` | array | none | `[{name, rotation: [user], shift_hours, start}]`. `shift_hours` defaults to 168. On call is `rotation[(now - start) // shift % n]`, with `start` relative to world now (`-1h`). |
| `services` | array | none | `[{name, schedule, integration_key, auto_resolve?}]`. Each gets an escalation policy that pages its schedule. `integration_key` must be 32 characters for the Events API. `auto_resolve` defaults to true. |

### Who a request is

REST needs the token (`Authorization: Token token=...` or `Bearer`), else `401 {"error": {"message":
"Authentication failed", "code": 2006}}`. A write (`PUT /incidents/{id}`, `POST .../notes`) must name a user's
email in `From`, else `400` with code `2001`. It acts as the agent the proxy names in `X-Shakedown-Agent`, else as
the `From` user. `users/me` is the agent, else `default_user`. The Events API takes the service's integration key
as `routing_key`. The routes say `honours_readonly`: a request marked `X-Shakedown-Readonly: 1` gets its reads,
and its writes answer `403 {"error": {"message": "Access Denied", "code": 2010}}` (an event answers
`403 {"status": "forbidden", "message": "Access Denied", ...}`), marked `X-Shakedown-Effect: read`.

### What it serves

| Operation | Notes |
|---|---|
| `GET /incidents` | `statuses[]`, `service_ids[]`, `user_ids[]`, `incident_key`, `since`, `until`. Classic paging: `limit`, `offset`, `more`, `total`. |
| `GET /incidents/{id}`, `PUT /incidents/{id}` | acknowledge, resolve, change the title |
| `GET /incidents/{id}/notes`, `POST /incidents/{id}/notes` | |
| `GET /services` | `query` |
| `GET /oncalls`, `GET /users`, `GET /users/me`, `GET /schedules` | |
| `POST /v2/enqueue` | `trigger`, `acknowledge`, `resolve`. An open incident with the same `dedup_key` on the service gets the alert instead of a new incident. Answers `202 {"status": "success", "message": "Event processed", "dedup_key"}`, or `400 {"status": "invalid event", ...}`. |

Incidents are `P<number>`. Any other path answers `404 {"error": {"message": "Not Found", "code": 2100}}`. The
operations are listed in `pagerduty_twin/covered.py`, and the answers follow PagerDuty's OpenAPI descriptions
(`tests/schemas/SOURCE.md`). With `auto_resolve: false`, a resolve event only clears an incident's alerts, so a
person must resolve it. By default the event resolves it, as Events v2 does.

### The world tools and notices

Both are called by links:

- `pagerduty_event {action, dedup_key, summary, service, severity, source?}`, from the world's alerting. An
  enqueue body with `routing_key` also works. `service` is a service's name, id or integration key.
- `pagerduty_act {person, action, args}`, usually for the world's simulated people (see
  [`people`](../people/README.md)): `ack {incident}`, `resolve {incident}` and `note {incident, text}`.

At the next `advance` the twin reports `notice` events with `data = {system: "pagerduty", kind, data}`:
`incident_triggered {incident, service, assignee, summary}`, and `alerts_cleared` (the same fields) when a resolve
event clears an open incident's alerts. A person resolving an incident reports no notice.

### inspect and diff

| Key | Doc |
|---|---|
| `incident/<id>` | `{status, service, created_ms, title, dedup_key, assignee, acknowledged_by, resolved_ms, notes}` (`notes` is a count) |

## How it's tested

```bash
cd integrations/pagerduty-twin
uv run --with pytest pytest -q
```

The tests need no services. They cover Events v2 dedup and resolve (with its `alerts_cleared` notice, and an
incident left open with `auto_resolve: false`), the `From` rule, acknowledge, resolve and notes, filters and paging,
the on-call rotation in world time, the world tools and acts, and read-only agents. They run the `pagerduty` client
7.2 (`RestApiV2Client`) unchanged: it lists incidents, acknowledges one (`rput` on its `self`) and writes notes
through a transport that stands in for the proxy. `tests/test_schema.py` checks every covered answer against
PagerDuty's OpenAPI descriptions. They are not committed: the test fetches them from a pinned commit of
[PagerDuty/api-schema](https://github.com/PagerDuty/api-schema) on first use into `tests/.vendor-cache/`, checks
their sha256, and skips, saying why, when it cannot.

The twin is a part of the software-factory world. Its live tests under `shaked`
(`benchmarks/world/tests/software_factory/test_live_world.py`) include an error spike that pages on-call. The
commit that added them records them as passed once, on 2026-11-20. No test drives the `pagerduty` client through
`shaked`'s proxy.

## Limits

- Eleven operations. No escalation policies' or schedules' writes, overrides, maintenance windows, priorities,
  responders, status updates, analytics or webhooks.
- One escalation level: a service pages its schedule's on-call user.
- A schedule is one rotation with a fixed shift length. No layers or restrictions.
- One REST token for everyone. Who a request is comes from the proxy's agent header or `From`.
