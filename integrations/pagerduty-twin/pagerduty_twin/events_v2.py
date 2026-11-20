"""The Events API v2 (`POST /v2/enqueue`, authorised by the service's integration key as
`routing_key`), and the same event from the alert link (`pagerduty_event`). An event without a dedup
key gets one made from its service, summary and world time."""
import hashlib

from shakedown_part.twin import Done, Response

from . import incidents, ids, records
from .errors import EventError, PagerDutyError

ACTIONS = ("trigger", "acknowledge", "resolve")
SEVERITIES = ("critical", "error", "warning", "info")


def _dedup(ctx, service, summary: str) -> str:
    return hashlib.sha256(f"{service['id']}:{summary}:{ctx.now_ms()}".encode()).hexdigest()[:32]


def apply(ctx, service, action: str, dedup_key: str | None, payload: dict):
    """One event on `service`; the incident it touched (None when there was none to touch) and its dedup key."""
    if action not in ACTIONS:
        raise EventError("'event_action' must be one of trigger, acknowledge, resolve.")
    if action == "trigger":
        missing = [f"'payload.{f}' is missing or blank." for f in ("summary", "source", "severity") if not payload.get(f)]
        if missing:
            raise EventError(*missing)
        if payload["severity"] not in SEVERITIES:
            raise EventError("'payload.severity' must be one of critical, error, warning, info.")
        key = dedup_key or _dedup(ctx, service, payload["summary"])
        return incidents.trigger(ctx, service, key, payload["summary"], payload["severity"]), key
    if not dedup_key:
        raise EventError("'dedup_key' is required for acknowledge and resolve events.")
    row = records.open_incident(ctx.store.conn, service["id"], dedup_key)
    if row is None:
        return None, dedup_key
    if action == "resolve":
        incidents.cleared(ctx, row, service)
        if not _auto_resolves(ctx.config, service):
            return records.incident(ctx.store.conn, ids.incident(row["number"])), dedup_key
    target = "acknowledged" if action == "acknowledge" else "resolved"
    return incidents.change(ctx, row, target, f"service:{service['id']}"), dedup_key


def _auto_resolves(config: dict, service) -> bool:
    """Whether a resolve event resolves the service's incident (Events v2's behaviour, the default) or only clears
    its alerts, leaving a person to resolve it (`auto_resolve: false` in the service's config)."""
    spec = next((s for s in config.get("services", []) if s["name"] == service["name"]), {})
    return bool(spec.get("auto_resolve", True))


def enqueue(req, ctx) -> Response:
    if req.readonly:
        return Response(403, EventError("A read-only agent may not send events.", status=403,
                                         message="Access Denied").body(), effect="read")
    try:
        body = req.json()
        service = records.service(ctx.store.conn, str(body.get("routing_key", ""))) if isinstance(body, dict) else None
        if service is None or len(str(body.get("routing_key", ""))) != 32:
            raise EventError("'routing_key' is not a valid integration key.")
        _, key = apply(ctx, service, body.get("event_action", ""), body.get("dedup_key"), body.get("payload") or {})
    except (ValueError, EventError) as e:
        err = e if isinstance(e, EventError) else EventError("The request body is not JSON.")
        return Response(err.status, err.body(), effect="read")
    return Response(202, {"status": "success", "message": "Event processed", "dedup_key": key})


TOOL = {"name": "pagerduty_event", "effect": "write",
        "description": "An alert's event (trigger, acknowledge, resolve) on a service, as the Events API takes it "
                       "(called by the alert link).",
        "inputSchema": {"type": "object", "required": ["service"], "properties": {
            "action": {"type": "string", "enum": list(ACTIONS)}, "dedup_key": {"type": "string"},
            "summary": {"type": "string"}, "service": {"type": "string"}, "severity": {"type": "string"},
            "source": {"type": "string"}}}}


def tool(ctx, args: dict) -> Done:
    """`{action, dedup_key, summary, service, severity, source?}` (or an enqueue body with `routing_key`)."""
    service = records.service(ctx.store.conn, str(args.get("service") or args.get("routing_key") or ""))
    if service is None:
        return Done(error=f"no service {args.get('service') or args.get('routing_key')!r}")
    payload = args.get("payload") or {"summary": args.get("summary"), "source": args.get("source") or "alertmanager",
                                      "severity": args.get("severity") or "error"}
    try:
        row, _ = apply(ctx, service, args.get("action") or args.get("event_action", ""), args.get("dedup_key"), payload)
    except (EventError, PagerDutyError) as e:
        return Done(error="; ".join(getattr(e, "errors", None) or [str(e)]))
    if row is None:
        return Done({"incident": None, "status": None})
    incident_id = ids.incident(row["number"])
    return Done({"incident": incident_id, "status": row["status"]}, writes=[f"incident/{incident_id}"])

