import re

import pytest

from conftest import HOST, START_MS, agile, api, basic, call, local, search


def test_server_info_says_cloud_and_needs_no_login(jira):
    r = call(jira, "GET", "/rest/api/2/serverInfo", auth="")
    info = r.json()
    assert r.status == 200 and info["deploymentType"] == "Cloud" and info["versionNumbers"] == [1001, 0, 0]
    assert info["baseUrl"] == f"https://{HOST}"


def test_server_mode_says_server(server_jira):
    info = api(server_jira, "GET", "serverInfo")
    assert info["deploymentType"] == "Server" and info["versionNumbers"][0] == 10


def test_every_route_needs_the_token(jira):
    assert call(jira, "GET", "/rest/api/2/myself", auth=basic(token="wrong")).status == 401
    r = call(jira, "GET", "/rest/api/2/myself", auth="")
    assert r.status == 401 and r.json()["errorMessages"]
    assert call(jira, "GET", "/rest/api/2/myself", auth=f"Bearer {jira.token}").status == 200


def test_myself_is_the_agent_with_a_cloud_account_id(jira):
    me = api(jira, "GET", "myself", as_user="rev")
    assert re.fullmatch(r"[0-9a-f]{24}", me["accountId"]) and me["displayName"] == "Rev Two"
    assert me["self"] == f"https://{HOST}/rest/api/2/user?accountId={me['accountId']}"
    assert "name" not in me and me["emailAddress"] == "rev@acme.test"
    assert api(jira, "GET", "myself")["displayName"] == "Dev One"   # no agent: the default user


def test_a_new_agent_is_a_user_on_first_sight(jira):
    me = api(jira, "GET", "myself", as_user="newcomer")
    assert me["displayName"] == "newcomer"
    found = api(jira, "GET", "user/search?query=newc")
    assert [u["accountId"] for u in found] == [me["accountId"]]


def test_server_mode_users_have_names(server_jira):
    me = api(server_jira, "GET", "myself")
    assert me["name"] == "dev" and me["key"] == "dev" and "accountId" not in me
    assert me["self"] == f"https://{HOST}/rest/api/2/user?username=dev"
    assert api(server_jira, "GET", "user?username=rev")["displayName"] == "Rev Two"
    assert [u["name"] for u in api(server_jira, "GET", "user/search?username=re")] == ["rev"]


def test_create_read_update_delete(jira):
    made = api(jira, "POST", "issue", {"fields": {"project": {"key": "AC"}, "summary": "New thing",
                                                  "issuetype": {"name": "Task"}, "labels": ["x"],
                                                  "priority": {"name": "High"}, "description": "Body"}})
    assert made["key"] == "AC-5" and made["self"] == f"https://{HOST}/rest/api/2/issue/{made['id']}"
    issue = api(jira, "GET", "issue/AC-5")
    f = issue["fields"]
    assert (f["summary"], f["status"]["name"], f["issuetype"]["name"], f["priority"]["name"]) == \
        ("New thing", "To Do", "Task", "High")
    assert f["reporter"]["displayName"] == "Dev One" and f["assignee"] is None and f["description"] == "Body"
    assert f["created"].endswith("+0000") and f["resolution"] is None
    call(jira, "PUT", "/rest/api/2/issue/AC-5", {"fields": {"summary": "Renamed"},
                                                  "update": {"labels": [{"add": "y"}, {"remove": "x"}]}})
    f = api(jira, "GET", f"issue/{made['id']}")["fields"]
    assert f["summary"] == "Renamed" and f["labels"] == ["y"]
    assert call(jira, "DELETE", "/rest/api/2/issue/AC-5").status == 204
    r = call(jira, "GET", "/rest/api/2/issue/AC-5")
    assert r.status == 404
    assert r.json() == {"errorMessages": ["Issue does not exist or you do not have permission to see it."],
                        "errors": {}}


def test_create_says_what_is_missing(jira):
    r = call(jira, "POST", "/rest/api/2/issue", {"fields": {"project": {"key": "AC"}, "issuetype": {"name": "Task"}}})
    assert r.status == 400
    assert r.json() == {"errorMessages": [], "errors": {"summary": "You must specify a summary of the issue."}}


def test_an_unknown_field_cannot_be_set(jira):
    r = call(jira, "PUT", "/rest/api/2/issue/AC-1", {"fields": {"customfield_99999": 1}})
    assert r.status == 400 and "customfield_99999" in r.json()["errors"]


def test_transitions_follow_the_workflow(jira):
    names = [t["name"] for t in api(jira, "GET", "issue/AC-4/transitions")["transitions"]]
    assert names == ["To Do", "In Progress", "Done"]   # "Review" is only from In Progress
    r = call(jira, "POST", "/rest/api/2/issue/AC-4/transitions", {"transition": {"id": "31"}, "fields": {}})
    assert r.status == 400
    call(jira, "POST", "/rest/api/2/issue/AC-4/transitions", {"transition": {"id": "41"}, "fields": {}})
    f = api(jira, "GET", "issue/AC-4")["fields"]
    assert f["status"]["name"] == "Done" and f["resolution"]["name"] == "Done" and f["resolutiondate"]
    call(jira, "POST", "/rest/api/2/issue/AC-4/transitions", {"transition": {"id": "11"}})
    assert api(jira, "GET", "issue/AC-4")["fields"]["resolution"] is None


def test_a_transition_can_carry_a_comment(jira):
    call(jira, "POST", "/rest/api/2/issue/AC-4/transitions",
         {"transition": {"id": "21"}, "fields": {}, "update": {"comment": [{"add": {"body": "Starting"}}]}})
    assert [c["body"] for c in api(jira, "GET", "issue/AC-4/comment")["comments"]] == ["Starting"]


def test_comments_are_paged_and_editable(jira):
    c = api(jira, "POST", "issue/AC-1/comment", {"body": "first"}, as_user="rev")
    assert c["self"] == f"https://{HOST}/rest/api/2/issue/{c['self'].split('/')[-3]}/comment/{c['id']}"
    assert c["author"]["displayName"] == "Rev Two"
    api(jira, "POST", "issue/AC-1/comment", {"body": "second"})
    page = api(jira, "GET", "issue/AC-1/comment?maxResults=1&startAt=1")
    assert (page["total"], page["startAt"], [x["body"] for x in page["comments"]]) == (2, 1, ["second"])
    edited = api(jira, "PUT", f"issue/AC-1/comment/{c['id']}", {"body": "first, edited"})
    assert edited["body"] == "first, edited"
    assert api(jira, "GET", "issue/AC-1")["fields"]["comment"]["total"] == 2


def test_assign_by_account_id_and_unassign(jira):
    rev = api(jira, "GET", "user/search?query=rev")[0]["accountId"]
    assert call(jira, "PUT", "/rest/api/latest/issue/AC-4/assignee", {"accountId": rev}).status == 204
    assert api(jira, "GET", "issue/AC-4")["fields"]["assignee"]["accountId"] == rev
    call(jira, "PUT", "/rest/api/3/issue/AC-4/assignee", {"accountId": None})
    assert api(jira, "GET", "issue/AC-4")["fields"]["assignee"] is None


def test_assignment_and_mentions_and_transitions_are_notices(jira):
    jira.notices()
    rev = api(jira, "GET", "user/search?query=rev")[0]["accountId"]
    call(jira, "PUT", "/rest/api/2/issue/AC-4/assignee", {"accountId": rev})
    api(jira, "POST", "issue/AC-4/comment", {"body": f"[~accountid:{rev}] please look"})
    call(jira, "POST", "/rest/api/2/issue/AC-4/transitions", {"transition": {"id": "21"}})
    assert jira.notices() == [
        {"system": "jira", "kind": "issue_assigned", "data": {"key": "AC-4", "assignee": "rev"}},
        {"system": "jira", "kind": "mentioned", "data": {"key": "AC-4", "user": "rev",
                                                          "body": f"[~accountid:{rev}] please look"}},
        {"system": "jira", "kind": "issue_transitioned", "data": {"key": "AC-4", "from": "To Do",
                                                                   "to": "In Progress", "by": "dev"}}]


def test_metadata_lists(jira):
    assert [p["key"] for p in api(jira, "GET", "project")] == ["AC", "OPS"]
    project = api(jira, "GET", "project/AC?expand=issueTypes")
    assert {"Task", "Bug", "Story"} <= {t["name"] for t in project["issueTypes"]}
    assert [s["name"] for s in api(jira, "GET", "status")] == ["To Do", "In Progress", "In Review", "Done"]
    assert [p["name"] for p in api(jira, "GET", "priority")][:2] == ["Highest", "High"]
    assert {t["name"] for t in api(jira, "GET", "issuetype")} >= {"Task", "Bug"}
    fields = {f["name"]: f for f in api(jira, "GET", "field")}
    assert fields["Rank"]["schema"]["custom"] == "com.pyxis.greenhopper.jira:gh-lexo-rank"
    assert fields["Sprint"]["schema"]["customId"] == int(fields["Sprint"]["id"].split("_")[1])
    assert "Blocks" in [t["name"] for t in api(jira, "GET", "issueLinkType")["issueLinkTypes"]]


def test_createmeta_cloud(jira):
    cloud = api(jira, "GET", "issue/createmeta/AC/issuetypes")
    task = next(t for t in cloud["issueTypes"] if t["name"] == "Task")
    fields = api(jira, "GET", f"issue/createmeta/AC/issuetypes/{task['id']}")["fields"]
    assert {"summary", "issuetype", "project"} <= {f["fieldId"] for f in fields}


def test_createmeta_server(server_jira):
    types = api(server_jira, "GET", "issue/createmeta/AC/issuetypes")["values"]
    fields = api(server_jira, "GET", f"issue/createmeta/AC/issuetypes/{types[0]['id']}")["values"]
    assert "summary" in {f["fieldId"] for f in fields}


def test_issue_links(jira):
    r = call(jira, "POST", "/rest/api/2/issueLink",
             {"type": {"name": "Blocks"}, "inwardIssue": {"key": "AC-3"}, "outwardIssue": {"key": "AC-4"}})
    assert r.status == 201 and r.body == b""
    links = api(jira, "GET", "issue/AC-4")["fields"]["issuelinks"]
    assert [(lnk["type"]["name"], lnk["inwardIssue"]["key"]) for lnk in links] == [("Blocks", "AC-3")]
    assert api(jira, "GET", "issue/AC-3")["fields"]["issuelinks"][0]["outwardIssue"]["key"] == "AC-4"


def test_v3_comment_is_adf_and_v2_is_text(jira):
    adf = {"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "hi"}]}]}
    made = api(jira, "POST", "issue/AC-1/comment", {"body": adf}, v="3")
    assert made["body"] == adf
    assert api(jira, "GET", f"issue/AC-1/comment/{made['id']}")["body"] == "hi"
    api(jira, "POST", "issue/AC-1/comment", {"body": "line one\nline two"})
    v3 = api(jira, "GET", "issue/AC-1/comment", v="3")["comments"][1]["body"]
    assert v3["content"][0]["content"][1] == {"type": "hardBreak"}
    r = call(jira, "POST", "/rest/api/3/issue/AC-1/comment", {"body": "plain"})
    assert r.status == 400 and r.json()["errors"] == {"comment": "Comment body is not valid!"}
    desc = api(jira, "GET", "issue/AC-1", v="3")["fields"]["description"]
    assert desc["type"] == "doc" and api(jira, "GET", "issue/AC-1")["fields"]["description"] == "Users sign in with email."


def _selfs(node, out):
    if isinstance(node, dict):
        if isinstance(node.get("self"), str):
            out.append(node["self"])
        for v in node.values():
            _selfs(v, out)
    elif isinstance(node, list):
        for v in node:
            _selfs(v, out)
    return out


@pytest.mark.parametrize("v", ["2", "3"])
def test_self_urls_dereference(jira, v):
    api(jira, "POST", "issue/AC-1/comment", {"body": "a comment"})
    call(jira, "POST", "/rest/api/2/issueLink", {"type": {"name": "Blocks"}, "inwardIssue": {"key": "AC-3"},
                                                 "outwardIssue": {"key": "AC-1"}})
    issue = api(jira, "GET", "issue/AC-1", v=v)
    selfs = _selfs(issue, [])
    assert len(selfs) >= 12
    for url in selfs:
        r = call(jira, "GET", local(jira, url))
        assert r.status == 200 and r.json(), url
    assert call(jira, "PUT", local(jira, issue["self"]), {"fields": {"summary": "Via self"}}).status == 204
    comment = issue["fields"]["comment"]["comments"][0]
    body = "via self" if v == "2" else {"type": "doc", "version": 1, "content": []}
    assert call(jira, "PUT", local(jira, comment["self"]), {"body": body}).status == 200
    assert api(jira, "GET", "issue/AC-1")["fields"]["summary"] == "Via self"


def test_search_jql_pages_by_token_and_the_last_page_has_none(jira_150_issues):
    j = jira_150_issues
    first = api(j, "GET", "search/jql?jql=project%20%3D%20OPS&maxResults=200&fields=summary")
    assert len(first["issues"]) == 100 and first["isLast"] is False and first["nextPageToken"]
    second = api(j, "POST", "search/jql", {"jql": "project = OPS", "nextPageToken": first["nextPageToken"],
                                           "maxResults": 100, "fields": ["summary"]})
    assert len(second["issues"]) == 50 and second["isLast"] is True and "nextPageToken" not in second
    keys = [i["key"] for i in first["issues"] + second["issues"]]
    assert len(set(keys)) == 150


def test_search_jql_answers_ids_unless_fields_are_asked(jira):
    issues = api(jira, "GET", "search/jql?jql=key%20%3D%20AC-1")["issues"]
    assert list(issues[0]) == ["id"]


def test_cloud_has_no_offset_search(jira):
    r = call(jira, "GET", "/rest/api/2/search?jql=project%20%3D%20AC")
    assert r.status == 410 and "search/jql" in r.json()["errorMessages"][0]


def test_server_search_is_offset_with_total(server_jira):
    page = api(server_jira, "GET", "search?jql=project%20%3D%20AC&startAt=1&maxResults=2&validateQuery=True")
    assert (page["total"], page["startAt"], page["maxResults"]) == (4, 1, 2)
    assert [i["key"] for i in page["issues"]] == ["AC-2", "AC-3"]


def test_boards_and_sprints(jira):
    boards = agile(jira, "GET", "board?projectKeyOrId=AC")
    assert boards["values"][0]["location"]["projectKey"] == "AC" and boards["isLast"] is True
    sprints = agile(jira, "GET", "board/1/sprint?state=active,future")["values"]
    assert [(s["name"], s["state"]) for s in sprints] == [("AC Sprint 1", "active"), ("AC Sprint 2", "future")]
    assert sprints[0]["self"] == f"https://{HOST}/rest/agile/1.0/sprint/1"
    made = agile(jira, "POST", "sprint", {"name": "AC Sprint 3", "originBoardId": 1})
    assert made["state"] == "future"
    assert call(jira, "POST", "/rest/agile/1.0/sprint/2/issue", {"issues": ["AC-4"]}).status == 204
    assert [i["key"] for i in agile(jira, "GET", "sprint/2/issue")["issues"]] == ["AC-4"]
    assert search(jira, "sprint = 2")[0]["key"] == "AC-4"


def test_a_sprint_starts_and_closes(jira):
    r = call(jira, "PUT", "/rest/agile/1.0/sprint/2", {"state": "active"})
    assert r.status == 400   # starting needs dates
    started = agile(jira, "PUT", "sprint/2", {"state": "active", "startDate": "2026-03-10T00:00:00.000Z",
                                              "endDate": "2026-03-24T00:00:00.000Z"})
    assert started["state"] == "active" and started["startDate"].startswith("2026-03-10")
    closed = agile(jira, "PUT", "sprint/1", {"state": "closed"})
    assert closed["state"] == "closed" and closed["completeDate"]
    assert [i["key"] for i in search(jira, "sprint is EMPTY ORDER BY key")] == ["AC-1", "AC-3", "AC-4"]


def test_backlog_board_issues_and_rank(jira):
    assert [i["key"] for i in agile(jira, "GET", "board/1/backlog")["issues"]] == ["AC-4"]
    assert agile(jira, "GET", "board/1/issue?maxResults=2")["total"] == 4
    assert call(jira, "POST", "/rest/agile/1.0/backlog/issue", {"issues": ["AC-1"]}).status == 204
    assert [i["key"] for i in agile(jira, "GET", "board/1/backlog")["issues"]] == ["AC-1", "AC-4"]
    assert call(jira, "PUT", "/rest/agile/1.0/issue/rank", {"issues": ["AC-4"], "rankBeforeIssue": "AC-3"}).status == 204
    assert [i["key"] for i in search(jira, "ORDER BY rank")] == ["AC-4", "AC-3", "AC-1", "AC-2"]


def test_a_read_only_agent_reads_and_searches_but_cannot_write(jira):
    ro = {"X-Arena-Readonly": "1"}
    assert jira.part.http_routes("i")["routes"][0]["honours_readonly"] is True
    read = call(jira, "GET", "/rest/api/2/issue/AC-1", as_user="auditor", headers=ro)
    assert read.status == 200 and read.headers["x-arena-effect"] == "read"
    assert call(jira, "POST", "/rest/api/2/search/jql", {"jql": "project = AC"}, headers=ro).status == 200
    r = call(jira, "POST", "/rest/api/2/issue/AC-1/comment", {"body": "sneaky"}, as_user="auditor", headers=ro)
    assert r.status == 403 and r.headers["x-arena-effect"] == "read" and r.json()["errorMessages"]
    assert api(jira, "GET", "issue/AC-1/comment")["total"] == 0
    assert jira.part.advance("i", 0)["changed"] is False


def test_unknown_routes_and_versions_are_404(jira):
    assert call(jira, "GET", "/rest/api/9/myself").status == 404
    assert call(jira, "GET", "/rest/api/2/nothing-here").status == 404


def test_times_are_world_time(jira):
    made = api(jira, "POST", "issue", {"fields": {"project": {"key": "AC"}, "summary": "S", "issuetype": {"name": "Task"}}})
    assert api(jira, "GET", f"issue/{made['key']}")["fields"]["created"] == "2026-03-10T06:00:00.000+0000"
    assert START_MS == 1_773_122_400_000


def test_docs_and_save_restore(jira, tmp_path):
    call(jira, "POST", "/rest/api/2/issue/AC-4/transitions", {"transition": {"id": "21"}})
    docs = jira.docs()
    assert docs["issue/AC-4"] == {"status": "In Progress", "status_category": "indeterminate", "sprint": None,
                                  "assignee": "dev", "resolution": None, "summary": "Tidy the readme",
                                  "type": "Task", "priority": "Low", "labels": []}
    assert docs["issue/AC-2"]["status_category"] == "done" and docs["issue/AC-2"]["resolution"] == "Done"
    assert docs["sprint/1"] == {"name": "AC Sprint 1", "state": "active", "issues": ["AC-1", "AC-2", "AC-3"]}
    jira.part.save("i", "s1", tmp_path / "saves" / "jira-twin" / "s1")
    call(jira, "DELETE", "/rest/api/2/issue/AC-4")
    assert jira.part.inspect({"save": "s1"}, ["issue/AC-4"])["docs"]["issue/AC-4"]["status"] == "In Progress"
    jira.part.start("j", jira.part.ctx("i").config, 1, 0, {"save": "s1"})
    try:
        assert jira.part.inspect({"instance": "j"}, ["issue/AC-4"])["docs"]["issue/AC-4"]["status"] == "In Progress"
    finally:
        jira.part.stop("j")
