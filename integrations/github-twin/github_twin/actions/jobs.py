"""The jobs of runs as the worker moves them, each change under the twin's lock in one transaction:
taking the next job that may run (oldest first, its needs met), its steps' results and logs, its end,
the run's end, and a deploy step's wait for the world's answer."""
import json
from dataclasses import dataclass, replace

from ..repos import full_name
from . import publish
from .workflow import Job, Workflow

FAILED = {"failure", "cancelled", "timed_out"}


def run_row(db, run_id: int):
    return db.execute("SELECT * FROM workflow_runs WHERE id = ?", (run_id,)).fetchone()


def job_row(db, job_id: int):
    return db.execute("SELECT * FROM jobs WHERE id = ?", (job_id,)).fetchone()


def jobs_of(db, run_id: int, attempt: int | None = None) -> list:
    if attempt is None:
        return db.execute("SELECT * FROM jobs WHERE run_id = ? ORDER BY id", (run_id,)).fetchall()
    return db.execute("SELECT * FROM jobs WHERE run_id = ? AND attempt = ? ORDER BY id", (run_id, attempt)).fetchall()


def steps_of(db, job_id: int) -> list:
    return db.execute("SELECT * FROM job_steps WHERE job_id = ? ORDER BY number", (job_id,)).fetchall()


def _repo(db, run):
    return db.execute("SELECT * FROM repos WHERE id = ?", (run["repo_id"],)).fetchone()


@dataclass(frozen=True)
class Ticket:
    """What running a job needs outside the lock."""
    job_id: int
    run_id: int
    attempt: int
    run_number: int
    repo: str
    sha: str
    ref: str
    event: str
    actor: str
    env: dict
    job: Job


def github_ref(run) -> str:
    if run["event"] == "pull_request":
        return f"refs/pull/{json.loads(run['pr_numbers'])[0]}/merge"
    return f"refs/heads/{run['head_branch']}"


def conclusion_of(rows: list, cancelled: bool = False) -> str:
    found = {r["conclusion"] for r in rows}
    if cancelled or "cancelled" in found:
        return "cancelled"
    return "failure" if found & FAILED else "success"


def _blocked(db, run, spec_job: Job) -> str | None:
    """'wait' while a needed job runs, 'skip' when one did not succeed or the `if` fails; else None."""
    needed = [j for j in jobs_of(db, run["id"], run["run_attempt"]) if j["job_key"] in spec_job.needs]
    if any(j["status"] != "completed" for j in needed):
        return "wait"
    if any(j["conclusion"] != "success" for j in needed):
        return "skip"
    ok = spec_job.runs_if({"github.ref": github_ref(run), "github.event_name": run["event"]})
    return None if ok else "skip"


def _queued(db) -> list:
    return db.execute("SELECT j.* FROM jobs j JOIN workflow_runs r ON r.id = j.run_id AND r.run_attempt = j.attempt "
                      "WHERE j.status = 'queued' AND r.status != 'completed' ORDER BY j.run_id, j.id").fetchall()


def claim(v) -> Ticket | None:
    """The next job to run, started; jobs that cannot run any more are skipped on the way."""
    for job in _queued(v.db):
        run = run_row(v.db, job["run_id"])
        w = Workflow.from_json(run["spec"])
        spec_job = next(j for j in w.jobs if j.key == job["job_key"])
        why = _blocked(v.db, run, spec_job)
        if why == "skip":
            finish(v, _repo(v.db, run), job["id"], "skipped")
        if why is None:
            return _start(v, run, job, w, replace(spec_job, image=job["image"]))
    return None


def _start(v, run, job, w: Workflow, spec_job: Job) -> Ticket:
    repo, now = _repo(v.db, run), v.now()
    v.db.execute("UPDATE jobs SET status = 'in_progress', started_ms = ? WHERE id = ?", (now, job["id"]))
    v.db.execute("UPDATE workflow_runs SET status = 'in_progress', run_started_ms = COALESCE(run_started_ms, ?), "
                 "updated_ms = ? WHERE id = ?", (now, now, run["id"]))
    publish.started(v, repo, job["id"])
    return Ticket(job["id"], run["id"], run["run_attempt"], run["run_number"], full_name(repo), run["head_sha"],
                  github_ref(run), run["event"], run["triggering_actor"], dict(w.env), spec_job)


def step_started(v, job_id: int, number: int) -> None:
    v.db.execute("UPDATE job_steps SET status = 'in_progress', started_ms = ? WHERE job_id = ? AND number = ?",
                 (v.now(), job_id, number))


def step_finished(v, job_id: int, number: int, conclusion: str, log: str) -> None:
    v.db.execute("UPDATE job_steps SET status = 'completed', conclusion = ?, started_ms = COALESCE(started_ms, ?), "
                 "completed_ms = ?, log = log || ? WHERE job_id = ? AND number = ?",
                 (conclusion, v.now(), v.now(), log, job_id, number))


def note(v, job_id: int, text: str) -> None:
    """Add `text` to the log of the job's step that was running (else its first step)."""
    steps = steps_of(v.db, job_id)
    running = [s for s in steps if s["status"] == "in_progress"] or steps[:1]
    if running:
        v.db.execute("UPDATE job_steps SET log = log || ? WHERE job_id = ? AND number = ?",
                     (text, job_id, running[0]["number"]))


def finish(v, repo, job_id: int, conclusion: str, quiet: bool = False) -> None:
    """The job completed with `conclusion`; steps not finished are closed (cancelled or skipped).
    `quiet`: an attempt a restore replaces, whose check run just ends (no status, notice or run end)."""
    job, now = job_row(v.db, job_id), v.now()
    left = "cancelled" if conclusion == "cancelled" else "skipped"
    v.db.execute("UPDATE job_steps SET status = 'completed', conclusion = CASE WHEN status = 'in_progress' THEN ? "
                 "ELSE 'skipped' END, completed_ms = ? WHERE job_id = ? AND status != 'completed'", (left, now, job_id))
    v.db.execute("UPDATE jobs SET status = 'completed', conclusion = ?, completed_ms = ? WHERE id = ?",
                 (conclusion, now, job_id))
    run = run_row(v.db, job["run_id"])
    publish.completed(v, repo, run, job, conclusion, _summary(v.db, job_id), quiet)
    if not quiet:
        settle_run(v, repo, run["id"])


def _summary(db, job_id: int) -> str:
    return "\n".join(f"{s['number']}. {s['name']}: {s['conclusion']}" for s in steps_of(db, job_id))


def settle_run(v, repo, run_id: int) -> None:
    """Complete the run when every job of its attempt is."""
    run = run_row(v.db, run_id)
    rows = jobs_of(v.db, run_id, run["run_attempt"])
    if run["status"] == "completed" or any(j["status"] != "completed" for j in rows):
        return
    v.db.execute("UPDATE workflow_runs SET status = 'completed', conclusion = ?, updated_ms = ? WHERE id = ?",
                 (conclusion_of(rows, bool(run["cancel_requested"])), v.now(), run_id))


def job_done(v, job_id: int, conclusion: str) -> None:
    run = run_row(v.db, job_row(v.db, job_id)["run_id"])
    finish(v, _repo(v.db, run), job_id, conclusion)


def is_cancelled(v, run_id: int) -> bool:
    return bool(run_row(v.db, run_id)["cancel_requested"])


# --- a deploy step's wait -----------------------------------------------------------------
def deploy_waiting(v, t: Ticket, service: str, image: str) -> None:
    """Hand the deploy to the world (event `deploy`) and wait for `deploy_result`."""
    v.db.execute("UPDATE jobs SET deploy_state = 'waiting', deploy_started_ms = ?, deploy_ok = NULL, deploy_error = NULL "
                 "WHERE id = ?", (v.now(), t.job_id))
    v.ctx.event("deploy", f"{t.actor}'s run {t.run_id} deploys {image} to {service}",
                {"service": service, "image": image, "sha": t.sha, "repo": t.repo, "run_id": t.run_id,
                 "job_id": str(t.job_id), "by": t.actor})


def deploy_poll(v, job_id: int, timeout_ms: int) -> tuple:
    """('done', ok, error), ('timed_out',), ('cancelled',) or ('waiting',); ends the wait when timed out
    (world time) or cancelled."""
    job = job_row(v.db, job_id)
    if job["deploy_state"] == "done":
        return "done", bool(job["deploy_ok"]), job["deploy_error"]
    for state, hit in (("cancelled", is_cancelled(v, job["run_id"])),
                       ("timed_out", v.now() - job["deploy_started_ms"] >= timeout_ms)):
        if hit:
            v.db.execute("UPDATE jobs SET deploy_state = ? WHERE id = ?", (state, job_id))
            return (state,)
    return ("waiting",)


def deploy_result(v, job_id: int, ok: bool, error: str | None) -> str | None:
    """The world's answer to a job's deploy; why it is refused, else None."""
    job = job_row(v.db, job_id)
    if job is None or job["deploy_state"] != "waiting":
        return f"job {job_id} is not waiting for a deploy"
    v.db.execute("UPDATE jobs SET deploy_state = 'done', deploy_ok = ?, deploy_error = ? WHERE id = ?",
                 (int(bool(ok)), error, job_id))
    return None
