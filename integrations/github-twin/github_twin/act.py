"""`github_act {person, action, args}`: what the world's people do on the code host, called by links.
Each action follows the same rules as the REST API."""
import os
import tempfile

from shakedown_part.twin import Done

from . import events, issue_rules, pull_rules, records, users
from .actions import jobs
from .errors import GithubError, not_found
from .gitcmd import attempt
from .repos import full_name
from .view import View


def _repo(v: View, args: dict):
    name = args.get("repo", "")
    row = records.repo_named(v.db, name if "/" in name else f"{v.config.get('org')}/{name}")
    if row is None:
        raise not_found()
    return row


def _commit_patch(v: View, repo, base: str, patch: str, person: str, message: str) -> str:
    fn = full_name(repo)
    with tempfile.TemporaryDirectory() as tmp:
        env = {"GIT_INDEX_FILE": os.path.join(tmp, "index")}
        attempt(v.repos.path(fn), "read-tree", base, env=env)
        ok, _ = attempt(v.repos.path(fn), "apply", "--cached", "-", input=patch.encode(), env=env)
        if not ok:
            raise GithubError(422, "the patch does not apply to the base branch")
        tree = attempt(v.repos.path(fn), "write-tree", env=env)[1].decode().strip()
    who = v.person(person)
    return v.repos.commit_tree(fn, tree, [base], message, who, who)


def open_pr(v: View, person: str, args: dict) -> Done:
    repo = _repo(v, args)
    fn, branch = full_name(repo), args.get("branch", "")
    base = args.get("base") or repo["default_branch"]
    base_sha = v.repos.branches(fn).get(base)
    if base_sha is None or not branch or branch in v.repos.branches(fn):
        raise GithubError(422, f"branch {branch!r} exists already or base {base!r} does not")
    sha = _commit_patch(v, repo, base_sha, args.get("patch", ""), person, args.get("title", ""))
    events.moved(v.ctx, repo, lambda: v.repos.update_ref(fn, f"refs/heads/{branch}", sha, "0" * 40), by=person)
    pr = pull_rules.open_pull(v, repo, person, branch, base, args.get("title", ""), args.get("body"))
    return Done({"number": pr["number"]}, writes=[f"pull/{fn}/{pr['number']}", f"branch/{fn}/{branch}"])


def _pull(v: View, args: dict):
    repo = _repo(v, args)
    return repo, records.pull(v.db, repo["id"], int(args.get("number", 0)))


def review(v: View, person: str, args: dict) -> Done:
    repo, pr = _pull(v, args)
    pull_rules.review(v, repo, pr, person, args.get("event"), args.get("body", ""))
    return Done({"number": pr["number"]}, writes=[f"pull/{full_name(repo)}/{pr['number']}"])


def merge(v: View, person: str, args: dict) -> Done:
    repo, pr = _pull(v, args)
    sha = pull_rules.merge(v, repo, pr, person, args.get("method", "merge"))
    fn = full_name(repo)
    return Done({"sha": sha}, writes=[f"pull/{fn}/{pr['number']}", f"branch/{fn}/{pr['base_ref']}"])


def _issue(v: View, args: dict):
    repo = _repo(v, args)
    return repo, records.issue(v.db, repo["id"], int(args.get("number", 0)))


def comment(v: View, person: str, args: dict) -> Done:
    repo, row = _issue(v, args)
    c = issue_rules.comment(v, repo, row, person, args.get("body", ""))
    return Done({"id": c["id"]}, writes=[_key(repo, row)])


def close(v: View, person: str, args: dict) -> Done:
    repo, row = _issue(v, args)
    issue_rules.set_state(v, repo, row, "closed", person, "completed")
    return Done({"number": row["number"]}, writes=[_key(repo, row)])


def _key(repo, row) -> str:
    return f"{'pull' if row['is_pull'] else 'issue'}/{full_name(repo)}/{row['number']}"


def deploy_result(v: View, person: str, args: dict) -> Done:
    """The world's answer to a deploy step (`{job_id, ok, error?}`); the job waiting for it goes on."""
    job_id = str(args.get("job_id", ""))
    if not job_id.isdigit():
        raise GithubError(422, "deploy_result needs the job_id of the deploy")
    refused = jobs.deploy_result(v, int(job_id), bool(args.get("ok")), args.get("error"))
    if refused is not None:
        raise GithubError(422, refused)
    run = jobs.run_row(v.db, jobs.job_row(v.db, int(job_id))["run_id"])
    repo = v.db.execute("SELECT * FROM repos WHERE id = ?", (run["repo_id"],)).fetchone()
    return Done({"job_id": job_id}, writes=[f"run/{full_name(repo)}/{run['id']}"])


ACTIONS = {"deploy_result": deploy_result, "open_pr": open_pr, "review": review, "merge": merge, "comment": comment, "close": close}


def run(ctx, person: str, action: str, args: dict) -> Done:
    fn = ACTIONS.get(action)
    if fn is None:
        return Done(error=f"unknown action {action}")
    v = View(ctx)
    users.ensure(v.db, person, v.now())
    try:
        return fn(v, person, args)
    except GithubError as e:
        return Done(error=e.message)
