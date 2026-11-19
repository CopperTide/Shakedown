"""One sample request per covered operation, for the schema test: `exercise(twin, op, tmp)`
answers `(status, body)`."""
from conftest import api, api_raw, b64

R = "/api/v3/repos/acme/app"


def _main(t):
    return api(t, "GET", f"{R}/branches/main")["commit"]["sha"]


def _tree(t):
    return api(t, "GET", f"{R}/commits/{_main(t)}")["commit"]["tree"]["sha"]


def _file_sha(t, path):
    return api(t, "GET", f"{R}/contents/{path}")["sha"]


def _topic(t):
    api(t, "POST", f"{R}/git/refs", {"ref": "refs/heads/topic", "sha": _main(t)})


def _blob(t):
    return api(t, "POST", f"{R}/git/blobs", {"content": "x\n"})["sha"]


def _pr(t):
    """PR 1 from branch `feat` (one new file), opened by coder."""
    _topic(t)
    api(t, "PUT", f"{R}/contents/f.txt", {"message": "Add f", "content": b64("f\n"), "branch": "topic"})
    api(t, "POST", f"{R}/pulls", {"title": "Add f", "head": "topic", "base": "main"})
    return api(t, "GET", f"{R}/pulls/1")


def _review(t, event="APPROVE"):
    _pr(t)
    return api(t, "POST", f"{R}/pulls/1/reviews", {"event": event, "body": "ok"} if event else {"body": "draft"},
               as_user="reviewer")["id"]


def _review_comment(t):
    pr = _pr(t)
    return api(t, "POST", f"{R}/pulls/1/comments", {"body": "Why?", "commit_id": pr["head"]["sha"], "path": "f.txt",
                                                    "line": 1, "side": "RIGHT"}, as_user="reviewer")["id"]


def _issue(t):
    api(t, "POST", f"{R}/issues", {"title": "Bug", "body": "Broken", "labels": ["bug"], "assignees": ["dana"]})


def _comment(t):
    _issue(t)
    return api(t, "POST", f"{R}/issues/1/comments", {"body": "Seen"})["id"]


def _label(t):
    api(t, "POST", f"{R}/labels", {"name": "ops", "color": "ff0000", "description": "Operations"})


def _status(t):
    api(t, "POST", f"{R}/statuses/{_main(t)}", {"state": "success", "context": "ci", "target_url": "https://ci.acme.test/1"})


def _run(t):
    return api(t, "POST", f"{R}/check-runs", {"name": "CI / test", "head_sha": _main(t), "status": "in_progress"})


def _hook(t):
    api(t, "POST", f"{R}/hooks", {"name": "web", "events": ["push"], "config": {"url": "http://127.0.0.1:9/h", "secret": "s"}})


P = f"{R}/pulls/1"
SAMPLES = {
    ("GET", "/"): lambda t: ("GET", "/api/v3/", None),
    ("GET", "/meta"): lambda t: ("GET", "/api/v3/meta", None),
    ("GET", "/rate_limit"): lambda t: ("GET", "/api/v3/rate_limit", None),
    ("GET", "/user"): lambda t: ("GET", "/api/v3/user", None),
    ("GET", "/users/{username}"): lambda t: ("GET", "/api/v3/users/coder", None),
    ("GET", "/repos/{owner}/{repo}"): lambda t: ("GET", R, None),
    ("GET", "/repos/{owner}/{repo}/branches"): lambda t: ("GET", f"{R}/branches", None),
    ("GET", "/repos/{owner}/{repo}/branches/{branch}"): lambda t: ("GET", f"{R}/branches/main", None),
    ("GET", "/repos/{owner}/{repo}/contents/{path}"): lambda t: ("GET", f"{R}/contents/app.py", None),
    ("PUT", "/repos/{owner}/{repo}/contents/{path}"): lambda t: (
        "PUT", f"{R}/contents/app.py", {"message": "Edit", "content": b64("y\n"), "sha": _file_sha(t, "app.py")}),
    ("DELETE", "/repos/{owner}/{repo}/contents/{path}"): lambda t: (
        "DELETE", f"{R}/contents/app.py", {"message": "Remove", "sha": _file_sha(t, "app.py")}),
    ("GET", "/repos/{owner}/{repo}/commits"): lambda t: ("GET", f"{R}/commits", None),
    ("GET", "/repos/{owner}/{repo}/commits/{ref}"): lambda t: ("GET", f"{R}/commits/main", None),
    ("GET", "/repos/{owner}/{repo}/compare/{basehead}"): lambda t: (_topic(t), ("GET", f"{R}/compare/main...topic", None))[1],
    ("GET", "/repos/{owner}/{repo}/git/ref/{ref}"): lambda t: ("GET", f"{R}/git/ref/heads/main", None),
    ("POST", "/repos/{owner}/{repo}/git/refs"): lambda t: ("POST", f"{R}/git/refs", {"ref": "refs/heads/n", "sha": _main(t)}),
    ("PATCH", "/repos/{owner}/{repo}/git/refs/{ref}"): lambda t: (_topic(t), ("PATCH", f"{R}/git/refs/heads/topic", {"sha": _main(t)}))[1],
    ("DELETE", "/repos/{owner}/{repo}/git/refs/{ref}"): lambda t: (_topic(t), ("DELETE", f"{R}/git/refs/heads/topic", None))[1],
    ("GET", "/repos/{owner}/{repo}/git/trees/{tree_sha}"): lambda t: ("GET", f"{R}/git/trees/{_tree(t)}?recursive=1", None),
    ("POST", "/repos/{owner}/{repo}/git/blobs"): lambda t: ("POST", f"{R}/git/blobs", {"content": "x\n"}),
    ("POST", "/repos/{owner}/{repo}/git/trees"): lambda t: ("POST", f"{R}/git/trees", {
        "base_tree": _tree(t), "tree": [{"path": "x.txt", "mode": "100644", "type": "blob", "sha": _blob(t)}]}),
    ("POST", "/repos/{owner}/{repo}/git/commits"): lambda t: ("POST", f"{R}/git/commits", {
        "message": "M", "tree": _tree(t), "parents": [_main(t)]}),
    ("POST", "/repos/{owner}/{repo}/pulls"): lambda t: (_topic(t), api(t, "PUT", f"{R}/contents/f.txt", {
        "message": "Add f", "content": b64("f\n"), "branch": "topic"}), ("POST", f"{R}/pulls", {
            "title": "Add f", "head": "topic", "base": "main", "body": "Adds f"}))[2],
    ("GET", "/repos/{owner}/{repo}/pulls"): lambda t: (_pr(t), ("GET", f"{R}/pulls", None))[1],
    ("GET", "/repos/{owner}/{repo}/pulls/{pull_number}"): lambda t: (_pr(t), ("GET", P, None))[1],
    ("PATCH", "/repos/{owner}/{repo}/pulls/{pull_number}"): lambda t: (_pr(t), ("PATCH", P, {"title": "New"}))[1],
    ("PUT", "/repos/{owner}/{repo}/pulls/{pull_number}/merge"): lambda t: (_pr(t), ("PUT", f"{P}/merge", {"merge_method": "squash"}))[1],
    ("GET", "/repos/{owner}/{repo}/pulls/{pull_number}/merge"): lambda t: (
        _pr(t), api(t, "PUT", f"{P}/merge", {}), ("GET", f"{P}/merge", None))[2],
    ("GET", "/repos/{owner}/{repo}/pulls/{pull_number}/files"): lambda t: (_pr(t), ("GET", f"{P}/files", None))[1],
    ("GET", "/repos/{owner}/{repo}/pulls/{pull_number}/commits"): lambda t: (_pr(t), ("GET", f"{P}/commits", None))[1],
    ("POST", "/repos/{owner}/{repo}/pulls/{pull_number}/reviews"): lambda t: (
        _pr(t), ("POST", f"{P}/reviews", {"event": "COMMENT", "body": "Hm", "comments": [{"path": "f.txt", "line": 1, "body": "x"}]}))[1],
    ("GET", "/repos/{owner}/{repo}/pulls/{pull_number}/reviews"): lambda t: (_review(t), ("GET", f"{P}/reviews", None))[1],
    ("POST", "/repos/{owner}/{repo}/pulls/{pull_number}/reviews/{review_id}/events"): lambda t: (
        "POST", f"{P}/reviews/{_review(t, None)}/events", {"event": "COMMENT", "body": "done"}),
    ("PUT", "/repos/{owner}/{repo}/pulls/{pull_number}/reviews/{review_id}/dismissals"): lambda t: (
        "PUT", f"{P}/reviews/{_review(t)}/dismissals", {"message": "stale"}),
    ("POST", "/repos/{owner}/{repo}/pulls/{pull_number}/comments"): lambda t: (
        "POST", f"{P}/comments", {"body": "Why?", "commit_id": _pr(t)["head"]["sha"], "path": "f.txt", "line": 1}),
    ("GET", "/repos/{owner}/{repo}/pulls/{pull_number}/comments"): lambda t: (_review_comment(t), ("GET", f"{P}/comments", None))[1],
    ("POST", "/repos/{owner}/{repo}/pulls/{pull_number}/comments/{comment_id}/replies"): lambda t: (
        "POST", f"{P}/comments/{_review_comment(t)}/replies", {"body": "Because"}),
    ("POST", "/repos/{owner}/{repo}/pulls/{pull_number}/requested_reviewers"): lambda t: (
        _pr(t), ("POST", f"{P}/requested_reviewers", {"reviewers": ["reviewer"]}))[1],
    ("DELETE", "/repos/{owner}/{repo}/pulls/{pull_number}/requested_reviewers"): lambda t: (
        _pr(t), ("DELETE", f"{P}/requested_reviewers", {"reviewers": ["reviewer"]}))[1],
    ("GET", "/repos/{owner}/{repo}/pulls/{pull_number}/requested_reviewers"): lambda t: (
        _pr(t), api(t, "POST", f"{P}/requested_reviewers", {"reviewers": ["reviewer"]}), ("GET", f"{P}/requested_reviewers", None))[2],
    ("POST", "/repos/{owner}/{repo}/issues"): lambda t: ("POST", f"{R}/issues", {"title": "Bug", "labels": ["bug"], "assignees": ["dana"]}),
    ("GET", "/repos/{owner}/{repo}/issues"): lambda t: (_pr(t), _issue(t), ("GET", f"{R}/issues", None))[2],
    ("GET", "/repos/{owner}/{repo}/issues/{issue_number}"): lambda t: (_issue(t), ("GET", f"{R}/issues/1", None))[1],
    ("PATCH", "/repos/{owner}/{repo}/issues/{issue_number}"): lambda t: (_issue(t), ("PATCH", f"{R}/issues/1", {"state": "closed"}))[1],
    ("POST", "/repos/{owner}/{repo}/issues/{issue_number}/comments"): lambda t: (_issue(t), ("POST", f"{R}/issues/1/comments", {"body": "Hi"}))[1],
    ("GET", "/repos/{owner}/{repo}/issues/{issue_number}/comments"): lambda t: (_comment(t), ("GET", f"{R}/issues/1/comments", None))[1],
    ("PATCH", "/repos/{owner}/{repo}/issues/comments/{comment_id}"): lambda t: ("PATCH", f"{R}/issues/comments/{_comment(t)}", {"body": "Edited"}),
    ("DELETE", "/repos/{owner}/{repo}/issues/comments/{comment_id}"): lambda t: ("DELETE", f"{R}/issues/comments/{_comment(t)}", None),
    ("POST", "/repos/{owner}/{repo}/issues/{issue_number}/labels"): lambda t: (_issue(t), ("POST", f"{R}/issues/1/labels", {"labels": ["p1"]}))[1],
    ("GET", "/repos/{owner}/{repo}/issues/{issue_number}/labels"): lambda t: (_issue(t), ("GET", f"{R}/issues/1/labels", None))[1],
    ("PUT", "/repos/{owner}/{repo}/issues/{issue_number}/labels"): lambda t: (_issue(t), ("PUT", f"{R}/issues/1/labels", {"labels": ["p2"]}))[1],
    ("DELETE", "/repos/{owner}/{repo}/issues/{issue_number}/labels/{name}"): lambda t: (_issue(t), ("DELETE", f"{R}/issues/1/labels/bug", None))[1],
    ("POST", "/repos/{owner}/{repo}/labels"): lambda t: ("POST", f"{R}/labels", {"name": "ops", "color": "ff0000"}),
    ("GET", "/repos/{owner}/{repo}/labels"): lambda t: (_label(t), ("GET", f"{R}/labels", None))[1],
    ("PATCH", "/repos/{owner}/{repo}/labels/{name}"): lambda t: (_label(t), ("PATCH", f"{R}/labels/ops", {"new_name": "infra"}))[1],
    ("DELETE", "/repos/{owner}/{repo}/labels/{name}"): lambda t: (_label(t), ("DELETE", f"{R}/labels/ops", None))[1],
    ("GET", "/search/issues"): lambda t: (_pr(t), _issue(t), ("GET", "/api/v3/search/issues?q=repo:acme/app+is:open", None))[2],
    ("POST", "/repos/{owner}/{repo}/statuses/{sha}"): lambda t: ("POST", f"{R}/statuses/{_main(t)}", {"state": "pending", "context": "ci"}),
    ("GET", "/repos/{owner}/{repo}/commits/{ref}/status"): lambda t: (_status(t), ("GET", f"{R}/commits/main/status", None))[1],
    ("GET", "/repos/{owner}/{repo}/commits/{ref}/statuses"): lambda t: (_status(t), ("GET", f"{R}/commits/main/statuses", None))[1],
    ("POST", "/repos/{owner}/{repo}/check-runs"): lambda t: ("POST", f"{R}/check-runs", {
        "name": "CI / test", "head_sha": _main(t), "status": "completed", "conclusion": "success",
        "output": {"title": "ok", "summary": "all passed"}}),
    ("PATCH", "/repos/{owner}/{repo}/check-runs/{check_run_id}"): lambda t: (
        "PATCH", f"{R}/check-runs/{_run(t)['id']}", {"conclusion": "failure"}),
    ("GET", "/repos/{owner}/{repo}/check-runs/{check_run_id}"): lambda t: ("GET", f"{R}/check-runs/{_run(t)['id']}", None),
    ("GET", "/repos/{owner}/{repo}/commits/{ref}/check-runs"): lambda t: (_pr(t), _run(t), ("GET", f"{R}/commits/main/check-runs", None))[2],
    ("GET", "/repos/{owner}/{repo}/check-suites/{check_suite_id}/check-runs"): lambda t: (
        "GET", f"{R}/check-suites/{_run(t)['check_suite']['id']}/check-runs", None),
    ("POST", "/repos/{owner}/{repo}/hooks"): lambda t: ("POST", f"{R}/hooks", {"name": "web", "events": ["push"], "config": {"url": "http://127.0.0.1:9/h"}}),
    ("GET", "/repos/{owner}/{repo}/hooks"): lambda t: (_hook(t), ("GET", f"{R}/hooks", None))[1],
    ("GET", "/repos/{owner}/{repo}/hooks/{hook_id}"): lambda t: (_hook(t), ("GET", f"{R}/hooks/1", None))[1],
    ("PATCH", "/repos/{owner}/{repo}/hooks/{hook_id}"): lambda t: (_hook(t), ("PATCH", f"{R}/hooks/1", {"active": False}))[1],
    ("DELETE", "/repos/{owner}/{repo}/hooks/{hook_id}"): lambda t: (_hook(t), ("DELETE", f"{R}/hooks/1", None))[1],
    ("POST", "/repos/{owner}/{repo}/hooks/{hook_id}/pings"): lambda t: (_hook(t), ("POST", f"{R}/hooks/1/pings", None))[1],
    ("POST", "/repos/{owner}/{repo}/hooks/{hook_id}/tests"): lambda t: (_hook(t), ("POST", f"{R}/hooks/1/tests", None))[1],
}


def exercise(twin, op, tmp_path):
    method, path, body = SAMPLES[op](twin)
    r = api_raw(twin, method, path, body)
    return r.status, r.json()
