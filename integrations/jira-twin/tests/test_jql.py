import pytest

from shakedown_part.world_time import WorldTime
from conftest import DAY_MS, START_MS, api, call, search, search_error
from jira_twin import jql


@pytest.mark.parametrize("text,keys", [
    ("project = AC AND sprint in openSprints() AND statusCategory != Done ORDER BY rank", ["AC-3", "AC-1"]),
    ('assignee = currentUser() AND updated >= -7d', ["AC-1"]),
    ('labels in (backend, api) OR summary ~ "checkout"', ["AC-2", "AC-3"]),
    ("resolution = Unresolved AND priority >= High", ["AC-3"]),
])
def test_jql_subset(jira, text, keys):
    assert [i["key"] for i in search(jira, text)] == keys


@pytest.mark.parametrize("text,keys", [
    ("key = AC-2", ["AC-2"]),
    ("issuekey in (AC-1, AC-4)", ["AC-1", "AC-4"]),
    ('status = "In Progress"', ["AC-1"]),
    ("status in ('To Do', Done) ORDER BY key DESC", ["AC-4", "AC-3", "AC-2"]),
    ("statusCategory = indeterminate", ["AC-1"]),
    ("type = Bug AND NOT assignee = dev", ["AC-2", "AC-3"]),
    ("issuetype != Bug", ["AC-1", "AC-4"]),
    ("sprint is EMPTY", ["AC-4"]),
    ("sprint = 1 AND labels is not EMPTY", ["AC-2"]),
    ('sprint = "AC Sprint 1" AND resolution is EMPTY ORDER BY priority DESC', ["AC-3", "AC-1"]),
    ("reporter = lead ORDER BY created ASC", ["AC-1", "AC-3"]),
    ("created < -15d ORDER BY created DESC, key ASC", ["AC-1", "AC-2", "AC-4"]),
    ("updated <= startOfDay(-25) OR text ~ email", ["AC-1", "AC-4"]),
    ("summary !~ fix and project = ac", ["AC-1", "AC-3", "AC-4"]),
    ("assignee in (rev) AND (priority = Highest OR priority = Low)", ["AC-3"]),
    ("cf[10020] in futureSprints()", []),
    ("", ["AC-1", "AC-2", "AC-3", "AC-4"]),
    ("ORDER BY rank ASC", ["AC-3", "AC-1", "AC-2", "AC-4"]),
])
def test_jql_more(jira, text, keys):
    assert [i["key"] for i in search(jira, text)] == keys


def test_jql_error_wording(jira):
    e = search_error(jira, "project = NOPE")
    assert e == {"errorMessages": ["The value 'NOPE' does not exist for the field 'project'."], "errors": {}}


@pytest.mark.parametrize("text,message", [
    ("nosuchfield = 1", "Field 'nosuchfield' does not exist or you do not have permission to view it."),
    ("status = Nope", "The value 'Nope' does not exist for the field 'status'."),
    ("sprint in nosuch()", "Unable to find JQL function 'nosuch()'."),
])
def test_jql_value_errors(jira, text, message):
    assert search_error(jira, text) == {"errorMessages": [message], "errors": {}}


@pytest.mark.parametrize("text", ["project =", "project = AC AND", "(status = Done", "status in (Done", "= AC"])
def test_jql_syntax_errors_say_error_in_the_query(jira, text):
    (message,) = search_error(jira, text)["errorMessages"]
    assert message.startswith("Error in the JQL Query: ")


def test_parse_answers_a_query_or_an_error():
    q = jql.parse("project = AC ORDER BY rank DESC")
    assert not isinstance(q, jql.JqlError) and q.order == [("rank", "desc")]
    assert isinstance(jql.parse("project = AC AND"), jql.JqlError)


def test_relative_dates_are_world_time(jira):
    world = WorldTime(START_MS + 10 * DAY_MS, 1.0)   # the branch's world, ten days after the seed
    world.observe(0)
    jira.part._world_times = {"i": world}
    assert [i["key"] for i in search(jira, "updated >= -7d")] == []


def test_post_search_is_a_read(jira):
    r = call(jira, "POST", "/rest/api/2/search/jql", {"jql": "key = AC-1", "fields": ["summary"], "expand": None})
    assert r.status == 200 and r.headers["x-arena-effect"] == "read"
    assert r.json()["issues"][0]["fields"] == {"summary": "Add the login form"}
    assert jira.part.advance("i", 0)["changed"] is False


def test_approximate_count(jira):
    assert api(jira, "POST", "search/approximate-count", {"jql": "project = AC AND type = Bug"}) == {"count": 2}
