"""The docs `inspect` reads (and checks score): each issue and each sprint."""
from . import records
from .workflow import Workflow


def _issue(db, row, wf: Workflow) -> dict:
    status = wf.status(row["status"])
    return {"status": row["status"], "status_category": status.category if status else None,
            "sprint": row["sprint_id"], "assignee": row["assignee"], "resolution": row["resolution"],
            "summary": row["summary"], "type": row["type"], "priority": row["priority"],
            "labels": records.labels(db, row["id"])}


def build(ctx) -> dict:
    db, wf, out = ctx.store.conn, Workflow(ctx.config.get("workflow")), {}
    for row in records.issues(db):
        out[f"issue/{row['key']}"] = _issue(db, row, wf)
    for s in db.execute("SELECT * FROM sprints ORDER BY id").fetchall():
        keys = [r["key"] for r in records.issues(db, "sprint_id = ?", (s["id"],))]
        out[f"sprint/{s['id']}"] = {"name": s["name"], "state": s["state"], "issues": keys}
    return out
