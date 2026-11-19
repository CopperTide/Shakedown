"""A write that moves a git ref and then fails leaves the ref where it was, as it leaves the store."""
import pytest

from conftest import api, api_raw, branch_from_main, commit, create_pr, git, push
from github_twin import notices, records, routes
from github_twin.errors import GithubError

R = "/api/v3/repos/acme/app"


def main_sha(t):
    return api(t, "GET", f"{R}/branches/main")["commit"]["sha"]


def test_a_merge_that_fails_after_moving_the_base_leaves_it(with_clone, monkeypatch):
    branch_from_main(with_clone, "f", "f.txt", "f\n")
    n, before = create_pr(with_clone, "f"), main_sha(with_clone)
    monkeypatch.setattr(records, "close", lambda *a: (_ for _ in ()).throw(RuntimeError("after the move")))
    assert api_raw(with_clone, "PUT", f"{R}/pulls/{n}/merge", {}, as_user="lead").status == 500
    assert main_sha(with_clone) == before
    assert api(with_clone, "GET", f"{R}/pulls/{n}")["state"] == "open"


def test_a_merge_refused_after_moving_the_base_leaves_it(with_clone, monkeypatch):
    branch_from_main(with_clone, "f", "f.txt", "f\n")
    n, before = create_pr(with_clone, "f"), main_sha(with_clone)

    def refuse(*a):
        raise GithubError(422, "refused after the move")

    monkeypatch.setattr(notices, "pr_merged", refuse)
    assert api_raw(with_clone, "PUT", f"{R}/pulls/{n}/merge", {}, as_user="lead").status == 422
    assert main_sha(with_clone) == before


def test_a_refused_open_pr_act_leaves_no_branch(twin):
    patch = "diff --git a/n.txt b/n.txt\nnew file mode 100644\n--- /dev/null\n+++ b/n.txt\n@@ -0,0 +1 @@\n+n\n"
    out = twin.part.call_tool("i", "github_act", {"person": "dana", "action": "open_pr", "args": {
        "repo": "app", "branch": "nb", "patch": patch, "title": ""}}, 0)
    assert "error" in out
    assert api_raw(twin, "GET", f"{R}/branches/nb").status == 404
    assert [b["name"] for b in api(twin, "GET", f"{R}/branches")] == ["main"]


def test_a_push_whose_follow_up_fails_is_undone(with_clone, monkeypatch):
    before = main_sha(with_clone)
    commit(with_clone.clone, "p.txt", "p\n")
    monkeypatch.setattr(routes.events, "after_push", lambda *a, **k: (_ for _ in ()).throw(RuntimeError("after")))
    assert git(with_clone.clone, "push", "-q", "origin", "HEAD:main", creds=with_clone.token, check=False).returncode != 0
    assert main_sha(with_clone) == before
