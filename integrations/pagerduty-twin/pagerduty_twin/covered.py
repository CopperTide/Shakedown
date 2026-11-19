"""The operations the twin serves, `(api, METHOD, path)` as PagerDuty's descriptions name them:
`rest` (REST API v2) and `events` (Events API v2, served under `/v2`)."""
EVENTS_PREFIX = "/v2"

COVERED = [
    ("rest", "GET", "/incidents"),
    ("rest", "GET", "/incidents/{id}"),
    ("rest", "PUT", "/incidents/{id}"),
    ("rest", "GET", "/incidents/{id}/notes"),
    ("rest", "POST", "/incidents/{id}/notes"),
    ("rest", "GET", "/services"),
    ("rest", "GET", "/oncalls"),
    ("rest", "GET", "/users"),
    ("rest", "GET", "/users/me"),
    ("rest", "GET", "/schedules"),
    ("events", "POST", "/enqueue"),
]


def router_ops(router) -> list[tuple[str, str, str]]:
    return [("events", r.method, r.pattern[len(EVENTS_PREFIX):]) if r.pattern.startswith(EVENTS_PREFIX + "/")
            else ("rest", r.method, r.pattern) for r in router.routes]
