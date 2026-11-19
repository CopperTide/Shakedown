"""Check runs: create, update, read, and list a commit's or a suite's (`{total_count, check_runs}`).
When the last run on a commit completes, the people are told."""
from datetime import datetime

from .. import ci, notices, records, webhooks
from ..errors import invalid, not_found
from ..repos import full_name
from ..times import iso
from ..users import node_id
from .api import created

TIMES = ("started_at", "completed_at")


def _ms(value: str | None) -> int | None:
    return int(datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp() * 1000) if value else None


def run_json(call, repo, r) -> dict:
    fn = full_name(repo)
    api = call.urls.repo_api(fn, f"/check-runs/{r['id']}")
    prs = [{"id": p["id"], "number": p["number"], "url": call.urls.repo_api(fn, f"/pulls/{p['number']}"),
            "head": {"ref": p["head_ref"], "sha": p["head_sha"], "repo": _mini(call, repo)},
            "base": {"ref": p["base_ref"], "sha": p["base_sha"], "repo": _mini(call, repo)}}
           for p in records.pulls(call.db, repo["id"], "i.state = 'open' AND p.head_sha = ?", (r["head_sha"],))]
    return {"id": r["id"], "node_id": node_id("CheckRun", r["id"]), "head_sha": r["head_sha"], "name": r["name"],
            "external_id": r["external_id"], "url": api, "html_url": call.urls.repo_html(fn, f"/runs/{r['id']}"),
            "details_url": r["details_url"], "status": r["status"], "conclusion": r["conclusion"],
            "started_at": iso(r["started_ms"]) if r["started_ms"] else None,
            "completed_at": iso(r["completed_ms"]) if r["completed_ms"] else None,
            "output": {"title": r["title"], "summary": r["summary"], "text": r["text"], "annotations_count": 0,
                       "annotations_url": f"{api}/annotations"},
            "check_suite": {"id": r["suite_id"]}, "app": None, "pull_requests": prs}


def _mini(call, repo) -> dict:
    return {"id": repo["id"], "url": call.urls.repo_api(full_name(repo)), "name": repo["name"]}


def _suite(call, repo, sha: str) -> int:
    call.db.execute("INSERT OR IGNORE INTO check_suites(repo_id, head_sha, created_ms) VALUES (?, ?, ?)",
                    (repo["id"], sha, call.now()))
    return call.db.execute("SELECT id FROM check_suites WHERE repo_id = ? AND head_sha = ?", (repo["id"], sha)).fetchone()[0]


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


def _run(call, repo, run_id) -> dict:
    r = call.db.execute("SELECT * FROM check_runs WHERE id = ? AND repo_id = ?", (int(run_id), repo["id"])).fetchone()
    if r is None:
        raise not_found()
    return r


def _completed(call, repo, before, after) -> None:
    webhooks.fire(call, repo, "check_run", call.login, "completed" if after["status"] == "completed" else "created",
                  check_run=run_json(call, repo, after))
    if after["status"] != "completed" or (before is not None and before["status"] == "completed"):
        return
    conclusion = ci.runs_conclusion(ci.runs(call.db, repo["id"], after["head_sha"]))
    if conclusion is not None:
        prs = [p["number"] for p in records.pulls(call.db, repo["id"], "i.state = 'open' AND p.head_sha = ?", (after["head_sha"],))]
        notices.checks_completed(call.ctx, full_name(repo), after["head_sha"], conclusion, sorted(prs))


def create_run(call):
    repo, b = call.repo(), call.body()
    sha = call.repos.ref(full_name(repo), b.get("head_sha", ""))
    if not b.get("name") or sha is None:
        raise invalid("Validation Failed", [{"resource": "CheckRun", "code": "invalid", "field": "head_sha"}])
    f = {"status": "queued", **_fields(call, b)}
    cols = ", ".join(f)
    cur = call.db.execute(f"INSERT INTO check_runs(repo_id, suite_id, head_sha, {cols}) VALUES (?, ?, ?{', ?' * len(f)})",
                          (repo["id"], _suite(call, repo, sha), sha, *f.values()))
    r = _run(call, repo, cur.lastrowid)
    _completed(call, repo, None, r)
    return created(run_json(call, repo, r))


def update_run(call):
    repo = call.repo()
    before = _run(call, repo, call.req.params["check_run_id"])
    f = _fields(call, call.body())
    if f:
        call.db.execute(f"UPDATE check_runs SET {', '.join(f'{k} = ?' for k in f)} WHERE id = ?", (*f.values(), before["id"]))
    after = _run(call, repo, before["id"])
    _completed(call, repo, before, after)
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
