"""The docs `inspect` reads (and checks score): each incident."""
from . import ids


def build(ctx) -> dict:
    db, out = ctx.store.conn, {}
    rows = db.execute("SELECT i.*, s.name AS service_name FROM incidents i JOIN services s ON s.id = i.service_id "
                      "ORDER BY number").fetchall()
    for r in rows:
        acked_by = r["acked_by"].partition(":")[2] if r["acked_by"] and r["acked_by"].startswith("user:") else None
        out[f"incident/{ids.incident(r['number'])}"] = {
            "status": r["status"], "service": r["service_name"], "created_ms": r["created_ms"], "title": r["title"],
            "dedup_key": r["dedup_key"], "assignee": r["assignee"], "acknowledged_by": acked_by,
            "resolved_ms": r["resolved_ms"], "notes": db.execute("SELECT COUNT(*) FROM notes WHERE incident = ?",
                                                                 (r["number"],)).fetchone()[0]}
    return out
