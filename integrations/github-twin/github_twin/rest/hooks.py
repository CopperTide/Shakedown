"""Repository webhooks: create (with a ping), list, read, edit, delete, ping and test."""
import json

from .. import hooks_delivery
from ..errors import invalid, not_found
from ..repos import full_name
from ..times import iso
from . import shapes
from .api import created, no_content
from .issue_shapes import user

MASK = "********"
ZEN = "Keep it logically awesome."


def hook_json(call, repo, h) -> dict:
    fn, cfg = full_name(repo), json.loads(h["config"])
    api = call.urls.repo_api(fn, f"/hooks/{h['id']}")
    shown = {"content_type": cfg.get("content_type", "form"), "insecure_ssl": str(cfg.get("insecure_ssl", "0")),
             "url": cfg.get("url")}
    if cfg.get("secret"):
        shown["secret"] = MASK
    return {"type": "Repository", "id": h["id"], "name": h["name"], "active": bool(h["active"]),
            "events": json.loads(h["events"]), "config": shown, "url": api, "test_url": f"{api}/tests",
            "ping_url": f"{api}/pings", "deliveries_url": f"{api}/deliveries",
            "created_at": iso(h["created_ms"]), "updated_at": iso(h["updated_ms"]),
            "last_response": {"code": h["last_code"], "status": h["last_status"] or "unused", "message": h["last_message"]}}


def _hook(call, repo):
    h = call.db.execute("SELECT * FROM hooks WHERE id = ? AND repo_id = ?", (int(call.req.params["hook_id"]), repo["id"])).fetchone()
    if h is None:
        raise not_found()
    return h


def _ping(call, repo, h) -> None:
    hooks_delivery.queue_one(call.ctx, h, "ping", {"zen": ZEN, "hook_id": h["id"], "hook": hook_json(call, repo, h),
                                                    "repository": shapes.repository(call, repo), "sender": user(call, call.login)})


def create_hook(call):
    repo, b = call.repo(), call.body()
    cfg = b.get("config") or {}
    if not cfg.get("url"):
        raise invalid("Validation Failed", [{"resource": "Hook", "code": "custom", "message": "Config must contain url"}])
    now = call.now()
    cur = call.db.execute("INSERT INTO hooks(repo_id, name, active, events, config, created_ms, updated_ms) VALUES (?, ?, ?, ?, ?, ?, ?)",
                          (repo["id"], b.get("name", "web"), int(b.get("active", True)), json.dumps(b.get("events") or ["push"]),
                           json.dumps(cfg), now, now))
    h = call.db.execute("SELECT * FROM hooks WHERE id = ?", (cur.lastrowid,)).fetchone()
    _ping(call, repo, h)
    return created(hook_json(call, repo, h))


def list_hooks(call):
    repo = call.repo()
    return [hook_json(call, repo, h) for h in call.db.execute("SELECT * FROM hooks WHERE repo_id = ? ORDER BY id", (repo["id"],))]


def get_hook(call):
    repo = call.repo()
    return hook_json(call, repo, _hook(call, repo))


def _events(h, b: dict) -> list:
    events = b.get("events") or json.loads(h["events"])
    events = [e for e in events + (b.get("add_events") or []) if e not in (b.get("remove_events") or [])]
    return list(dict.fromkeys(events))


def update_hook(call):
    repo = call.repo()
    h, b = _hook(call, repo), call.body()
    cfg = {**json.loads(h["config"]), **(b.get("config") or {})}
    call.db.execute("UPDATE hooks SET active = ?, events = ?, config = ?, updated_ms = ? WHERE id = ?",
                    (int(b.get("active", bool(h["active"]))), json.dumps(_events(h, b)), json.dumps(cfg), call.now(), h["id"]))
    return hook_json(call, repo, _hook(call, repo))


def delete_hook(call):
    repo = call.repo()
    call.db.execute("DELETE FROM hooks WHERE id = ?", (_hook(call, repo)["id"],))
    return no_content()


def ping_hook(call):
    repo = call.repo()
    _ping(call, repo, _hook(call, repo))
    return no_content()


def test_hook(call):
    """Send the latest push to the default branch again, as a push event to this hook."""
    repo = call.repo()
    h, fn = _hook(call, repo), full_name(repo)
    sha = call.repos.branches(fn).get(repo["default_branch"])
    payload = {"ref": f"refs/heads/{repo['default_branch']}", "before": sha, "after": sha, "created": False,
               "deleted": False, "forced": False, "commits": [], "repository": shapes.repository(call, repo),
               "sender": user(call, call.login)}
    if "push" in json.loads(h["events"]):
        hooks_delivery.queue_one(call.ctx, h, "push", payload)
    return no_content()


def register(api) -> None:
    r = "/repos/{owner}/{repo}/hooks"
    api.route("POST", r)(create_hook)
    api.route("GET", r)(list_hooks)
    api.route("GET", r + "/{hook_id}")(get_hook)
    api.route("PATCH", r + "/{hook_id}")(update_hook)
    api.route("DELETE", r + "/{hook_id}")(delete_hook)
    api.route("POST", r + "/{hook_id}/pings")(ping_hook)
    api.route("POST", r + "/{hook_id}/tests")(test_hook)
