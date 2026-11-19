"""The REST API v2 subset: incidents (list with filters, show, acknowledge or resolve), their notes,
services, on-calls, users (and `users/me`) and schedules."""
from . import incidents, records, times
from .api import page
from .errors import invalid


def _filter(call, rows: list) -> list:
    statuses, services = call.list_arg("statuses"), call.list_arg("service_ids")
    users, key = call.list_arg("user_ids"), call.req.arg("incident_key")
    since, until = (times.parse(call.req.arg(n)) if call.req.arg(n) else None for n in ("since", "until"))
    if (call.req.arg("since") and since is None) or (call.req.arg("until") and until is None):
        raise invalid("Invalid 'since' or 'until' parameter values.")
    user_names = {r["name"] for r in records.users(call.db) if r["id"] in users}
    return [r for r in rows if (not statuses or r["status"] in statuses) and (not services or r["service_id"] in services)
            and (not users or r["assignee"] in user_names) and (not key or r["dedup_key"] == key)
            and (since is None or r["created_ms"] >= since) and (until is None or r["created_ms"] < until)]


def list_incidents(call):
    rows = call.db.execute("SELECT * FROM incidents ORDER BY number").fetchall()
    found = _filter(call, rows)
    return page(call, "incidents", [call.shapes.incident(r) for r in found])


def show_incident(call):
    return {"incident": call.shapes.incident(records.incident(call.db, call.req.params["id"]))}


def update_incident(call):
    row = records.incident(call.db, call.req.params["id"])
    body = call.body().get("incident") or {}
    who = call.sender()
    if body.get("status"):
        row = incidents.change(call.ctx, row, body["status"], f"user:{who}")
    if body.get("title"):
        call.db.execute("UPDATE incidents SET title = ? WHERE number = ?", (body["title"], row["number"]))
        row = records.incident(call.db, call.req.params["id"])
    return {"incident": call.shapes.incident(row)}


def list_notes(call):
    row = records.incident(call.db, call.req.params["id"])
    return {"notes": [call.shapes.note(n) for n in records.notes(call.db, row["number"])]}


def add_note(call):
    row = records.incident(call.db, call.req.params["id"])
    note = incidents.note(call.ctx, row, call.sender(), (call.body().get("note") or {}).get("content", ""))
    return {"note": call.shapes.note(note)}


def list_services(call):
    query = (call.req.arg("query") or "").lower()
    rows = [r for r in records.services(call.db) if query in r["name"].lower()]
    return page(call, "services", [call.shapes.service(r) for r in rows])


def list_oncalls(call):
    at = times.parse(call.req.arg("since")) if call.req.arg("since") else call.ctx.now_ms()
    out = []
    for service in records.services(call.db):
        schedule = records.schedule(call.db, service["schedule_id"]) if service["schedule_id"] else None
        entry = call.shapes.oncall(service, schedule, at) if schedule else None
        if entry is not None:
            out.append(entry)
    users, schedules = call.list_arg("user_ids"), call.list_arg("schedule_ids")
    out = [o for o in out if (not users or o["user"]["id"] in users) and (not schedules or o["schedule"]["id"] in schedules)]
    return page(call, "oncalls", out)


def list_users(call):
    query = (call.req.arg("query") or "").lower()
    rows = [r for r in records.users(call.db) if query in r["display_name"].lower() or query in r["email"].lower()]
    return page(call, "users", [call.shapes.user(r) for r in rows])


def list_schedules(call):
    return page(call, "schedules", [call.shapes.schedule(r) for r in records.schedules(call.db)])


ROUTES = [("GET", "/incidents", list_incidents), ("GET", "/incidents/{id}", show_incident),
          ("PUT", "/incidents/{id}", update_incident), ("GET", "/incidents/{id}/notes", list_notes),
          ("POST", "/incidents/{id}/notes", add_note), ("GET", "/services", list_services),
          ("GET", "/oncalls", list_oncalls), ("GET", "/users", list_users),
          ("GET", "/users/me", lambda call: {"user": call.shapes.user(call.me())}),
          ("GET", "/schedules", list_schedules)]
