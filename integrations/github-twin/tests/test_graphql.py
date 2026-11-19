"""The GraphQL subset gh needs, over the pruned GHES schema: GitHub's own schema as the reference,
gh's queries, graphql-ruby error words, reads and writes, and the read-only rule."""
from pathlib import Path

import pytest
from graphql import parse, validate
from graphql.language import EnumTypeDefinitionNode, print_ast

from conftest import api, branch_from_main, create_pr, gql, gql_data, gql_raw
from github_twin.graphql import executor

TESTS = Path(__file__).parent
EXTRACT = TESTS / "schemas" / "ghes-3.20-graphql-subset.graphql"
GH_QUERIES = sorted((TESTS / "gh_queries").rglob("*.graphql"))
INTROSPECTION = sorted((TESTS / "gh_queries").glob("*.graphql"))
R = "/api/v3/repos/acme/app"
OPTIONAL = {"isInMergeQueue", "isMergeQueueEnabled", "checkRunCount", "blockedBy", "pullRequestTemplates",
            "autoMergeAllowed", "event"}


def sdl(path: Path) -> dict:
    """{type: {field or enum value: its signature}} of an SDL file."""
    out = {}
    for d in parse(path.read_text()).definitions:
        if isinstance(d, EnumTypeDefinitionNode):
            out[d.name.value] = {v.name.value: "" for v in d.values}
        elif getattr(d, "fields", None) is not None:
            out[d.name.value] = {f.name.value: _sig(f) for f in d.fields}
        elif getattr(d, "types", None) is not None:
            out[d.name.value] = {t.name.value: "" for t in d.types}
        else:
            out[d.name.value] = {}
    return out


def _sig(f) -> str:
    args = ", ".join(f"{a.name.value}: {print_ast(a.type)}" for a in getattr(f, "arguments", None) or ())
    return f"({args}): {print_ast(f.type)}"


def repo_id(twin):
    return gql_data(twin, '{ repository(owner: "acme", name: "app") { id } }')["repository"]["id"]


def test_the_pruned_schema_is_a_subset_of_ghes():
    full, ours = sdl(EXTRACT), sdl(executor.SCHEMA_FILE)
    assert len(ours) > 250
    for t, fields in ours.items():
        assert t in full, t
        assert all(full[t].get(f) == sig for f, sig in fields.items()), t


@pytest.mark.parametrize("q", GH_QUERIES, ids=lambda p: p.stem)
def test_every_gh_query_validates(q):
    assert validate(executor.schema(), parse(q.read_text()), executor.RULES) == []


def test_an_unknown_field_uses_graphql_ruby_wording(twin):
    r = gql(twin, "{ viewer { bogus } }")
    assert r["errors"][0]["message"] == "Field 'bogus' doesn't exist on type 'User'"
    assert "data" not in r


def test_other_validation_errors_use_graphql_ruby_wording(twin):
    words = [gql(twin, q)["errors"][0]["message"] for q in (
        '{ repository(owner: "acme") { id } }', '{ viewer { login(x: 1) } }', "{ viewer { ...on Nope { id } } }")]
    assert words == ["Field 'repository' is missing required arguments: name",
                     "Field 'login' doesn't accept argument 'x'", "No such type Nope, so it can't be a fragment condition"]


def test_a_missing_repository_is_not_found(twin):
    r = gql(twin, '{ repository(owner: "acme", name: "nope") { id } }')
    assert r["data"] == {"repository": None}
    assert r["errors"] == [{"type": "NOT_FOUND", "path": ["repository"], "locations": [{"line": 1, "column": 3}],
                            "message": "Could not resolve to a Repository with the name 'acme/nope'."}]


def test_the_viewer_is_the_agent(twin):
    assert gql_data(twin, "query UserCurrent{viewer{login}}", as_user="reviewer") == {"viewer": {"login": "reviewer"}}


def test_a_request_without_the_token_is_refused(twin):
    r = gql_raw(twin, "{ viewer { login } }", token="wrong")
    assert (r.status, r.json()["message"]) == (401, "Bad credentials")


def test_introspection_advertises_no_optional_field(twin):
    names = set()
    for q in INTROSPECTION:
        for found in gql_data(twin, q.read_text()).values():
            names |= {f["name"] for f in (found.get("fields") or found.get("enumValues") or [])}
    assert {"number", "title", "statusCheckRollup", "ISSUE"} <= names
    assert not names & OPTIONAL and "ISSUE_ADVANCED" not in names


CREATE_PR = """mutation PullRequestCreate($input: CreatePullRequestInput!) {
  createPullRequest(input: $input) { pullRequest { id url number } } }"""


def test_queries_are_reads_and_mutations_writes(with_clone):
    branch_from_main(with_clone, "f", "f.txt", "f\n")
    assert gql_raw(with_clone, "{ viewer { login } }").headers["x-arena-effect"] == "read"
    vars_ = {"input": {"repositoryId": repo_id(with_clone), "baseRefName": "main", "headRefName": "f", "title": "F"}}
    assert gql_raw(with_clone, CREATE_PR, vars_).headers["x-arena-effect"] == "write"


def test_create_pull_request_returns_its_url(with_clone):
    branch_from_main(with_clone, "f", "f.txt", "f\n")
    vars_ = {"input": {"repositoryId": repo_id(with_clone), "baseRefName": "main", "headRefName": "f",
                       "title": "F", "body": "B", "draft": False, "maintainerCanModify": True}}
    pr = gql_data(with_clone, CREATE_PR, vars_)["createPullRequest"]["pullRequest"]
    assert (pr["number"], pr["url"]) == (1, "https://github.acme.test/acme/app/pull/1")
    assert api(with_clone, "GET", f"{R}/pulls/1")["node_id"] == pr["id"]


def test_a_refused_mutation_changes_nothing(with_clone):
    vars_ = {"input": {"repositoryId": repo_id(with_clone), "baseRefName": "main", "headRefName": "nope", "title": "F"}}
    r = gql_raw(with_clone, CREATE_PR, vars_)
    assert r.headers["x-arena-effect"] == "read"
    out = r.json()
    assert out["data"] == {"createPullRequest": None} and out["errors"][0]["type"] == "UNPROCESSABLE"
    assert api(with_clone, "GET", f"{R}/pulls?state=all") == []


def test_a_read_only_agent_queries_but_its_mutations_are_refused(with_clone):
    branch_from_main(with_clone, "f", "f.txt", "f\n")
    vars_ = {"input": {"repositoryId": repo_id(with_clone), "baseRefName": "main", "headRefName": "f", "title": "F"}}
    with_clone.part.advance("i", 50)
    ro = {"X-Arena-Readonly": "1"}
    r = gql_raw(with_clone, '{ repository(owner: "acme", name: "app") { id } }', as_user="auditor", headers=ro)
    assert r.headers["x-arena-effect"] == "read" and "errors" not in r.json()
    r = gql_raw(with_clone, CREATE_PR, vars_, as_user="auditor", headers=ro)
    assert r.headers["x-arena-effect"] == "read"
    assert r.json()["errors"][0] | {"locations": None} == {
        "type": "FORBIDDEN", "path": ["createPullRequest"], "locations": None,
        "message": "Resource not accessible by integration"}
    assert api(with_clone, "GET", f"{R}/pulls?state=all") == []
    assert with_clone.part.advance("i", 100)["changed"] is False


ROLLUP = """query($n: Int!) { repository(owner: "acme", name: "app") { pullRequest(number: $n) {
  statusCheckRollup { state contexts(first: 10) { totalCount nodes { __typename
    ...on CheckRun { name status conclusion } ...on StatusContext { context state } } } } } } }"""


def test_the_status_rollup_unions_check_runs_and_statuses(with_clone):
    sha = branch_from_main(with_clone, "f", "f.txt", "f\n")
    create_pr(with_clone, "f")
    assert gql_data(with_clone, ROLLUP, {"n": 1})["repository"]["pullRequest"]["statusCheckRollup"] is None
    api(with_clone, "POST", f"{R}/check-runs", {"name": "build", "head_sha": sha, "status": "in_progress"})
    api(with_clone, "POST", f"{R}/statuses/{sha}", {"state": "failure", "context": "ci/lint"})
    rollup = gql_data(with_clone, ROLLUP, {"n": 1})["repository"]["pullRequest"]["statusCheckRollup"]
    assert rollup["state"] == "FAILURE" and rollup["contexts"]["totalCount"] == 2
    assert rollup["contexts"]["nodes"] == [
        {"__typename": "CheckRun", "name": "build", "status": "IN_PROGRESS", "conclusion": None},
        {"__typename": "StatusContext", "context": "ci/lint", "state": "FAILURE"}]


def test_review_decision_is_null_unless_reviews_are_required(tmp_path):
    from conftest import config, git, started
    t = started(config(protection={"app": {"required_reviews": 1}}))
    try:
        git(tmp_path, "clone", "-q", t.git_url("acme", "app"), "app", creds=t.token)
        t.clone = tmp_path / "app"
        branch_from_main(t, "f", "f.txt", "f\n")
        create_pr(t, "f")
        q = '{ repository(owner: "acme", name: "app") { pullRequest(number: 1) { reviewDecision } } }'
        decision = lambda: gql_data(t, q)["repository"]["pullRequest"]["reviewDecision"]  # noqa: E731
        assert decision() == "REVIEW_REQUIRED"
        api(t, "POST", f"{R}/pulls/1/reviews", {"event": "APPROVE"}, as_user="reviewer")
        assert decision() == "APPROVED"
    finally:
        t.part.stop("i")
