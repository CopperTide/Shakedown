"""Issues and comments: what the REST API and the people's acts do to them, in one place."""
from . import notices, records, users, webhooks
from .errors import invalid
from .repos import full_name
from .rest import issue_shapes, pull_shapes


def _hook(v, repo, row, action: str, sender: str, **extra) -> None:
    event = "pull_request" if row["is_pull"] else "issues"
    key = "pull_request" if row["is_pull"] else "issue"
    payload = {key: issue_shapes.issue(v, repo, row)}
    if row["is_pull"]:
        payload = {"number": row["number"], "pull_request": pull_shapes.simple(v, repo, records.pull(v.db, repo["id"], row["number"]))}
    webhooks.fire(v, repo, event, sender, action, **payload, **extra)


def create(v, repo, by: str, title: str, body: str | None, labels: list, assignees: list):
    if not title:
        raise invalid("Validation Failed", [{"resource": "Issue", "field": "title", "code": "missing_field"}])
    row = records.insert_issue(v.db, repo["id"], title, body, by, v.now())
    records.add_labels(v.db, repo["id"], row["id"], labels)
    records.set_assignees(v.db, row["id"], assignees)
    for login in assignees:
        users.ensure(v.db, login, v.now())
    notices.mentions(v.ctx, full_name(repo), row["number"], by, body)
    _hook(v, repo, row, "opened", by)
    return records.issue(v.db, repo["id"], row["number"])


def set_state(v, repo, row, state: str, by: str, reason: str | None = None) -> None:
    if state == row["state"]:
        return
    if state == "closed":
        records.close(v.db, row["id"], by, v.now())
    else:
        records.reopen(v.db, row["id"], v.now())
    records.touch(v.db, row["id"], v.now(), state_reason=reason if state == "closed" else "reopened")
    _hook(v, repo, records.issue(v.db, repo["id"], row["number"]), "closed" if state == "closed" else "reopened", by)


def comment(v, repo, row, by: str, body: str):
    if not body:
        raise invalid("Validation Failed", [{"resource": "IssueComment", "field": "body", "code": "missing_field"}])
    now = v.now()
    cur = v.db.execute("INSERT INTO comments(issue_id, user, body, created_ms, updated_ms) VALUES (?, ?, ?, ?, ?)",
                       (row["id"], by, body, now, now))
    records.touch(v.db, row["id"], now)
    c = v.db.execute("SELECT * FROM comments WHERE id = ?", (cur.lastrowid,)).fetchone()
    notices.mentions(v.ctx, full_name(repo), row["number"], by, body)
    webhooks.fire(v, repo, "issue_comment", by, "created", issue=issue_shapes.issue(v, repo, row),
                  comment=issue_shapes.comment(v, repo, row, c))
    return c


def edit(v, repo, row, by: str, changes: dict) -> None:
    """Apply what `changes` holds of title, body, labels, assignees, state and state_reason, as REST's
    PATCH and GraphQL's updates do."""
    fields = {k: changes[k] for k in ("title", "body") if k in changes}
    if fields:
        records.touch(v.db, row["id"], v.now(), **fields)
    if "labels" in changes:
        records.set_labels(v.db, repo["id"], row["id"], changes["labels"])
    if "assignees" in changes:
        records.set_assignees(v.db, row["id"], changes["assignees"])
        for login in changes["assignees"]:
            users.ensure(v.db, login, v.now())
    if changes.get("state") in ("open", "closed"):
        set_state(v, repo, row, changes["state"], by, changes.get("state_reason") or "completed")
