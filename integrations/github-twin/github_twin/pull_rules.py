"""Pull requests: opening, reviews, review requests and merging, with GitHub's rules and words.
An author cannot approve or request changes on their own PR; approvals count only on the head
that is merged, and what counted is recorded at merge."""
import json

from . import ci, events, merging, notices, records, users, webhooks
from .actions import runs
from .errors import GithubError, invalid
from .repos import full_name
from .rest import pull_shapes

STATES = {"APPROVE": "APPROVED", "REQUEST_CHANGES": "CHANGES_REQUESTED", "COMMENT": "COMMENTED"}
OWN = {"APPROVE": "Can not approve your own pull request",
       "REQUEST_CHANGES": "Can not request changes on your own pull request"}


def _failed(field: str, code: str = "invalid", message: str | None = None) -> GithubError:
    err = {"resource": "PullRequest", "field": field, "code": code}
    return invalid("Validation Failed", [{**err, "message": message} if message else err])


def _hook(v, repo, pr, action: str, sender: str, **extra) -> None:
    pr = records.pull(v.db, repo["id"], pr["number"])
    webhooks.fire(v, repo, "pull_request", sender, action, number=pr["number"],
                  pull_request=pull_shapes.simple(v, repo, pr), **extra)


def _check_open(v, repo, head: str, base: str) -> None:
    fn = full_name(repo)
    branches = v.repos.branches(fn)
    if head not in branches:
        raise _failed("head")
    if base not in branches:
        raise _failed("base")
    if records.pulls(v.db, repo["id"], "i.state = 'open' AND p.head_ref = ? AND p.base_ref = ?", (head, base)):
        raise _failed("head", "custom", f"A pull request already exists for {repo['owner']}:{head}.")
    if not v.repos.rev_list(fn, f"{branches[base]}..{branches[head]}"):
        raise _failed("head", "custom", f"No commits between {base} and {head}")


def open_pull(v, repo, by: str, head: str, base: str | None, title: str, body: str | None, draft: bool = False):
    head, base = head.split(":", 1)[-1], base or repo["default_branch"]
    _check_open(v, repo, head, base)
    if not title:
        raise _failed("title", "missing_field")
    branches = v.repos.branches(full_name(repo))
    row = records.insert_issue(v.db, repo["id"], title, body, by, v.now(), is_pull=True)
    v.db.execute("INSERT INTO pulls(issue_id, head_ref, head_sha, base_ref, base_sha, draft) VALUES (?, ?, ?, ?, ?, ?)",
                 (row["id"], head, branches[head], base, branches[base], int(draft)))
    v.repos.set_pull_ref(full_name(repo), row["number"], branches[head])
    pr = records.pull(v.db, repo["id"], row["number"])
    notices.pr_opened(v.ctx, full_name(repo), pr)
    notices.mentions(v.ctx, full_name(repo), pr["number"], by, body)
    _hook(v, repo, pr, "opened", by)
    runs.on_pull_request(v, repo, pr, by)
    return pr


def _own(pr, by: str, event: str | None) -> None:
    if by == pr["user"] and event in OWN:
        raise invalid(OWN[event])


def review(v, repo, pr, by: str, event: str | None, body: str, commit_id: str | None = None):
    """A review; no `event` leaves it PENDING. Its commit is the head unless given."""
    _own(pr, by, event)
    if event is not None and event not in STATES:
        raise invalid(f"Unprocessable Entity: event {event} is not one of APPROVE, REQUEST_CHANGES, COMMENT")
    state = STATES.get(event, "PENDING")
    now = v.now()
    cur = v.db.execute("INSERT INTO reviews(issue_id, user, body, state, commit_id, submitted_ms) VALUES (?, ?, ?, ?, ?, ?)",
                       (pr["id"], by, body or "", state, commit_id or pr["head_sha"], None if state == "PENDING" else now))
    users.ensure(v.db, by, now)
    r = v.db.execute("SELECT * FROM reviews WHERE id = ?", (cur.lastrowid,)).fetchone()
    if state != "PENDING":
        _submitted(v, repo, pr, r, by)
    return r


def _submitted(v, repo, pr, r, by: str) -> None:
    v.db.execute("DELETE FROM review_requests WHERE issue_id = ? AND login = ?", (pr["id"], by))
    records.touch(v.db, pr["id"], v.now())
    notices.mentions(v.ctx, full_name(repo), pr["number"], by, r["body"])
    webhooks.fire(v, repo, "pull_request_review", by, "submitted", review=pull_shapes.review(v, repo, pr, r),
                  pull_request=pull_shapes.simple(v, repo, pr))


def _review(v, pr, review_id: int):
    r = v.db.execute("SELECT * FROM reviews WHERE id = ? AND issue_id = ?", (review_id, pr["id"])).fetchone()
    if r is None:
        raise GithubError(404, "Not Found")
    return r


def submit(v, repo, pr, review_id: int, by: str, event: str, body: str | None):
    r = _review(v, pr, review_id)
    if r["state"] != "PENDING":
        raise invalid("Can not submit a review that is not pending")
    _own(pr, r["user"], event)
    v.db.execute("UPDATE reviews SET state = ?, body = ?, submitted_ms = ? WHERE id = ?",
                 (STATES.get(event, "COMMENTED"), body if body is not None else r["body"], v.now(), r["id"]))
    r = _review(v, pr, review_id)
    _submitted(v, repo, pr, r, r["user"])
    return r


def dismiss(v, repo, pr, review_id: int, by: str):
    r = _review(v, pr, review_id)
    if r["state"] not in ("APPROVED", "CHANGES_REQUESTED"):
        raise invalid("Can not dismiss a review that is not approved or changes requested")
    v.db.execute("UPDATE reviews SET state = 'DISMISSED' WHERE id = ?", (r["id"],))
    return _review(v, pr, review_id)


def request_reviewers(v, repo, pr, by: str, reviewers: list) -> None:
    if pr["user"] in reviewers:
        raise invalid("Review cannot be requested from pull request author.")
    checks = ci.head_checks(v.db, repo["id"], pr["head_sha"])
    for login in reviewers:
        users.ensure(v.db, login, v.now())
        v.db.execute("INSERT OR IGNORE INTO review_requests VALUES (?, ?)", (pr["id"], login))
        notices.review_requested(v.ctx, full_name(repo), pr, login, by, checks)
        _hook(v, repo, pr, "review_requested", by, requested_reviewer=pull_shapes.user(v, login))


def remove_reviewers(v, pr, reviewers: list) -> None:
    v.db.executemany("DELETE FROM review_requests WHERE issue_id = ? AND login = ?", [(pr["id"], r) for r in reviewers])


def _message(pr, method: str, title: str | None, message: str | None, owner: str) -> str:
    if method == "squash":
        head = title or f"{pr['title']} (#{pr['number']})"
        return f"{head}\n\n{message if message is not None else pr['body'] or ''}".rstrip() + "\n"
    head = title or f"Merge pull request #{pr['number']} from {owner}/{pr['head_ref']}"
    return f"{head}\n\n{message if message is not None else pr['title']}\n"


def _check_merge(v, repo, pr, sha: str | None) -> list:
    """The approvals on the head; refuses what GitHub refuses."""
    if pr["state"] != "open" or pr["merged"]:
        raise GithubError(405, "Pull Request is not mergeable")
    if sha and sha != pr["head_sha"]:
        raise GithubError(409, "Head branch was modified. Review and try the merge again.")
    approvals, needed = records.approvals_on(v.db, pr["id"], pr["head_sha"]), pull_shapes.required_reviews(v, repo)
    if len(approvals) < needed:
        s = "" if needed == 1 else "s"
        raise GithubError(405, f"At least {needed} approving review{s} is required by reviewers with write access.")
    return approvals


def merge(v, repo, pr, by: str, method: str = "merge", title: str | None = None, message: str | None = None,
          sha: str | None = None) -> str:
    approvals = _check_merge(v, repo, pr, sha)
    if method not in ("merge", "squash", "rebase"):
        raise invalid(f"Invalid merge_method {method}")
    fn, base = full_name(repo), pull_shapes.base_sha(v, repo, pr)
    new = merging.merge(v.repos, fn, base, pr["head_sha"], method, _message(pr, method, title, message, repo["owner"]),
                        v.person(by))
    events.moved(v.ctx, repo, lambda: v.repos.update_ref(fn, f"refs/heads/{pr['base_ref']}", new, base), by=by)
    records.set_pull(v.db, pr["id"], merged=1, merged_ms=v.now(), merged_by=by, merge_commit_sha=new, base_sha=base,
                     reviewed_at_merge=int(bool(approvals)), approvals_at_merge=json.dumps(approvals))
    records.close(v.db, pr["id"], by, v.now())
    notices.pr_merged(v.ctx, fn, pr["number"], by, new)
    _hook(v, repo, pr, "closed", by)
    return new
