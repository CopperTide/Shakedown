"""What a job shows on its commit, as GitHub Actions: a check run named `<workflow> / <job>` (the
job's id is its check run's) and a commit status `<workflow> / <job> (<event>)`."""
from .. import users
from ..repos import full_name
from ..rest import checks, statuses

BOT = "github-actions"
STATES = {"success": "success", "skipped": "success", "cancelled": "error"}
WORDS = {"success": "Successful", "skipped": "Skipped", "cancelled": "Cancelled", "failure": "Failing",
         "timed_out": "Timed out"}


def check_name(run, job_name: str) -> str:
    return f"{run['name']} / {job_name}"


def context(run, job_name: str) -> str:
    return f"{check_name(run, job_name)} ({run['event']})"


def job_url(v, repo, run_id: int, job_id: int) -> str:
    return v.urls.repo_html(full_name(repo), f"/actions/runs/{run_id}/job/{job_id}")


def queued(v, repo, run, job_name: str) -> int:
    """A queued check run and a pending status for a new job of `run`; the job's id."""
    users.ensure(v.db, BOT, v.now(), kind="Bot")
    first = checks.insert(v, repo, run["suite_id"], run["head_sha"], {"name": check_name(run, job_name), "status": "queued"})
    url = job_url(v, repo, run["id"], first["id"])
    _, row = checks.update(v, repo, first["id"], {"details_url": url})
    checks.changed(v, repo, None, row, BOT)
    statuses.post(v, repo, run["head_sha"], "pending", context(run, job_name), "Queued", url, BOT)
    return row["id"]


def started(v, repo, job_id: int) -> None:
    before, after = checks.update(v, repo, job_id, {"status": "in_progress", "started_ms": v.now()})
    checks.changed(v, repo, before, after, BOT)


def completed(v, repo, run, job, conclusion: str, summary: str, quiet: bool = False) -> None:
    """The job's check run completed with `conclusion`, and its status set to match (`quiet`: the
    check run only)."""
    before, after = checks.update(v, repo, job["id"], {
        "status": "completed", "conclusion": conclusion, "completed_ms": v.now(),
        "title": WORDS.get(conclusion, conclusion), "summary": summary})
    if quiet:
        return
    checks.changed(v, repo, before, after, BOT)
    statuses.post(v, repo, run["head_sha"], STATES.get(conclusion, "failure"), context(run, job["name"]),
                  WORDS.get(conclusion, conclusion), job_url(v, repo, run["id"], job["id"]), BOT)
