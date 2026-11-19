from conftest import api


def test_create_transition_comment_assign(jira):
    made = jira.act("lead", "create_issue", {"project": "AC", "summary": "From a person", "type": "Bug",
                                             "assignee": "dev", "description": "Seen in prod"})
    assert made["result"] == {"key": "AC-5"} and made["writes"] == ["issue/AC-5"]
    assert jira.act("dev", "transition", {"key": "AC-5", "to": "In Progress"})["writes"] == ["issue/AC-5"]
    jira.act("dev", "comment", {"key": "AC-5", "body": "Looking, [~rev]"})
    jira.act("dev", "assign", {"key": "AC-5", "to": "rev"})
    f = api(jira, "GET", "issue/AC-5")["fields"]
    assert (f["status"]["name"], f["assignee"]["displayName"], f["reporter"]["displayName"]) == \
        ("In Progress", "Rev Two", "Lead Three")
    assert f["comment"]["comments"][0]["author"]["displayName"] == "Dev One"
    kinds = [(n["kind"], n["data"].get("assignee") or n["data"].get("user") or n["data"].get("to")) for n in jira.notices()]
    assert kinds == [("issue_assigned", "dev"), ("issue_transitioned", "In Progress"), ("mentioned", "rev"),
                     ("issue_assigned", "rev")]


def test_transition_by_status_name_when_no_transition_is_named(jira):
    assert jira.act("dev", "transition", {"key": "AC-1", "to": "In Review"})["result"] == {"key": "AC-1", "status": "In Review"}


def test_a_refused_act_changes_nothing(jira):
    assert jira.act("dev", "transition", {"key": "AC-4", "to": "In Review"})["error"]   # not from To Do
    assert jira.act("dev", "comment", {"key": "AC-99", "body": "x"})["error"]
    assert jira.act("dev", "levitate", {})["error"] == "unknown action levitate"
    assert jira.notices() == [] and jira.part.advance("i", 0)["changed"] is False
