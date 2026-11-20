"""Every query and mutation gh 2.45.0 sent the twin (tests/gh_queries), run with their own text over a
twin holding a pull request with checks, a review and a comment, and an issue."""
from pathlib import Path

import pytest
from graphql import parse

from conftest import TOKEN, api, branch_from_main, config, git, gql, gql_data, started

QUERIES = Path(__file__).parent / "gh_queries" / "gh-2.45.0"
R = "/api/v3/repos/acme/app"


def text(name: str) -> str:
    """The query gh sent for operation `name` (the first, when it sent several)."""
    return sorted(QUERIES.glob(f"{name}-*.graphql"))[0].read_text()


def declared(query: str) -> set:
    ops = [d for d in parse(query).definitions if hasattr(d, "operation")]
    return {v.variable.name.value for v in ops[0].variable_definitions or ()}


def is_mutation(path: Path) -> bool:
    return path.read_text().split("\n", 1)[1].lstrip().startswith("mutation")


READS = sorted(p for p in QUERIES.glob("*.graphql") if not is_mutation(p))


@pytest.fixture(scope="module")
def world(tmp_path_factory):
    """PR #1 (head `f`, a check run, a pending status, reviewer's approval, a comment) and issue #2."""
    tmp = tmp_path_factory.mktemp("world")
    mp = pytest.MonkeyPatch()
    mp.setenv("SHAKEDOWN_SAVES", str(tmp / "saves"))
    t = started(config())
    try:
        git(tmp, "clone", "-q", t.git_url("acme", "app"), "app", creds=TOKEN)
        t.clone = tmp / "app"
        sha = branch_from_main(t, "f", "f.txt", "f\n")
        _fill(t, sha)
        yield t
    finally:
        t.part.stop("i")
        mp.undo()


def _fill(t, sha: str) -> None:
    api(t, "POST", f"{R}/labels", {"name": "bug"})
    pr = api(t, "POST", f"{R}/pulls", {"title": "F", "head": "f", "base": "main", "body": "Fixes it @reviewer"})
    api(t, "POST", f"{R}/check-runs", {"name": "build", "head_sha": sha, "status": "completed", "conclusion": "success"})
    api(t, "POST", f"{R}/statuses/{sha}", {"state": "pending", "context": "ci/lint"})
    api(t, "POST", f"{R}/pulls/1/requested_reviewers", {"reviewers": ["reviewer"]})
    api(t, "POST", f"{R}/pulls/1/reviews", {"event": "APPROVE", "body": "Good"}, as_user="reviewer")
    api(t, "POST", f"{R}/issues/1/comments", {"body": "Thanks"})
    api(t, "POST", f"{R}/issues", {"title": "Bug", "body": "Breaks", "labels": ["bug"], "assignees": ["coder"]})
    t.pr_id = pr["node_id"]


def values(t) -> dict:
    """A value for each variable gh's operations declare."""
    return {"owner": "acme", "repo": "app", "name": "app", "pr_number": 1, "number": 1, "headRefName": "f",
            "states": ["OPEN"], "state": ["OPEN"], "limit": 30, "per_page": 10, "id": t.pr_id, "viewer": "coder",
            "type": "ISSUE", "query": "repo:acme/app is:issue bug", "q": "repo:acme/app is:pr",
            "viewerQuery": "repo:acme/app is:pr is:open author:coder",
            "reviewerQuery": "repo:acme/app is:pr is:open review-requested:coder"}


@pytest.mark.parametrize("q", READS, ids=lambda p: p.stem)
def test_every_gh_read_answers_without_errors(world, q):
    query = q.read_text()
    vars_ = {k: v for k, v in values(world).items() if k in declared(query)}
    if q.stem.startswith("Issue") and "number" in vars_:
        vars_["number"] = 2  # issue(number:) of a pull request's number is NOT_FOUND, as on GitHub
    data = gql_data(world, query, vars_)
    assert data and all(v is not None for v in data.values())


def test_gh_pr_view_sees_the_checks_review_and_comment(world):
    data = gql_data(world, max((p.read_text() for p in QUERIES.glob("PullRequestByNumber-*")), key=len),
                    {"owner": "acme", "repo": "app", "pr_number": 1})
    pr = data["repository"]["pullRequest"]
    contexts = pr["statusCheckRollup"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"]["nodes"]
    assert [(c["__typename"], c.get("name") or c.get("context")) for c in contexts] == [
        ("CheckRun", "build"), ("StatusContext", "ci/lint")]
    assert [(r["author"]["login"], r["state"]) for r in pr["latestReviews"]["nodes"]] == [("reviewer", "APPROVED")]
    assert [c["body"] for c in pr["comments"]["nodes"]] == ["Thanks"]
    assert (pr["mergeable"], pr["mergeStateStatus"], pr["reviewDecision"]) == ("MERGEABLE", "UNSTABLE", None)


def test_a_search_finds_what_rest_search_finds(world):
    data = gql_data(world, text("PullRequestStatus"), {k: v for k, v in values(world).items()
                                                       if k in declared(text("PullRequestStatus"))})
    assert data["viewerCreated"]["totalCount"] == 1
    assert data["reviewRequested"]["totalCount"] == 0  # the review was given: no request is left


# --- mutations, each with gh's own text ---------------------------------------------------
def run(t, name: str, input_: dict, **kw) -> dict:
    return gql_data(t, text(name), {"input": input_}, **kw)


def ids(t) -> dict:
    d = gql_data(t, '{ repository(owner: "acme", name: "app") { id label(name: "bug") { id } '
                    'pullRequest(number: 1) { id } } u: user(login: "coder") { id } r: user(login: "reviewer") { id } }')
    return {"repo": d["repository"]["id"], "label": d["repository"]["label"]["id"],
            "pr": d["repository"]["pullRequest"]["id"], "coder": d["u"]["id"], "reviewer": d["r"]["id"]}


@pytest.fixture
def opened(with_clone):
    branch_from_main(with_clone, "f", "f.txt", "f\n")
    api(with_clone, "POST", f"{R}/labels", {"name": "bug"})
    api(with_clone, "GET", "/api/v3/user", as_user="reviewer")
    repo = gql_data(with_clone, '{ repository(owner: "acme", name: "app") { id } }')["repository"]["id"]
    run(with_clone, "PullRequestCreate", {"repositoryId": repo, "baseRefName": "main", "headRefName": "f",
                                          "title": "F", "body": "B", "draft": True})
    with_clone.ids = ids(with_clone)
    return with_clone


def pull(t) -> dict:
    return api(t, "GET", f"{R}/pulls/1")


def test_pull_request_mutations(opened):
    t, i = opened, opened.ids
    run(t, "PullRequestCreateMetadata", {"pullRequestId": i["pr"], "labelIds": [i["label"]], "assigneeIds": [i["coder"]]})
    run(t, "PullRequestCreateRequestReviews", {"pullRequestId": i["pr"], "userIds": [i["reviewer"]], "union": True})
    run(t, "PullRequestReadyForReview", {"pullRequestId": i["pr"]})
    run(t, "PullRequestUpdate", {"pullRequestId": i["pr"], "title": "F2", "body": "B2"})
    p = pull(t)
    assert (p["title"], p["body"], p["draft"]) == ("F2", "B2", False)
    assert [x["name"] for x in p["labels"]] == ["bug"] and [x["login"] for x in p["assignees"]] == ["coder"]
    assert [x["login"] for x in p["requested_reviewers"]] == ["reviewer"]
    run(t, "PullRequestUpdateRequestReviews", {"pullRequestId": i["pr"], "userIds": [], "union": False})
    assert pull(t)["requested_reviewers"] == []
    run(t, "LabelRemove", {"labelableId": i["pr"], "labelIds": [i["label"]]})
    run(t, "LabelAdd", {"labelableId": i["pr"], "labelIds": [i["label"]]})
    assert [x["name"] for x in pull(t)["labels"]] == ["bug"]
    run(t, "PullRequestClose", {"pullRequestId": i["pr"]})
    assert pull(t)["state"] == "closed"
    run(t, "PullRequestReopen", {"pullRequestId": i["pr"]})
    assert pull(t)["state"] == "open"


def test_review_comment_and_merge(opened):
    t, i = opened, opened.ids
    own = gql(t, text("PullRequestReviewAdd"), {"input": {"pullRequestId": i["pr"], "event": "APPROVE"}})
    assert own["errors"][0]["message"] == "Can not approve your own pull request"
    run(t, "PullRequestReviewAdd", {"pullRequestId": i["pr"], "event": "APPROVE", "body": "Good"}, as_user="reviewer")
    url = run(t, "CommentCreate", {"subjectId": i["pr"], "body": "Thanks"})["addComment"]["commentEdge"]["node"]["url"]
    assert url == "https://github.acme.test/acme/app/pull/1#issuecomment-1"
    run(t, "PullRequestReadyForReview", {"pullRequestId": i["pr"]})
    head = pull(t)["head"]["sha"]
    run(t, "PullRequestMerge", {"pullRequestId": i["pr"], "mergeMethod": "SQUASH", "expectedHeadOid": head})
    doc = t.docs()["pull/acme/app/1"]
    assert (doc["merged"], doc["merged_by"], doc["reviewed_at_merge"]) == (True, "coder", True)


def test_issue_mutations(opened):
    t, i = opened, opened.ids
    out = run(t, "IssueCreate", {"repositoryId": i["repo"], "title": "Bug", "body": "B", "labelIds": [i["label"]],
                                 "assigneeIds": [i["coder"]]})
    issue_id, n = out["createIssue"]["issue"]["id"], int(out["createIssue"]["issue"]["url"].rsplit("/", 1)[1])
    run(t, "IssueUpdate", {"id": issue_id, "title": "Bug2", "assigneeIds": []})
    run(t, "CommentCreate", {"subjectId": issue_id, "body": "More"})
    run(t, "IssueClose", {"issueId": issue_id, "stateReason": "NOT_PLANNED"})
    got = api(t, "GET", f"{R}/issues/{n}")
    assert (got["title"], got["assignees"], got["state"], got["state_reason"], got["comments"]) == (
        "Bug2", [], "closed", "not_planned", 1)
    run(t, "IssueReopen", {"issueId": issue_id})
    assert api(t, "GET", f"{R}/issues/{n}")["state"] == "open"


def test_a_mutation_naming_no_node_is_not_found(opened):
    out = gql(opened, text("PullRequestClose"), {"input": {"pullRequestId": "bm9wZQ=="}})
    assert out["errors"][0]["type"] == "NOT_FOUND" and out["data"] == {"closePullRequest": None}


def test_a_reviewer_finds_the_pull_requests_waiting_for_it(opened):
    api(opened, "POST", f"{R}/pulls/1/requested_reviewers", {"reviewers": ["reviewer"]})
    q = text("PullRequestSearch")
    found = [gql_data(opened, q, {"q": f"repo:acme/app is:pr review-requested:{who}", "limit": 10}, as_user="reviewer")
             ["search"]["nodes"] for who in ("@me", "reviewer", "coder")]
    assert [[n["number"] for n in f] for f in found] == [[1], [1], []]
