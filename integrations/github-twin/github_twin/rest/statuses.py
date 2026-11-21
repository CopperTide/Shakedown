"""Commit statuses: post one, the latest of each context combined, and every one."""
from .. import ci, webhooks
from ..errors import invalid
from ..repos import full_name
from ..times import iso
from ..users import node_id
from . import shapes
from .api import created, paged
from .issue_shapes import user

STATES = ("error", "failure", "pending", "success")


def status_json(call, repo, s) -> dict:
    return {"id": s["id"], "node_id": node_id("StatusContext", s["id"]), "state": s["state"], "context": s["context"],
            "description": s["description"], "target_url": s["target_url"], "avatar_url": None,
            "url": call.urls.repo_api(full_name(repo), f"/statuses/{s['sha']}"), "creator": user(call, s["creator"]),
            "created_at": iso(s["created_ms"]), "updated_at": iso(s["created_ms"])}


def _sha(call, repo) -> str:
    ref = call.req.params.get("ref") or call.req.params.get("sha")
    sha = call.repos.ref(full_name(repo), ref)
    if sha is None:
        raise invalid(f"No commit found for SHA: {ref}")
    return sha


def create_status(call):
    repo, b = call.repo(), call.body()
    if b.get("state") not in STATES:
        raise invalid("Validation Failed", [{"resource": "Status", "field": "state", "code": "custom"}])
    row = post(call, repo, _sha(call, repo), b["state"], b.get("context") or "default", b.get("description"),
               b.get("target_url"), call.login)
    return created(status_json(call, repo, row))


def post(v, repo, sha: str, state: str, context: str, description: str | None, target_url: str | None, creator: str):
    """A new status on `sha` by `creator`, and its webhook; its row."""
    cur = v.db.execute("INSERT INTO statuses(repo_id, sha, state, context, description, target_url, creator, created_ms) "
                       "VALUES (?, ?, ?, ?, ?, ?, ?, ?)", (repo["id"], sha, state, context, description, target_url,
                                                           creator, v.now()))
    row = v.db.execute("SELECT * FROM statuses WHERE id = ?", (cur.lastrowid,)).fetchone()
    webhooks.fire(v, repo, "status", creator, None, sha=sha, **{k: row[k] for k in ("state", "context", "description", "target_url")})
    return row


def combined(call):
    repo = call.repo()
    sha, fn = _sha(call, repo), full_name(repo)
    latest = ci.latest_statuses(call.db, repo["id"], sha)
    return {"state": ci.combined_state(latest), "sha": sha, "total_count": len(latest),
            "statuses": [status_json(call, repo, s) for s in latest], "repository": shapes.repository(call, repo),
            "commit_url": call.urls.repo_api(fn, f"/commits/{sha}"), "url": call.urls.repo_api(fn, f"/commits/{sha}/status")}


def list_statuses(call):
    repo = call.repo()
    rows = call.db.execute("SELECT * FROM statuses WHERE repo_id = ? AND sha = ? ORDER BY id DESC",
                           (repo["id"], _sha(call, repo))).fetchall()
    resp = paged(call, rows)
    resp.body = [status_json(call, repo, s) for s in resp.body]
    return resp


def register(api) -> None:
    api.route("POST", "/repos/{owner}/{repo}/statuses/{sha}")(create_status)
    api.route("GET", "/repos/{owner}/{repo}/commits/{ref}/status")(combined)
    api.route("GET", "/repos/{owner}/{repo}/commits/{ref}/statuses")(list_statuses)
