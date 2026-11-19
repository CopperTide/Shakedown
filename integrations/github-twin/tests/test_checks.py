import pytest

from conftest import advance_events, api, api_raw

R = "/api/v3/repos/acme/app"


@pytest.fixture
def twin_with_check(twin_with_pr):
    head = api(twin_with_pr, "GET", f"{R}/pulls/1")["head"]["sha"]
    r = api_raw(twin_with_pr, "POST", f"{R}/check-runs", {"name": "CI / test", "head_sha": head, "status": "in_progress"})
    assert r.status == 201
    twin_with_pr.head, twin_with_pr.run = head, r.json()
    return twin_with_pr


def test_check_run_lists_have_one_other_key(twin_with_check):
    r = api(twin_with_check, "GET", f"{R}/commits/{twin_with_check.head}/check-runs")
    assert set(r) == {"total_count", "check_runs"} and r["total_count"] == 1
    suite = twin_with_check.run["check_suite"]["id"]
    assert api(twin_with_check, "GET", f"{R}/check-suites/{suite}/check-runs")["total_count"] == 1


def test_completing_a_check_notifies_and_shows_in_docs(twin_with_check):
    t = twin_with_check
    r = api(t, "PATCH", f"{R}/check-runs/{t.run['id']}", {"conclusion": "failure", "output": {"title": "1 failed", "summary": "test_add"}})
    assert (r["status"], r["conclusion"], r["output"]["title"]) == ("completed", "failure", "1 failed")
    assert t.docs()[f"check/acme/app/{t.head}"] == {"conclusion": "failure", "runs": [
        {"name": "CI / test", "status": "completed", "conclusion": "failure"}]}
    [n] = advance_events(t)
    assert n["data"]["kind"] == "checks_completed"
    assert n["data"]["data"] == {"repo": "acme/app", "sha": t.head, "conclusion": "failure", "prs": [1]}


def test_the_review_request_tells_the_checks(twin_with_check):
    api(twin_with_check, "PATCH", f"{R}/check-runs/{twin_with_check.run['id']}", {"conclusion": "success"})
    advance_events(twin_with_check)
    api(twin_with_check, "POST", f"{R}/pulls/1/requested_reviewers", {"reviewers": ["reviewer"]})
    [n] = advance_events(twin_with_check)
    assert n["data"]["data"]["checks"] == "success"


def test_statuses_combine_by_context(twin_with_pr):
    head = api(twin_with_pr, "GET", f"{R}/pulls/1")["head"]["sha"]
    assert api_raw(twin_with_pr, "POST", f"{R}/statuses/{head}", {"state": "pending", "context": "ci"}).status == 201
    api(twin_with_pr, "POST", f"{R}/statuses/{head}", {"state": "success", "context": "ci"})
    api(twin_with_pr, "POST", f"{R}/statuses/{head}", {"state": "failure", "context": "lint", "description": "E501"})
    combined = api(twin_with_pr, "GET", f"{R}/commits/add-b/status")
    assert (combined["state"], combined["total_count"]) == ("failure", 2)
    assert combined["url"] == f"https://github.acme.test/api/v3/repos/acme/app/commits/{head}/status"
    assert [s["state"] for s in api(twin_with_pr, "GET", f"{R}/commits/{head}/statuses")] == ["failure", "success", "pending"]
    assert api(twin_with_pr, "GET", f"{R}/pulls/1")["mergeable_state"] == "unstable"
