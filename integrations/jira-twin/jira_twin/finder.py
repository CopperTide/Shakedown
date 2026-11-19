"""Searching: JQL over the instance's issues, with names resolved against what it holds and dates
against world now."""
from . import jql, records, users
from .errors import JiraError
from .workflow import category_by


def _project(db):
    def resolve(value: str):
        row = records.project(db, value)
        return row["key"] if row else next((r["key"] for r in records.projects(db) if r["name"].lower() == value.lower()), None)

    return resolve


def _user(db):
    def resolve(value: str):
        row = users.find(db, value) or db.execute("SELECT * FROM users WHERE lower(display_name) = lower(?)",
                                                  (value,)).fetchone()
        return row["name"] if row else None

    return resolve


def _sprint(db):
    def resolve(value: str):
        row = db.execute("SELECT id FROM sprints WHERE id = ? OR lower(name) = lower(?)",
                         (int(value) if value.isdigit() else -1, value)).fetchone()
        return row[0] if row else None

    return resolve


def _named(find):
    return lambda value: (lambda x: x.name if x else None)(find(value))


def context(view, user: str | None) -> jql.Context:
    db, wf, meta = view.db, view.workflow, view.meta
    return jql.Context(
        now_ms=view.now(), user=user,
        resolve={"project": _project(db), "user": _user(db), "sprint": _sprint(db), "category": category_by,
                 "status": _named(wf.status), "type": _named(meta.type), "priority": _named(meta.priority),
                 "resolution": _named(meta.resolution), "labels": lambda v: v, "none": lambda v: None},
        sprint_states={r["id"]: r["state"] for r in db.execute("SELECT id, state FROM sprints")},
        category_of=lambda s: (wf.status(s) or wf.initial()).category,
        status_order=[s.name for s in wf.statuses], priority_rank=meta.priority_rank)


def find(view, user: str | None, text: str, rows: list | None = None) -> list:
    """The issue rows `text` selects, in its order; 400 in Jira's words for a query it refuses."""
    q = jql.parse(text or "")
    if isinstance(q, jql.JqlError):
        raise JiraError(400, [q.message])
    rows = records.issues(view.db) if rows is None else rows
    cache: dict = {}
    by_id = {r["id"]: r for r in rows}
    recs = [records.record(view.db, r, cache) for r in rows]
    try:
        found = jql.evaluate(q, recs, context(view, user))
    except jql.JqlError as e:
        raise JiraError(400, [e.message]) from None
    return [by_id[r["id"]] for r in found]
