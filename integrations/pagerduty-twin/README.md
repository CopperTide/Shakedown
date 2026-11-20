# pagerduty-twin

**A twin.** Puts a PagerDuty-compatible pager in the world: the REST API v2 subset agents and the `pagerduty`
Python client use (incidents, notes, services, on-call, users, schedules) and the Events API v2
(`POST /v2/enqueue`). Agents reach it only through shaked's recording proxy, on the host names its config gives
(`docs/protocol.md`, "Serving HTTP"): the first host is the REST API, the second the Events API (one server
answers both). `describe.system` is `{kind: "paging", product: "PagerDuty twin (REST v2, Events v2)",
stands_in_for: "PagerDuty", simulated: true}`. Restore: exact from a save (one SQLite store); saved at save
points; `uses_clock`: true, and every time it stamps, and who is on call, is world time.

REST needs the token (`Authorization: Token token=…` or `Bearer`; else `401 {"error": {"message":
"Authentication failed", "code": 2006}}`). A write (`PUT /incidents/{id}`, `POST …/notes`) must name a user's email
in `From` (else `400 {"error": {"message": "You must specify a user's email address in the \"From\" header",
"code": 2001}}`); it acts as the agent the proxy names in `X-Shakedown-Agent`, else as the `From` user. `users/me` is
the agent, else the config's `default_user`. The Events API takes the service's integration key as
`routing_key`. Its routes say `honours_readonly`: a request marked `X-Shakedown-Readonly: 1` gets its reads (marked
`X-Shakedown-Effect: read`), and its writes answer `403 {"error": {"message": "Access Denied", "code": 2010}}` (events:
`403 {"status": "forbidden", "message": "Access Denied", …}`) marked a read.

## Config

| key | what |
|---|---|
| `hosts` | `[{host, env?}]`: REST host, then Events host; each route's env gets `PAGERDUTY_API_URL`, `PAGERDUTY_EVENTS_URL` (`…/v2/enqueue`), `PAGERDUTY_TOKEN` |
| `token`, `default_user`, `email_domain` | the one REST token; who `users/me` is without an agent; users' addresses `<name>@<email_domain>` |
| `users` | `[{name, display_name, email?}]` |
| `schedules` | `[{name, rotation: [user], shift_hours (default 168), start}]`: on call is `rotation[(now − start) // shift % n]`, `start` relative to world now (`-1h`) |
| `services` | `[{name, schedule, integration_key, auto_resolve?}]` (32 characters); each gets an escalation policy paging its schedule; `auto_resolve: false` makes a resolve event only clear the incident's alerts (the `alerts_cleared` notice), so a person resolves it (default true: the event resolves it, as Events v2 does) |

## What it serves

`GET /incidents` (`statuses[]`, `service_ids[]`, `user_ids[]`, `incident_key`, `since`, `until`; classic `limit`/
`offset`/`more`/`total`), `GET|PUT /incidents/{id}` (acknowledge, resolve, title), `GET|POST /incidents/{id}/notes`,
`GET /services` (`query`), `/oncalls`, `/users`, `/users/me`, `/schedules`; `POST /v2/enqueue` (`trigger`,
`acknowledge`, `resolve`; an open incident with the same `dedup_key` on the service gets the alert instead of a
new incident; `202 {"status": "success", "message": "Event processed", "dedup_key"}`; invalid events
`400 {"status": "invalid event", …}`). Incidents are `P<number>`. Others answer `404 {"error": {"message": "Not
Found", "code": 2100}}`. Answers follow PagerDuty's descriptions (`tests/schemas/SOURCE.md`).

## Tools and notices

`pagerduty_event {action, dedup_key, summary, service, severity, source?}` (called by the alert link; an enqueue
body with `routing_key` also works), and `pagerduty_act {person, action, args}`: `ack {incident}`,
`resolve {incident}`, `note {incident, text}`. At the next `advance` the twin reports `notice` events,
`data = {system: "pagerduty", kind, data}`: `incident_triggered {incident, service, assignee, summary}`, and
`alerts_cleared` (same fields) when a resolve event clears an open incident's alerts (not when a person resolves it).

## Docs

`incident/<id>` `{status, service, created_ms, title, dedup_key, assignee, acknowledged_by, resolved_ms, notes}`.

## Verified

`uv run --with pytest pytest -q` (27 tests): Events v2 dedup and resolve (and its `alerts_cleared` notice; with `auto_resolve: false` the incident stays open for a person), the `From` rule, acknowledge, resolve and
notes, filters and paging, on-call rotation in world time, the world tool and acts, read-only agents, every covered
answer against PagerDuty's descriptions (fetched and pinned), and the `pagerduty` client 7.2 (`RestApiV2Client`),
unchanged, listing, acknowledging (`rput` on the incident's `self`) and writing notes against the twin through a
transport standing in for the proxy. Not yet verified through shaked's proxy.
