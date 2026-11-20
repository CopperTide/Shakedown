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
