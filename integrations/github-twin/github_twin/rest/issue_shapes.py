"""The JSON of issues, issue comments and labels."""
from urllib.parse import quote

from .. import records, users
from ..repos import full_name
from ..times import iso
from ..users import node_id


def user(v, login: str | None):
    if not login:
        return None
    return users.simple_user(v.urls, users.find(v.db, login) or users.ensure(v.db, login, v.now()))


def label(v, repo, row) -> dict:
    return {"id": row["id"], "node_id": node_id("Label", row["id"]), "name": row["name"], "color": row["color"],
            "description": row["description"], "default": False, "archived_at": None, "archived_by": None,
            "url": v.urls.repo_api(full_name(repo), f"/labels/{quote(row['name'])}")}


def times_of(row) -> dict:
    return {"created_at": iso(row["created_ms"]), "updated_at": iso(row["updated_ms"]),
            "closed_at": iso(row["closed_ms"]) if row["closed_ms"] else None}


def _pull_link(v, repo, row) -> dict:
    fn, n = full_name(repo), row["number"]
    merged = v.db.execute("SELECT merged_ms FROM pulls WHERE issue_id = ?", (row["id"],)).fetchone()
    return {"url": v.urls.repo_api(fn, f"/pulls/{n}"), "html_url": v.urls.repo_html(fn, f"/pull/{n}"),
            "diff_url": v.urls.repo_html(fn, f"/pull/{n}.diff"), "patch_url": v.urls.repo_html(fn, f"/pull/{n}.patch"),
            "merged_at": iso(merged["merged_ms"]) if merged and merged["merged_ms"] else None}


def issue(v, repo, row) -> dict:
    fn, n, u = full_name(repo), row["number"], v.urls
    assignees = records.assignees_of(v.db, row["id"])
    out = {"id": row["id"], "node_id": node_id("Issue", row["id"]), "number": n, "title": row["title"],
           "body": row["body"], "state": row["state"], "state_reason": row["state_reason"],
           "user": user(v, row["user"]), "labels": [label(v, repo, x) for x in records.labels_of(v.db, row["id"])],
           "assignee": user(v, assignees[0]) if assignees else None, "assignees": [user(v, a) for a in assignees],
           "milestone": None, "locked": bool(row["locked"]), "active_lock_reason": None,
           "comments": records.comment_count(v.db, row["id"]), "closed_by": user(v, row["closed_by"]),
           "author_association": "MEMBER", "url": u.repo_api(fn, f"/issues/{n}"),
           "repository_url": u.repo_api(fn), "labels_url": u.repo_api(fn, f"/issues/{n}/labels{{/name}}"),
           "comments_url": u.repo_api(fn, f"/issues/{n}/comments"), "events_url": u.repo_api(fn, f"/issues/{n}/events"),
           "html_url": u.repo_html(fn, f"/{'pull' if row['is_pull'] else 'issues'}/{n}"), **times_of(row)}
    if row["is_pull"]:
        out["pull_request"] = _pull_link(v, repo, row)
    return out


def comment(v, repo, issue_row, c) -> dict:
    fn = full_name(repo)
    return {"id": c["id"], "node_id": node_id("IssueComment", c["id"]), "body": c["body"], "user": user(v, c["user"]),
            "url": v.urls.repo_api(fn, f"/issues/comments/{c['id']}"),
            "html_url": v.urls.repo_html(fn, f"/issues/{issue_row['number']}#issuecomment-{c['id']}"),
            "issue_url": v.urls.repo_api(fn, f"/issues/{issue_row['number']}"), "author_association": "MEMBER",
            "created_at": iso(c["created_ms"]), "updated_at": iso(c["updated_ms"])}
