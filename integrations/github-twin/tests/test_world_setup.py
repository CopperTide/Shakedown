"""What a world's config sets up beyond the repositories: the commit each was seeded with (in its doc, so checks
can tell an untouched branch), and reviewers assigned to each new pull request in turn, as a GitHub team's code
review assignment does."""
from conftest import api, config, started

from test_act import PATCH, act

R = "/api/v3/repos/acme/app"


def test_a_repo_doc_names_the_commit_it_was_seeded_with(twin):
    seeded = twin.docs()["branch/acme/app/main"]["sha"]
    assert twin.docs()["repo/acme/app"]["seed_sha"] == seeded
    act(twin, "dana", "open_pr", repo="app", branch="swap", patch=PATCH, title="Swap", body="")
    act(twin, "lee", "review", repo="app", number=1, event="APPROVE", body="ok")
    act(twin, "dana", "merge", repo="app", number=1)
    docs = twin.docs()
    assert docs["branch/acme/app/main"]["sha"] != seeded and docs["repo/acme/app"]["seed_sha"] == seeded


def _requested(twin, number: int) -> list:
    return [u["login"] for u in api(twin, "GET", f"{R}/pulls/{number}/requested_reviewers")["users"]]


def test_each_new_pull_request_gets_the_next_reviewers_in_turn_never_its_author():
    twin = started(config(review_assignment={"reviewers": ["lee", "kim", "dana"], "count": 1}))
    try:
        for n, (author, branch) in enumerate([("dana", "a"), ("lee", "b"), ("dana", "c")], 1):
            assert act(twin, author, "open_pr", repo="app", branch=branch, patch=PATCH, title=branch, body="")[
                "result"] == {"number": n}
        assert [_requested(twin, n) for n in (1, 2, 3)] == [["lee"], ["kim"], ["lee"]]
        events = twin.part.advance("i", 100)["events"]
        asked = [(e["data"]["data"]["number"], e["data"]["data"]["reviewer"]) for e in events
                 if e["data"]["kind"] == "review_requested"]
        assert asked == [(1, "lee"), (2, "kim"), (3, "lee")]
    finally:
        twin.part.stop("i")


def test_without_review_assignment_no_one_is_asked(twin):
    act(twin, "dana", "open_pr", repo="app", branch="a", patch=PATCH, title="a", body="")
    assert _requested(twin, 1) == []


def test_a_branch_doc_counts_the_moves_no_merge_made(with_clone):
    from conftest import branch_from_main, commit, create_pr, git, push
    t = with_clone
    assert t.docs()["branch/acme/app/main"]["direct_pushes"] == 0
    branch_from_main(t, "add-b", "b.txt", "two\n", "Add b")
    number = create_pr(t, "add-b", title="Add b")
    api(t, "POST", f"{R}/pulls/{number}/reviews", {"event": "APPROVE"}, as_user="lee")
    api(t, "PUT", f"{R}/pulls/{number}/merge", {"merge_method": "squash"})
    docs = t.docs()
    assert docs["branch/acme/app/main"]["direct_pushes"] == 0 and docs["branch/acme/app/add-b"]["direct_pushes"] == 1
    git(t.clone, "fetch", "-q", "origin", creds=t.token)
    git(t.clone, "reset", "-q", "--hard", "origin/main")
    commit(t.clone, "c.txt", "three\n", "Add c straight to main")
    push(t.clone, t.token, "HEAD:main")
    api(t, "PUT", f"{R}/contents/d.txt", {"message": "Add d", "content": "ZA==", "branch": "main"})
    assert t.docs()["branch/acme/app/main"]["direct_pushes"] == 2
