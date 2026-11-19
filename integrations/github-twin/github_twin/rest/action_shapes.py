"""The JSON shapes of Actions: workflows, workflow runs (an attempt of one) and jobs; every URL from
the request's public base, `jobs_url` and `logs_url` too (gh follows them as given)."""
import json
import os

from .. import records, users
from ..actions import jobs
from ..repos import full_name
from ..times import iso
from ..users import node_id
from . import shapes


def _iso(ms):
    return iso(ms) if ms else None


def workflow_json(v, repo, w) -> dict:
    fn, file = full_name(repo), os.path.basename(w["path"])
    return {"id": w["id"], "node_id": node_id("Workflow", w["id"]), "name": w["name"], "path": w["path"],
            "state": w["state"], "created_at": iso(w["created_ms"]), "updated_at": iso(w["updated_ms"]),
            "url": v.urls.repo_api(fn, f"/actions/workflows/{w['id']}"),
            "html_url": v.urls.repo_html(fn, f"/blob/{repo['default_branch']}/{w['path']}"),
            "badge_url": v.urls.repo_html(fn, f"/workflows/{file}/badge.svg")}


def _head_commit(v, repo, sha: str) -> dict:
    info = v.repos.commit_info(full_name(repo), sha)
    return {"id": sha, "tree_id": info.tree, "message": info.message, "timestamp": iso(info.author.at_ms),
            "author": {"name": info.author.name, "email": info.author.email},
            "committer": {"name": info.committer.name, "email": info.committer.email}}


def attempt_state(db, run, attempt: int) -> tuple:
    """(status, conclusion) of attempt `attempt` of a run (the run's own for its current one)."""
    if attempt == run["run_attempt"]:
        return run["status"], run["conclusion"]
    rows = jobs.jobs_of(db, run["id"], attempt)
    if any(j["status"] != "completed" for j in rows):
        return "in_progress", None
    return "completed", jobs.conclusion_of(rows)


def _urls(v, repo, run, attempt: int) -> dict:
    fn, rid = full_name(repo), run["id"]
    api = v.urls.repo_api(fn, f"/actions/runs/{rid}")
    return {"url": api, "html_url": v.urls.repo_html(fn, f"/actions/runs/{rid}"), "jobs_url": f"{api}/jobs",
            "logs_url": f"{api}/logs", "check_suite_url": v.urls.repo_api(fn, f"/check-suites/{run['suite_id']}"),
            "artifacts_url": f"{api}/artifacts", "cancel_url": f"{api}/cancel", "rerun_url": f"{api}/rerun",
            "previous_attempt_url": f"{api}/attempts/{attempt - 1}" if attempt > 1 else None,
            "workflow_url": v.urls.repo_api(fn, f"/actions/workflows/{run['workflow_id']}")}


def run_json(v, repo, run, attempt: int | None = None, with_prs: bool = True) -> dict:
    attempt = attempt or run["run_attempt"]
    status, conclusion = attempt_state(v.db, run, attempt)
    prs = [shapes.pr_minimal(v, repo, p) for n in json.loads(run["pr_numbers"]) if with_prs
           for p in [records.pull(v.db, repo["id"], n)] if p is not None]
    who = {k: users.simple_user(v.urls, users.ensure(v.db, run[k], v.now())) for k in ("actor", "triggering_actor")}
    full = shapes.repository(v, repo)
    return {"id": run["id"], "name": run["name"], "node_id": node_id("WorkflowRun", run["id"]),
            "check_suite_id": run["suite_id"], "check_suite_node_id": node_id("CheckSuite", run["suite_id"]),
            "head_branch": run["head_branch"], "head_sha": run["head_sha"], "path": run["path"],
            "run_number": run["run_number"], "run_attempt": attempt, "referenced_workflows": [], "event": run["event"],
            "status": status, "conclusion": conclusion, "workflow_id": run["workflow_id"], "pull_requests": prs,
            "created_at": iso(run["created_ms"]), "updated_at": iso(run["updated_ms"]),
            "run_started_at": _iso(run["run_started_ms"]) or iso(run["created_ms"]), **who,
            **_urls(v, repo, run, attempt), "head_commit": _head_commit(v, repo, run["head_sha"]),
            "repository": full, "head_repository": full, "head_repository_id": repo["id"],
            "display_title": run["display_title"]}


def _step_json(s) -> dict:
    return {"name": s["name"], "status": s["status"], "conclusion": s["conclusion"], "number": s["number"],
            "started_at": _iso(s["started_ms"]), "completed_at": _iso(s["completed_ms"])}


def job_json(v, repo, run, job) -> dict:
    fn, assigned = full_name(repo), job["status"] != "queued"
    return {"id": job["id"], "run_id": run["id"], "run_url": v.urls.repo_api(fn, f"/actions/runs/{run['id']}"),
            "run_attempt": job["attempt"], "node_id": node_id("CheckRun", job["id"]), "head_sha": run["head_sha"],
            "url": v.urls.repo_api(fn, f"/actions/jobs/{job['id']}"),
            "html_url": v.urls.repo_html(fn, f"/actions/runs/{run['id']}/job/{job['id']}"),
            "status": job["status"], "conclusion": job["conclusion"], "created_at": iso(job["created_ms"]),
            "started_at": _iso(job["started_ms"]) or iso(job["created_ms"]), "completed_at": _iso(job["completed_ms"]),
            "name": job["name"], "steps": [_step_json(s) for s in jobs.steps_of(v.db, job["id"])],
            "check_run_url": v.urls.repo_api(fn, f"/check-runs/{job['id']}"), "labels": json.loads(job["labels"]),
            "runner_id": 1 if assigned else None, "runner_name": "shakedown" if assigned else None,
            "runner_group_id": 1 if assigned else None, "runner_group_name": "Default" if assigned else None,
            "workflow_name": run["name"], "head_branch": run["head_branch"]}
