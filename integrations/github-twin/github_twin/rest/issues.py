"""Issues and their comments (pull requests are issues too)."""
from .. import issue_rules, records
from ..errors import not_found
from . import issue_shapes
from .api import created, no_content, paged


def the_issue(call):
    repo = call.repo()
    try:
        return repo, records.issue(call.db, repo["id"], int(call.req.params["issue_number"]))
    except ValueError:
        raise not_found() from None


def create_issue(call):
    repo, b = call.repo(), call.body()
    assignees = b.get("assignees") or ([b["assignee"]] if b.get("assignee") else [])
    row = issue_rules.create(call, repo, call.login, b.get("title", ""), b.get("body"), b.get("labels") or [], assignees)
    return created(issue_shapes.issue(call, repo, row))


def _matches(call, row) -> bool:
    state = call.req.arg("state", "open")
    if state != "all" and row["state"] != state:
        return False
    wanted = [x for x in (call.req.arg("labels") or "").split(",") if x]
    have = {x["name"] for x in records.labels_of(call.db, row["id"])}
    return set(wanted) <= have


def list_issues(call):
    repo = call.repo()
    rows = call.db.execute("SELECT * FROM issues WHERE repo_id = ? ORDER BY number DESC", (repo["id"],)).fetchall()
    resp = paged(call, [r for r in rows if _matches(call, r)])
    resp.body = [issue_shapes.issue(call, repo, r) for r in resp.body]
    return resp


def get_issue(call):
    repo, row = the_issue(call)
    return issue_shapes.issue(call, repo, row)


def update_issue(call):
    repo, row = the_issue(call)
    b = call.body()
    fields = {k: b[k] for k in ("title", "body") if k in b}
    if fields:
        records.touch(call.db, row["id"], call.now(), **fields)
    if "labels" in b:
        records.set_labels(call.db, repo["id"], row["id"], [x if isinstance(x, str) else x["name"] for x in b["labels"]])
    if "assignees" in b:
        records.set_assignees(call.db, row["id"], b["assignees"] or [])
    if b.get("state") in ("open", "closed"):
        issue_rules.set_state(call, repo, row, b["state"], call.login, b.get("state_reason") or "completed")
    return issue_shapes.issue(call, repo, records.issue(call.db, repo["id"], row["number"]))


def create_comment(call):
    repo, row = the_issue(call)
    c = issue_rules.comment(call, repo, row, call.login, call.body().get("body", ""))
    return created(issue_shapes.comment(call, repo, row, c))


def list_comments(call):
    repo, row = the_issue(call)
    rows = call.db.execute("SELECT * FROM comments WHERE issue_id = ? ORDER BY id", (row["id"],)).fetchall()
    resp = paged(call, rows)
    resp.body = [issue_shapes.comment(call, repo, row, c) for c in resp.body]
    return resp


def _comment(call):
    repo = call.repo()
    c = call.db.execute("SELECT c.* FROM comments c JOIN issues i ON i.id = c.issue_id WHERE c.id = ? AND i.repo_id = ?",
                        (int(call.req.params["comment_id"]), repo["id"])).fetchone()
    if c is None:
        raise not_found()
    return repo, c, call.db.execute("SELECT * FROM issues WHERE id = ?", (c["issue_id"],)).fetchone()


def update_comment(call):
    repo, c, row = _comment(call)
    call.db.execute("UPDATE comments SET body = ?, updated_ms = ? WHERE id = ?", (call.body().get("body", ""), call.now(), c["id"]))
    return issue_shapes.comment(call, repo, row, call.db.execute("SELECT * FROM comments WHERE id = ?", (c["id"],)).fetchone())


def delete_comment(call):
    _, c, _ = _comment(call)
    call.db.execute("DELETE FROM comments WHERE id = ?", (c["id"],))
    return no_content()


def register(api) -> None:
    p = "/repos/{owner}/{repo}/issues"
    api.route("POST", p)(create_issue)
    api.route("GET", p)(list_issues)
    api.route("GET", p + "/{issue_number}")(get_issue)
    api.route("PATCH", p + "/{issue_number}")(update_issue)
    api.route("POST", p + "/{issue_number}/comments")(create_comment)
    api.route("GET", p + "/{issue_number}/comments")(list_comments)
    api.route("PATCH", p + "/comments/{comment_id}")(update_comment)
    api.route("DELETE", p + "/comments/{comment_id}")(delete_comment)
