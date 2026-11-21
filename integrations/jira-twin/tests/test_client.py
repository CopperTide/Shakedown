"""The pycontribs `jira` client, unchanged, against the twin: the client talks to the public host, and a
transport stands in for shaked's proxy (it forwards to the twin with X-Forwarded-* and the agent)."""
from urllib.parse import urlsplit, urlunsplit

import pytest
import requests.adapters
from jira import JIRA

import jira_flow
from conftest import HOST, api

_send = requests.adapters.HTTPAdapter.send


@pytest.fixture
def proxied(jira, monkeypatch):
    up = urlsplit(jira.up)

    def send(self, request, **kw):
        url = urlsplit(request.url)
        if url.netloc == HOST:
            request.url = urlunsplit((up.scheme, up.netloc, url.path, url.query, ""))
            request.headers.update({"X-Forwarded-Proto": url.scheme, "X-Forwarded-Host": HOST, "X-Shakedown-Agent": "dev"})
        return _send(self, request, **kw)

    monkeypatch.setattr(requests.adapters.HTTPAdapter, "send", send)
    return jira


def test_the_jira_client_works_unchanged(proxied):
    env = proxied.part.http_routes("i")["routes"][0]["env"]
    key, status = jira_flow.run(env["JIRA_SERVER"], env["JIRA_EMAIL"], env["JIRA_API_TOKEN"])
    assert status == "In Progress"
    assert proxied.docs()[f"issue/{key}"] == {**proxied.docs()[f"issue/{key}"], "status": "In Progress", "sprint": 1}
    assert [c["body"] for c in api(proxied, "GET", f"issue/{key}/comment")["comments"]] == ["on it"]


def test_the_jira_client_searches_assigns_and_updates(proxied):
    j = JIRA(server=f"https://{HOST}", basic_auth=("dev@acme.test", proxied.token))
    assert j.myself()["displayName"] == "Dev One"
    found = j.search_issues("project = AC AND statusCategory != Done ORDER BY rank", maxResults=False)
    assert [i.key for i in found] == ["AC-3", "AC-1", "AC-4"]
    rev = j.search_users(query="rev")[0]
    j.assign_issue("AC-4", rev.accountId)
    issue = j.issue("AC-4")
    issue.update(summary="Tidy the readme again", fields={"labels": ["docs"]})
    comment = j.add_comment(issue, "first")
    comment.update(body="first, edited")
    assert [c.body for c in j.comments("AC-4")] == ["first, edited"]
    assert (j.issue("AC-4").fields.assignee.displayName, j.issue("AC-4").fields.labels) == ("Rev Two", ["docs"])
    assert [b.name for b in j.boards()] == ["AC board"] and j.sprint(1).state == "active"
    assert {p.key for p in j.projects()} == {"AC", "OPS"} and j.fields()
    j.create_issue_link("Blocks", "AC-3", "AC-4")
    assert j.issue("AC-4").fields.issuelinks[0].type.name == "Blocks"
