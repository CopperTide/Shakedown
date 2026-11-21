"""Workflow runs: started by a push or a pull request from the workflows in its commit, one check
suite each and a job (check run and status) per workflow job; new attempts by re-run or by a
restore that found the run in progress (F14), and cancels."""
import json
from dataclasses import dataclass

from ..repos import full_name
from . import jobs, publish
from .workflow import Workflow, WorkflowError, parse

WORKFLOWS = ".github/workflows/"


def settings(config: dict) -> dict:
    return config.get("actions") or {}


def enabled(config: dict) -> bool:
    return bool(settings(config).get("enabled"))


@dataclass(frozen=True)
class Trigger:
    event: str
    head_branch: str
    branch: str          # what `on.<event>.branches` filters: the pushed branch, a pull request's base
    sha: str
    actor: str
    title: str
    prs: tuple = ()


def workflow_files(v, repo, sha: str) -> list:
    """`(path, text)` of each workflow file in commit `sha`."""
    fn = full_name(repo)
    return [(e.path, v.repos.blob(fn, e.sha).decode(errors="replace")) for e in v.repos.tree(fn, sha, WORKFLOWS)
            if e.type == "blob" and e.path.endswith((".yml", ".yaml"))]


def ensure_workflow(v, repo, path: str, name: str):
    db = v.db
    row = db.execute("SELECT * FROM workflows WHERE repo_id = ? AND path = ?", (repo["id"], path)).fetchone()
    if row is None:
        db.execute("INSERT INTO workflows(repo_id, path, name, created_ms, updated_ms) VALUES (?, ?, ?, ?, ?)",
                   (repo["id"], path, name, v.now(), v.now()))
    elif row["name"] != name:
        db.execute("UPDATE workflows SET name = ?, updated_ms = ? WHERE id = ?", (name, v.now(), row["id"]))
    return db.execute("SELECT * FROM workflows WHERE repo_id = ? AND path = ?", (repo["id"], path)).fetchone()


def register(v, repo, sha: str) -> None:
    """Know the workflows of commit `sha` (the seeded default branch), without running them."""
    for path, text in workflow_files(v, repo, sha):
        w = parse(text)
        ensure_workflow(v, repo, path, (w.name if isinstance(w, Workflow) else None) or path)


def on_push(v, repo, branch: str, sha: str, by: str) -> None:
    if enabled(v.config):
        title = v.repos.commit_info(full_name(repo), sha).message.split("\n", 1)[0]
        start_all(v, repo, Trigger("push", branch, branch, sha, by, title))


def on_pull_request(v, repo, pr, by: str) -> None:
    if enabled(v.config):
        start_all(v, repo, Trigger("pull_request", pr["head_ref"], pr["base_ref"], pr["head_sha"], by, pr["title"],
                                   (pr["number"],)))


def _no_runner(v, w: Workflow) -> WorkflowError | None:
    runners = settings(v.config).get("runners") or {}
    for job in w.jobs:
        if not (job.image or runners.get(job.runs_on)):
            return WorkflowError(f"jobs.{job.key}.runs-on: no runner for {job.runs_on} in this world")
    return None


def start_all(v, repo, t: Trigger) -> None:
    for path, text in workflow_files(v, repo, t.sha):
        w = parse(text)
        if isinstance(w, Workflow) and not w.triggered(t.event, t.branch):
            continue
        error = w if isinstance(w, WorkflowError) else _no_runner(v, w)
        name = (w.name if isinstance(w, Workflow) else None) or path
        run = _insert(v, repo, ensure_workflow(v, repo, path, name), t, None if error else w, error)
        if error is None:
            attempt(v, repo, run, w)


def _insert(v, repo, wf, t: Trigger, w: Workflow | None, error: WorkflowError | None):
    db, now = v.db, v.now()
    suite = db.execute("INSERT INTO check_suites(repo_id, head_sha, kind, created_ms) VALUES (?, ?, 'actions', ?)",
                       (repo["id"], t.sha, now)).lastrowid
    number = db.execute("SELECT COALESCE(MAX(run_number), 0) + 1 FROM workflow_runs WHERE workflow_id = ?",
                        (wf["id"],)).fetchone()[0]
    cur = db.execute(
        "INSERT INTO workflow_runs(repo_id, workflow_id, suite_id, name, path, run_number, event, head_branch, head_sha, "
        "actor, triggering_actor, display_title, status, conclusion, spec, error, pr_numbers, created_ms, updated_ms) "
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        (repo["id"], wf["id"], suite, wf["name"], wf["path"], number, t.event, t.head_branch, t.sha, t.actor, t.actor,
         t.title, "completed" if error else "queued", "startup_failure" if error else None,
         w.to_json() if w else None, str(error) if error else None, json.dumps(list(t.prs)), now, now))
    return jobs.run_row(db, cur.lastrowid)


def attempt(v, repo, run, w: Workflow) -> None:
    """The jobs of the run's current attempt, queued: a check run, a pending status and the steps each."""
    runners = settings(v.config).get("runners") or {}
    for job in w.jobs:
        job_id = publish.queued(v, repo, run, job.name)
        v.db.execute("INSERT INTO jobs(id, run_id, attempt, job_key, name, image, labels, status, created_ms) "
                     "VALUES (?, ?, ?, ?, ?, ?, ?, 'queued', ?)",
                     (job_id, run["id"], run["run_attempt"], job.key, job.name, job.image or runners[job.runs_on],
                      json.dumps([job.runs_on] if job.runs_on else []), v.now()))
        v.db.executemany("INSERT INTO job_steps(job_id, number, name, status) VALUES (?, ?, ?, 'queued')",
                         [(job_id, n, s.name) for n, s in enumerate(job.steps, 1)])


def _next_attempt(v, repo, run, by: str):
    v.db.execute("UPDATE workflow_runs SET run_attempt = run_attempt + 1, status = 'queued', conclusion = NULL, "
                 "cancel_requested = 0, triggering_actor = ?, run_started_ms = NULL, updated_ms = ? WHERE id = ?",
                 (by, v.now(), run["id"]))
    run = jobs.run_row(v.db, run["id"])
    attempt(v, repo, run, Workflow.from_json(run["spec"]))
    return run


def rerun(v, repo, run, by: str) -> bool:
    """Run a completed run again as its next attempt; False when it cannot be."""
    if run["status"] != "completed" or run["spec"] is None:
        return False
    _next_attempt(v, repo, run, by)
    return True


def cancel(v, repo, run) -> bool:
    """Cancel a run that is not completed: its queued jobs at once, its running job by the worker."""
    if run["status"] == "completed":
        return False
    v.db.execute("UPDATE workflow_runs SET cancel_requested = 1, updated_ms = ? WHERE id = ?", (v.now(), run["id"]))
    for job in jobs.jobs_of(v.db, run["id"], run["run_attempt"]):
        if job["status"] == "queued":
            jobs.finish(v, repo, job["id"], "cancelled")
    jobs.settle_run(v, repo, run["id"])
    return True


RESTORED = "The world was restored from a save made while this job ran; the run runs again as attempt {n}.\n"


def restart_interrupted(v) -> None:
    """F14: a run in progress when the save was made runs again, as its next attempt; the attempt
    the save caught is cancelled quietly (the new attempt's checks and statuses stand for it)."""
    for run in v.db.execute("SELECT * FROM workflow_runs WHERE status = 'in_progress' ORDER BY id").fetchall():
        repo = v.db.execute("SELECT * FROM repos WHERE id = ?", (run["repo_id"],)).fetchone()
        for job in jobs.jobs_of(v.db, run["id"], run["run_attempt"]):
            if job["status"] != "completed":
                jobs.note(v, job["id"], RESTORED.format(n=run["run_attempt"] + 1))
                jobs.finish(v, repo, job["id"], "cancelled", quiet=True)
        _next_attempt(v, repo, run, run["triggering_actor"])
