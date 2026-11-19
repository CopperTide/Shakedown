"""Reviews, review comments and requested reviewers of a pull request."""
from .. import pull_rules, records
from ..errors import invalid, not_found
from ..repos import full_name
from . import pull_shapes
from .api import created, paged
from .pulls import the_pull


def _review_id(call) -> int:
    return int(call.req.params["review_id"])


def create_review(call):
    repo, pr = the_pull(call)
    b = call.body()
    r = pull_rules.review(call, repo, pr, call.login, b.get("event"), b.get("body", ""), b.get("commit_id"))
    for c in b.get("comments") or []:
        _comment(call, repo, pr, {**c, "commit_id": r["commit_id"]}, review_id=r["id"])
    return pull_shapes.review(call, repo, pr, r)


def list_reviews(call):
    repo, pr = the_pull(call)
    resp = paged(call, records.reviews_of(call.db, pr["id"]))
    resp.body = [pull_shapes.review(call, repo, pr, r) for r in resp.body]
    return resp


def submit_review(call):
    repo, pr = the_pull(call)
    b = call.body()
    return pull_shapes.review(call, repo, pr, pull_rules.submit(call, repo, pr, _review_id(call), call.login,
                                                                b.get("event", "COMMENT"), b.get("body")))


def dismiss_review(call):
    repo, pr = the_pull(call)
    return pull_shapes.review(call, repo, pr, pull_rules.dismiss(call, repo, pr, _review_id(call), call.login))


def _hunk(call, repo, pr, path: str) -> str:
    fn = full_name(repo)
    base = call.repos.merge_base(fn, pull_shapes.base_sha(call, repo, pr), pr["head_sha"]) or pr["base_sha"]
    out = call.repos.git(fn, "diff", "--no-color", base, pr["head_sha"], "--", path)
    at = out.find("\n@@")
    return out[at + 1:].rstrip("\n") if at >= 0 else ""


def _comment(call, repo, pr, b: dict, review_id=None, reply_to=None):
    if not b.get("body") or not b.get("path"):
        raise invalid("Validation Failed", [{"resource": "PullRequestReviewComment", "code": "missing_field", "field": "path"}])
    now = call.now()
    cur = call.db.execute(
        "INSERT INTO review_comments(issue_id, review_id, user, body, path, line, side, commit_id, diff_hunk, in_reply_to, "
        "created_ms, updated_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        (pr["id"], review_id, call.login, b["body"], b["path"], b.get("line") or b.get("position"), b.get("side"),
         b.get("commit_id") or pr["head_sha"], _hunk(call, repo, pr, b["path"]), reply_to, now, now))
    return call.db.execute("SELECT * FROM review_comments WHERE id = ?", (cur.lastrowid,)).fetchone()


def create_review_comment(call):
    repo, pr = the_pull(call)
    b = call.body()
    if b.get("in_reply_to"):
        return reply(call, int(b["in_reply_to"]), b)
    return created(pull_shapes.review_comment(call, repo, pr, _comment(call, repo, pr, b)))


def list_review_comments(call):
    repo, pr = the_pull(call)
    rows = call.db.execute("SELECT * FROM review_comments WHERE issue_id = ? ORDER BY id", (pr["id"],)).fetchall()
    resp = paged(call, rows)
    resp.body = [pull_shapes.review_comment(call, repo, pr, c) for c in resp.body]
    return resp


def reply(call, comment_id: int | None = None, b: dict | None = None):
    repo, pr = the_pull(call)
    comment_id = comment_id or int(call.req.params["comment_id"])
    to = call.db.execute("SELECT * FROM review_comments WHERE id = ? AND issue_id = ?", (comment_id, pr["id"])).fetchone()
    if to is None:
        raise not_found()
    b = b or call.body()
    c = _comment(call, repo, pr, {"body": b.get("body"), "path": to["path"], "line": to["line"], "side": to["side"],
                                  "commit_id": to["commit_id"]}, to["review_id"], to["id"])
    return created(pull_shapes.review_comment(call, repo, pr, c))


def request_reviewers(call):
    repo, pr = the_pull(call)
    pull_rules.request_reviewers(call, repo, pr, call.login, call.body().get("reviewers") or [])
    return created(pull_shapes.simple(call, repo, records.pull(call.db, repo["id"], pr["number"])))


def remove_reviewers(call):
    repo, pr = the_pull(call)
    pull_rules.remove_reviewers(call, pr, call.body().get("reviewers") or [])
    return pull_shapes.simple(call, repo, records.pull(call.db, repo["id"], pr["number"]))


def requested_reviewers(call):
    _, pr = the_pull(call)
    return {"users": [pull_shapes.user(call, r) for r in records.requested(call.db, pr["id"])], "teams": []}


def register(api) -> None:
    p = "/repos/{owner}/{repo}/pulls/{pull_number}"
    api.route("POST", p + "/reviews")(create_review)
    api.route("GET", p + "/reviews")(list_reviews)
    api.route("POST", p + "/reviews/{review_id}/events")(submit_review)
    api.route("PUT", p + "/reviews/{review_id}/dismissals")(dismiss_review)
    api.route("POST", p + "/comments")(create_review_comment)
    api.route("GET", p + "/comments")(list_review_comments)
    api.route("POST", p + "/comments/{comment_id}/replies")(lambda call: reply(call))
    api.route("POST", p + "/requested_reviewers")(request_reviewers)
    api.route("DELETE", p + "/requested_reviewers")(remove_reviewers)
    api.route("GET", p + "/requested_reviewers")(requested_reviewers)
