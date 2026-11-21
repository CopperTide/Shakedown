"""Check runs: create, update, read, and list a commit's or a suite's (`{total_count, check_runs}`).
When the last run on a commit completes, the people are told."""
from datetime import datetime

from .. import ci, notices, records, webhooks
from ..errors import invalid, not_found
from ..repos import full_name
from ..times import iso
from ..users import node_id
from . import shapes
from .api import created

TIMES = ("started_at", "completed_at")


def _ms(value: str | None) -> int | None:
    return int(datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp() * 1000) if value else None


def run_json(call, repo, r) -> dict:
    fn = full_name(repo)
    api = call.urls.repo_api(fn, f"/check-runs/{r['id']}")
    prs = [shapes.pr_minimal(call, repo, p)
           for p in records.pulls(call.db, repo["id"], "i.state = 'open' AND p.head_sha = ?", (r["head_sha"],))]
    return {"id": r["id"], "node_id": node_id("CheckRun", r["id"]), "head_sha": r["head_sha"], "name": r["name"],
            "external_id": r["external_id"], "url": api, "html_url": call.urls.repo_html(fn, f"/runs/{r['id']}"),
            "details_url": r["details_url"], "status": r["status"], "conclusion": r["conclusion"],
            "started_at": iso(r["started_ms"]) if r["started_ms"] else None,
            "completed_at": iso(r["completed_ms"]) if r["completed_ms"] else None,
            "output": {"title": r["title"], "summary": r["summary"], "text": r["text"], "annotations_count": 0,
                       "annotations_url": f"{api}/annotations"},
            "check_suite": {"id": r["suite_id"]}, "app": None, "pull_requests": prs}


def _suite(call, repo, sha: str) -> int:
    """The commit's suite for check runs made through the API (Actions runs have one each)."""
    row = call.db.execute("SELECT id FROM check_suites WHERE repo_id = ? AND head_sha = ? AND kind = 'api'",
                          (repo["id"], sha)).fetchone()
    if row is not None:
        return row[0]
    return call.db.execute("INSERT INTO check_suites(repo_id, head_sha, created_ms) VALUES (?, ?, ?)",
                           (repo["id"], sha, call.now())).lastrowid


def _fields(call, b: dict) -> dict:
    out = {k: b[k] for k in ("name", "status", "conclusion", "external_id", "details_url") if k in b}
    out.update({k[:-3] + "_ms": _ms(b[k]) for k in TIMES if k in b})
    out.update({k: (b.get("output") or {}).get(k) for k in ("title", "summary", "text") if k in (b.get("output") or {})})
    if out.get("conclusion"):
        out["status"] = "completed"
        out.setdefault("completed_ms", call.now())
    if out.get("status") == "in_progress":
        out.setdefault("started_ms", call.now())
    return out


def insert(v, repo, suite_id: int, sha: str, fields: dict):
    """A new check run on `sha` in suite `suite_id`; its row."""
    cols = ", ".join(fields)
    cur = v.db.execute(f"INSERT INTO check_runs(repo_id, suite_id, head_sha, {cols}) VALUES (?, ?, ?{', ?' * len(fields)})",
                       (repo["id"], suite_id, sha, *fields.values()))
    return _run(v, repo, cur.lastrowid)


def update(v, repo, run_id: int, fields: dict):
    """Set `fields` of a check run; its row before and after."""
    before = _run(v, repo, run_id)
    if fields:
        v.db.execute(f"UPDATE check_runs SET {', '.join(f'{k} = ?' for k in fields)} WHERE id = ?", (*fields.values(), run_id))
    return before, _run(v, repo, run_id)


def _run(call, repo, run_id) -> dict:
    r = call.db.execute("SELECT * FROM check_runs WHERE id = ? AND repo_id = ?", (int(run_id), repo["id"])).fetchone()
    if r is None:
        raise not_found()
    return r


def changed(v, repo, before, after, sender: str) -> None:
    """What follows a check run made or changed by `sender`: its webhook, and when it was the last on
    its commit to complete, the people told."""
    webhooks.fire(v, repo, "check_run", sender, "completed" if after["status"] == "completed" else "created",
                  check_run=run_json(v, repo, after))
    if after["status"] != "completed" or (before is not None and before["status"] == "completed"):
        return
    conclusion = ci.runs_conclusion(ci.runs(v.db, repo["id"], after["head_sha"]))
    if conclusion is not None:
        prs = [p["number"] for p in records.pulls(v.db, repo["id"], "i.state = 'open' AND p.head_sha = ?", (after["head_sha"],))]
        notices.checks_completed(v.ctx, full_name(repo), after["head_sha"], conclusion, sorted(prs))


def create_run(call):
    repo, b = call.repo(), call.body()
    sha = call.repos.ref(full_name(repo), b.get("head_sha", ""))
    if not b.get("name") or sha is None:
        raise invalid("Validation Failed", [{"resource": "CheckRun", "code": "invalid", "field": "head_sha"}])
    r = insert(call, repo, _suite(call, repo, sha), sha, {"status": "queued", **_fields(call, b)})
    changed(call, repo, None, r, call.login)
    return created(run_json(call, repo, r))


def update_run(call):
    repo = call.repo()
    before, after = update(call, repo, _run(call, repo, call.req.params["check_run_id"])["id"], _fields(call, call.body()))
    changed(call, repo, before, after, call.login)
    return run_json(call, repo, after)


def get_run(call):
    repo = call.repo()
    return run_json(call, repo, _run(call, repo, call.req.params["check_run_id"]))


def _listing(call, repo, rows) -> dict:
    name, status = call.req.arg("check_name"), call.req.arg("status")
    rows = [r for r in rows if (not name or r["name"] == name) and (not status or r["status"] == status)]
    return {"total_count": len(rows), "check_runs": [run_json(call, repo, r) for r in rows]}


def for_ref(call):
    repo = call.repo()
    sha = call.repos.ref(full_name(repo), call.req.params["ref"])
    if sha is None:
        raise invalid(f"No commit found for SHA: {call.req.params['ref']}")
    return _listing(call, repo, ci.runs(call.db, repo["id"], sha))


def for_suite(call):
    repo = call.repo()
    rows = call.db.execute("SELECT * FROM check_runs WHERE repo_id = ? AND suite_id = ? ORDER BY id",
                           (repo["id"], int(call.req.params["check_suite_id"]))).fetchall()
    return _listing(call, repo, rows)


def register(api) -> None:
    r = "/repos/{owner}/{repo}"
    api.route("POST", r + "/check-runs")(create_run)
    api.route("PATCH", r + "/check-runs/{check_run_id}")(update_run)
    api.route("GET", r + "/check-runs/{check_run_id}")(get_run)
    api.route("GET", r + "/commits/{ref}/check-runs")(for_ref)
    api.route("GET", r + "/check-suites/{check_suite_id}/check-runs")(for_suite)
