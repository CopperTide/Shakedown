"""The writes, shared by REST, the Events API, `pagerduty_event` and `pagerduty_act`: an event
triggers an incident (or adds an alert to the open one with its dedup key) paged to the on-call user,
and acknowledges or resolves it; people acknowledge, resolve and write notes."""
from . import ids, oncall, records
from .errors import PagerDutyError

STATUSES = ("triggered", "acknowledged", "resolved")


def trigger(ctx, service, dedup_key: str, summary: str, severity: str):
    db, now = ctx.store.conn, ctx.now_ms()
    found = records.open_incident(db, service["id"], dedup_key)
    if found is not None:
        db.execute("UPDATE incidents SET alerts = alerts + 1, updated_ms = ? WHERE number = ?", (now, found["number"]))
        return records.incident(db, ids.incident(found["number"]))
    on_call = oncall.for_service(db, service, now)
    assignee = on_call.user if on_call else None
    cur = db.execute("INSERT INTO incidents(service_id, title, dedup_key, severity, status, assignee, created_ms, "
                     "updated_ms, changed_ms, changed_by) VALUES (?, ?, ?, ?, 'triggered', ?, ?, ?, ?, ?)",
                     (service["id"], summary, dedup_key, severity, assignee, now, now, now, f"service:{service['id']}"))
    incident_id = ids.incident(cur.lastrowid)
    ctx.notify("incident_triggered", f"pagerduty incident {incident_id} on {service['name']}",
               {"incident": incident_id, "service": service["name"], "assignee": assignee, "summary": summary})
    return records.incident(db, incident_id)


def change(ctx, row, status: str, by: str):
    """Moves an incident to `status` (`by` is `user:<name>` or `service:<id>`); a resolved one stays resolved."""
    if status not in STATUSES[1:]:
        raise PagerDutyError(400, "Invalid Input Provided", 2001, [f"Status '{status}' is not one of acknowledged, resolved."])
    if row["status"] == "resolved" or row["status"] == status:
        return row
    db, now = ctx.store.conn, ctx.now_ms()
    if status == "acknowledged":
        db.execute("UPDATE incidents SET status = ?, acked_by = ?, acked_ms = ?, updated_ms = ?, changed_ms = ?, "
                   "changed_by = ? WHERE number = ?", (status, by, now, now, now, by, row["number"]))
    else:
        db.execute("UPDATE incidents SET status = ?, resolved_ms = ?, updated_ms = ?, changed_ms = ?, changed_by = ? "
                   "WHERE number = ?", (status, now, now, now, by, row["number"]))
    return records.incident(db, ids.incident(row["number"]))


def note(ctx, row, user: str, content: str):
    if not content:
        raise PagerDutyError(400, "Invalid Input Provided", 2001, ["Note content can't be blank."])
    db = ctx.store.conn
    cur = db.execute("INSERT INTO notes(incident, user, content, created_ms) VALUES (?, ?, ?, ?)",
                     (row["number"], user, content, ctx.now_ms()))
    db.execute("UPDATE incidents SET updated_ms = ? WHERE number = ?", (ctx.now_ms(), row["number"]))
    return db.execute("SELECT * FROM notes WHERE id = ?", (cur.lastrowid,)).fetchone()
