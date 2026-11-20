"""Actions over REST, what `gh run list/view/watch` and the Actions API read: workflows, runs and
their attempts, jobs, logs (a redirect to the signed `/_logs/...`), re-run and cancel, and the
annotations and artifacts runs here never have."""
import os

from shakedown_part.twin import Response

from ..actions import jobs, logs, runs
from ..errors import GithubError, not_found
from ..pagination import page
from . import action_shapes as shapes
from .api import ApiError, created

STATUSES = {"queued", "in_progress", "completed", "waiting", "requested", "pending"}
R = "/repos/{owner}/{repo}"


def _listing(call, key: str, items: list, shape) -> Response:
    rows, link = page(items, call.req, call.urls)
    return Response(200, {"total_count": len(items), key: [shape(r) for r in rows]},
                    headers={"Link": link} if link else None)


def _id(call, key: str) -> int:
    value = call.req.params[key]
    if not value.isdigit():
        raise not_found()
    return int(value)


def _workflow(call, repo):
    wid = call.req.params["workflow_id"]
    for w in call.db.execute("SELECT * FROM workflows WHERE repo_id = ? ORDER BY id", (repo["id"],)).fetchall():
        if wid == str(w["id"]) or wid == os.path.basename(w["path"]):
            return w
    raise not_found()


def _run(call, repo):
    run = jobs.run_row(call.db, _id(call, "run_id"))
    if run is None or run["repo_id"] != repo["id"]:
        raise not_found()
    return run


def _attempt(call, run) -> int:
    n = _id(call, "attempt_number") if "attempt_number" in call.req.params else run["run_attempt"]
    if not 1 <= n <= run["run_attempt"]:
        raise not_found()
    return n


def list_workflows(call):
    repo = call.repo()
    rows = call.db.execute("SELECT * FROM workflows WHERE repo_id = ? ORDER BY id", (repo["id"],)).fetchall()
    return _listing(call, "workflows", rows, lambda w: shapes.workflow_json(call, repo, w))


def get_workflow(call):
    repo = call.repo()
    return shapes.workflow_json(call, repo, _workflow(call, repo))


def _matches(call, run) -> bool:
    a = call.req.arg
    status = a("status")
    checks = [("branch", run["head_branch"]), ("actor", run["actor"]), ("event", run["event"]),
              ("head_sha", run["head_sha"]), ("check_suite_id", str(run["suite_id"]))]
    if any(a(k) and a(k) != value for k, value in checks):
        return False
    return not status or (run["status"] if status in STATUSES else run["conclusion"]) == status


def _runs(call, repo, where: str = "1", args: tuple = ()) -> Response:
    rows = call.db.execute(f"SELECT * FROM workflow_runs WHERE repo_id = ? AND {where} ORDER BY id DESC",
                           (repo["id"], *args)).fetchall()
    with_prs = call.req.arg("exclude_pull_requests") != "true"
    return _listing(call, "workflow_runs", [r for r in rows if _matches(call, r)],
                    lambda r: shapes.run_json(call, repo, r, with_prs=with_prs))


def list_runs(call):
    return _runs(call, call.repo())


def workflow_runs(call):
    repo = call.repo()
    return _runs(call, repo, "workflow_id = ?", (_workflow(call, repo)["id"],))


def get_run(call):
    repo = call.repo()
    run = _run(call, repo)
    return shapes.run_json(call, repo, run, _attempt(call, run), call.req.arg("exclude_pull_requests") != "true")


def run_jobs(call):
    repo = call.repo()
    run = _run(call, repo)
    every = call.req.arg("filter") == "all" and "attempt_number" not in call.req.params
    rows = jobs.jobs_of(call.db, run["id"], None if every else _attempt(call, run))
    return _listing(call, "jobs", rows, lambda j: shapes.job_json(call, repo, run, j))


def _job(call, repo):
    job = jobs.job_row(call.db, _id(call, "job_id"))
    run = jobs.run_row(call.db, job["run_id"]) if job is not None else None
    if run is None or run["repo_id"] != repo["id"]:
        raise not_found()
    return run, job


def get_job(call):
    repo = call.repo()
    return shapes.job_json(call, repo, *_job(call, repo))


def job_logs(call):
    _, job = _job(call, call.repo())
    return Response(302, None, headers={"Location": logs.url(call, f"jobs/{job['id']}")})


def run_logs(call):
    run = _run(call, call.repo())
    return Response(302, None, headers={"Location": logs.url(call, f"runs/{run['id']}")})


def rerun(call):
    repo = call.repo()
    run = _run(call, repo)
    if run["status"] != "completed":
        raise ApiError(403, "This workflow is already running")
    if not runs.rerun(call, repo, run, call.login):
        raise ApiError(403, "This workflow run cannot be rerun")
    return created({})


def cancel(call):
    repo = call.repo()
    if not runs.cancel(call, repo, _run(call, repo)):
        raise GithubError(409, "Cannot cancel a workflow run that is completed.")
    return Response(202, {})


def annotations(call):
    repo = call.repo()
    if call.db.execute("SELECT 1 FROM check_runs WHERE id = ? AND repo_id = ?",
                       (_id(call, "check_run_id"), repo["id"])).fetchone() is None:
        raise not_found()
    return []


def artifacts(call):
    _run(call, call.repo())
    return {"total_count": 0, "artifacts": []}


def register(api) -> None:
    a = R + "/actions"
    for method, path, fn in (
            ("GET", "/workflows", list_workflows), ("GET", "/workflows/{workflow_id}", get_workflow),
            ("GET", "/workflows/{workflow_id}/runs", workflow_runs), ("GET", "/runs", list_runs),
            ("GET", "/runs/{run_id}", get_run), ("GET", "/runs/{run_id}/attempts/{attempt_number}", get_run),
            ("GET", "/runs/{run_id}/jobs", run_jobs), ("GET", "/runs/{run_id}/attempts/{attempt_number}/jobs", run_jobs),
            ("GET", "/runs/{run_id}/logs", run_logs), ("GET", "/runs/{run_id}/artifacts", artifacts),
            ("POST", "/runs/{run_id}/rerun", rerun), ("POST", "/runs/{run_id}/cancel", cancel),
            ("GET", "/jobs/{job_id}", get_job), ("GET", "/jobs/{job_id}/logs", job_logs)):
        api.route(method, a + path)(fn)
    api.route("GET", R + "/check-runs/{check_run_id}/annotations")(annotations)
