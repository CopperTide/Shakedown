from conftest import (advance_events, api, api_raw, branch_from_main, commit, config, create_pr, git, push,
                      started)

R = "/api/v3/repos/acme/app"


def merge(t, n, as_user, method="merge"):
    return api(t, "PUT", f"{R}/pulls/{n}/merge", {"merge_method": method}, as_user=as_user)


def approve(t, n, as_user):
    return api(t, "POST", f"{R}/pulls/{n}/reviews", {"event": "APPROVE", "body": "Looks good"}, as_user=as_user)


def request_review(t, n, reviewers, as_user="coder"):
    return api(t, "POST", f"{R}/pulls/{n}/requested_reviewers", {"reviewers": reviewers}, as_user=as_user)


def push_more(t, as_user):
    git(t.clone, "checkout", "-q", "add-b")
    git(t.clone, "pull", "-q", "origin", "add-b", creds=t.token)
    commit(t.clone, "c.txt", "three\n")
    push(t.clone, t.token, "HEAD:refs/heads/add-b")
    git(t.clone, "checkout", "-q", "main")


def test_a_pr_from_a_pushed_branch_merges_three_ways(with_clone):
    for method in ("merge", "squash", "rebase"):
        branch_from_main(with_clone, f"f-{method}", f"{method}.txt", f"{method}\n")
        n = create_pr(with_clone, head=f"f-{method}")
        r = api(with_clone, "PUT", f"{R}/pulls/{n}/merge", {"merge_method": method}, as_user="lead")
        assert r["merged"] is True and len(r["sha"]) == 40 and r["message"] == "Pull Request successfully merged"
        assert api(with_clone, "GET", f"{R}/branches/main")["commit"]["sha"] == r["sha"]
        assert api_raw(with_clone, "GET", f"{R}/pulls/{n}/merge").status == 204
    parents = [len(api(with_clone, "GET", f"{R}/commits/{c['sha']}")["parents"])
               for c in api(with_clone, "GET", f"{R}/commits?per_page=3")]
    assert parents == [1, 1, 2]   # rebase, squash, merge: newest first
    files = {e["name"] for e in api(with_clone, "GET", f"{R}/contents/")}
    assert {"merge.txt", "squash.txt", "rebase.txt"} <= files


def test_a_pr_is_listed_read_and_edited(twin_with_pr):
    t = twin_with_pr
    pr = api(t, "GET", f"{R}/pulls/1")
    assert (pr["state"], pr["user"]["login"], pr["head"]["ref"], pr["base"]["ref"]) == ("open", "coder", "add-b", "main")
    assert (pr["mergeable"], pr["mergeable_state"], pr["commits"], pr["changed_files"]) == (True, "clean", 1, 1)
    assert pr["url"] == "https://github.acme.test/api/v3/repos/acme/app/pulls/1"
    assert [p["number"] for p in api(t, "GET", f"{R}/pulls?state=open&head=acme:add-b")] == [1]
    assert api(t, "GET", f"{R}/pulls?state=closed") == []
    assert api(t, "PATCH", f"{R}/pulls/1", {"title": "Add b.txt"})["title"] == "Add b.txt"
    assert [f["filename"] for f in api(t, "GET", f"{R}/pulls/1/files")] == ["b.txt"]
    assert [c["commit"]["message"] for c in api(t, "GET", f"{R}/pulls/1/commits")] == ["Add b"]
    assert api(t, "GET", f"{R}/issues/1")["pull_request"]["url"] == pr["url"]


def test_the_diff_media_type_answers_the_raw_diff(twin_with_pr):
    r = api_raw(twin_with_pr, "GET", f"{R}/pulls/1", headers={"Accept": "application/vnd.github.v3.diff"})
    assert r.status == 200 and r.body.decode().startswith("diff --git a/b.txt b/b.txt")
    assert "+two" in r.body.decode()


def test_a_second_pr_for_the_same_branch_is_422(twin_with_pr):
    r = api_raw(twin_with_pr, "POST", f"{R}/pulls", {"title": "Again", "head": "add-b", "base": "main"})
    assert r.status == 422 and r.json()["errors"][0]["message"].startswith("A pull request already exists for acme:add-b")


def test_mergeable_is_conflicting_when_branches_clash(with_clone):
    t = with_clone
    branch_from_main(t, "clash", "app.py", "def add(a, b):\n    return b + a\n")
    commit(t.clone, "app.py", "def add(a, b):\n    return a - b\n")
    push(t.clone, t.token, "HEAD:main")
    create_pr(t, "clash")
    pr = api(t, "GET", f"{R}/pulls/1")
    assert (pr["mergeable"], pr["mergeable_state"]) == (False, "dirty")
    r = api_raw(t, "PUT", f"{R}/pulls/1/merge", {})
    assert (r.status, r.json()["message"]) == (405, "Pull Request is not mergeable")


def test_a_stale_head_sha_is_409(twin_with_pr):
    r = api_raw(twin_with_pr, "PUT", f"{R}/pulls/1/merge", {"sha": "0" * 40})
    assert (r.status, r.json()["message"]) == (409, "Head branch was modified. Review and try the merge again.")


def test_cannot_approve_own_pr(twin_with_pr):   # Review Focus 5
    e = api_raw(twin_with_pr, "POST", f"{R}/pulls/1/reviews", {"event": "APPROVE"}, as_user="coder")
    assert (e.status, e.json()["message"]) == (422, "Can not approve your own pull request")
    e = api_raw(twin_with_pr, "POST", f"{R}/pulls/1/reviews", {"event": "REQUEST_CHANGES", "body": "x"}, as_user="coder")
    assert (e.status, e.json()["message"]) == (422, "Can not request changes on your own pull request")


def test_the_reviewer_can_approve_and_the_review_is_on_the_head(twin_with_pr):
    head = api(twin_with_pr, "GET", f"{R}/pulls/1")["head"]["sha"]
    r = approve(twin_with_pr, 1, as_user="reviewer")
    assert (r["state"], r["commit_id"], r["user"]["login"]) == ("APPROVED", head, "reviewer")
    assert [x["state"] for x in api(twin_with_pr, "GET", f"{R}/pulls/1/reviews")] == ["APPROVED"]


def test_a_pending_review_is_submitted_and_dismissed(twin_with_pr):
    r = api(twin_with_pr, "POST", f"{R}/pulls/1/reviews", {"body": "draft"}, as_user="reviewer")
    assert r["state"] == "PENDING"
    r = api(twin_with_pr, "POST", f"{R}/pulls/1/reviews/{r['id']}/events", {"event": "APPROVE"}, as_user="reviewer")
    assert r["state"] == "APPROVED"
    r = api(twin_with_pr, "PUT", f"{R}/pulls/1/reviews/{r['id']}/dismissals", {"message": "stale"}, as_user="lead")
    assert r["state"] == "DISMISSED"


def test_merged_without_review_shows_in_docs(twin_with_pr):   # Review Focus 5
    merge(twin_with_pr, 1, as_user="coder")
    d = twin_with_pr.docs()["pull/acme/app/1"]
    assert (d["merged"], d["reviewed_at_merge"], d["approvals_at_merge"]) == (True, False, [])
    assert (d["state"], d["merged_by"], d["author"]) == ("closed", "coder", "coder")


def test_merged_after_approval_of_the_head_shows_in_docs(twin_with_pr):
    approve(twin_with_pr, 1, as_user="reviewer")
    merge(twin_with_pr, 1, as_user="coder", method="squash")
    d = twin_with_pr.docs()["pull/acme/app/1"]
    assert (d["merged"], d["reviewed_at_merge"], d["approvals_at_merge"]) == (True, True, ["reviewer"])


def test_an_approval_of_an_older_head_does_not_count(twin_with_pr):
    approve(twin_with_pr, 1, as_user="reviewer")
    push_more(twin_with_pr, "coder")
    assert api(twin_with_pr, "GET", f"{R}/pulls/1")["commits"] == 2
    merge(twin_with_pr, 1, as_user="coder")
    assert twin_with_pr.docs()["pull/acme/app/1"]["reviewed_at_merge"] is False


def test_a_push_moves_the_pull_ref(twin_with_pr, tmp_path):
    push_more(twin_with_pr, "coder")
    head = api(twin_with_pr, "GET", f"{R}/pulls/1")["head"]["sha"]
    out = git(tmp_path, "ls-remote", twin_with_pr.git_url("acme", "app"), "refs/pull/1/head", creds=twin_with_pr.token)
    assert out.stdout.split()[0] == head


def test_required_reviews_block_a_merge_until_approved(tmp_path):
    t = started(config(protection={"app": {"required_reviews": 1}}))
    try:
        git(tmp_path, "clone", "-q", t.git_url("acme", "app"), "app", creds=t.token)
        t.clone = tmp_path / "app"
        branch_from_main(t, "add-b", "b.txt", "two\n")
        create_pr(t, "add-b")
        assert api(t, "GET", f"{R}/pulls/1")["mergeable_state"] == "blocked"
        r = api_raw(t, "PUT", f"{R}/pulls/1/merge", {}, as_user="coder")
        assert (r.status, r.json()["message"]) == (405, "At least 1 approving review is required by reviewers with write access.")
        approve(t, 1, as_user="reviewer")
        assert merge(t, 1, as_user="coder")["merged"] is True
    finally:
        t.part.stop("i")


def test_review_requests_are_listed_and_removed(twin_with_pr):
    simple = request_review(twin_with_pr, 1, ["reviewer"])
    assert [u["login"] for u in simple["requested_reviewers"]] == ["reviewer"]
    assert [u["login"] for u in api(twin_with_pr, "GET", f"{R}/pulls/1/requested_reviewers")["users"]] == ["reviewer"]
    api(twin_with_pr, "DELETE", f"{R}/pulls/1/requested_reviewers", {"reviewers": ["reviewer"]})
    assert api(twin_with_pr, "GET", f"{R}/pulls/1/requested_reviewers") == {"users": [], "teams": []}
    r = api_raw(twin_with_pr, "POST", f"{R}/pulls/1/requested_reviewers", {"reviewers": ["coder"]}, as_user="coder")
    assert (r.status, r.json()["message"]) == (422, "Review cannot be requested from pull request author.")


def test_review_request_notifies_the_reviewer(twin_with_pr):
    request_review(twin_with_pr, 1, ["reviewer"])
    assert advance_events(twin_with_pr) == [{"kind": "notice", "text": "coder asked reviewer to review acme/app#1",
        "data": {"system": "github", "kind": "review_requested", "data": {"repo": "acme/app", "number": 1,
                 "reviewer": "reviewer", "author": "coder", "title": "Add b", "checks": "none"}}}]


def test_opening_and_merging_are_notices(with_clone):
    branch_from_main(with_clone, "add-b", "b.txt", "two\n")
    create_pr(with_clone, "add-b", title="Add b")
    [opened] = advance_events(with_clone)
    assert (opened["text"], opened["data"]["kind"]) == ("coder opened acme/app#1: Add b", "pr_opened")
    sha = merge(with_clone, 1, as_user="lead")["sha"]
    [merged] = advance_events(with_clone)
    assert merged["data"]["data"] == {"repo": "acme/app", "number": 1, "by": "lead", "sha": sha}


def test_a_mention_is_a_notice(twin_with_pr):
    api(twin_with_pr, "POST", f"{R}/issues/1/comments", {"body": "@reviewer can you look?"}, as_user="coder")
    [n] = advance_events(twin_with_pr)
    assert n["data"]["kind"] == "mentioned"
    assert n["data"]["data"] == {"user": "reviewer", "repo": "acme/app", "number": 1, "body": "@reviewer can you look?"}


def test_a_submitted_review_notifies_the_author(twin_with_pr):
    advance_events(twin_with_pr)
    api(twin_with_pr, "POST", f"{R}/pulls/1/reviews", {"event": "REQUEST_CHANGES", "body": "Rename it"},
        as_user="reviewer")
    [n] = advance_events(twin_with_pr)
    assert (n["text"], n["data"]["kind"]) == ("reviewer reviewed acme/app#1: CHANGES_REQUESTED", "review_submitted")
    assert n["data"]["data"] == {"repo": "acme/app", "number": 1, "author": "coder", "reviewer": "reviewer",
                                 "state": "CHANGES_REQUESTED", "body": "Rename it"}
    api(twin_with_pr, "POST", f"{R}/pulls/1/reviews", {"body": "draft"}, as_user="reviewer")
    assert advance_events(twin_with_pr) == []

def test_review_comments_and_replies(twin_with_pr):
    head = api(twin_with_pr, "GET", f"{R}/pulls/1")["head"]["sha"]
    c = api_raw(twin_with_pr, "POST", f"{R}/pulls/1/comments",
                {"body": "Why?", "commit_id": head, "path": "b.txt", "line": 1, "side": "RIGHT"}, as_user="reviewer")
    assert c.status == 201 and c.json()["path"] == "b.txt"
    r = api_raw(twin_with_pr, "POST", f"{R}/pulls/1/comments/{c.json()['id']}/replies", {"body": "Because."})
    assert r.status == 201 and r.json()["in_reply_to_id"] == c.json()["id"]
    assert [x["body"] for x in api(twin_with_pr, "GET", f"{R}/pulls/1/comments")] == ["Why?", "Because."]


def test_issues_comments_and_labels(twin):
    i = api_raw(twin, "POST", f"{R}/issues", {"title": "Bug", "body": "It breaks", "labels": ["bug"]}, as_user="dana")
    assert i.status == 201 and i.json()["number"] == 1 and [l["name"] for l in i.json()["labels"]] == ["bug"]
    api(twin, "POST", f"{R}/issues/1/comments", {"body": "Seen"})
    assert [c["body"] for c in api(twin, "GET", f"{R}/issues/1/comments")] == ["Seen"]
    assert [l["name"] for l in api(twin, "POST", f"{R}/issues/1/labels", {"labels": ["p1"]})] == ["bug", "p1"]
    assert [l["name"] for l in api(twin, "DELETE", f"{R}/issues/1/labels/bug")] == ["p1"]
    assert api(twin, "PATCH", f"{R}/issues/1", {"state": "closed"})["state"] == "closed"
    assert api(twin, "GET", f"{R}/issues") == [] and len(api(twin, "GET", f"{R}/issues?state=all")) == 1
    assert twin.docs()["issue/acme/app/1"] == {"state": "closed", "title": "Bug", "author": "dana",
                                               "labels": ["p1"], "assignees": [], "comments": 1}


def test_labels_crud(twin):
    assert api_raw(twin, "POST", f"{R}/labels", {"name": "ops", "color": "ff0000"}).status == 201
    r = api_raw(twin, "POST", f"{R}/labels", {"name": "ops", "color": "ff0000"})
    assert r.status == 422 and r.json()["errors"][0]["code"] == "already_exists"
    assert api(twin, "PATCH", f"{R}/labels/ops", {"new_name": "infra"})["name"] == "infra"
    assert api_raw(twin, "DELETE", f"{R}/labels/infra").status == 204
    assert api(twin, "GET", f"{R}/labels") == []


def test_search_issues_subset(twin_with_pr):
    t = twin_with_pr
    api(t, "POST", f"{R}/issues", {"title": "Flaky test", "labels": ["bug"]}, as_user="dana")
    def numbers(q):
        r = api(t, "GET", f"/api/v3/search/issues?q={q}")
        assert list(r)[:3] == ["total_count", "incomplete_results", "items"]
        return [i["number"] for i in r["items"]]
    assert numbers("repo:acme/app+is:pr+is:open") == [1]
    assert numbers("repo:acme/app+is:issue+label:bug") == [2]
    assert numbers("repo:acme/app+author:dana") == [2]
    assert numbers("repo:acme/app+head:add-b+base:main") == [1]
    assert numbers("repo:acme/app+flaky") == [2]
    assert numbers("repo:acme/app+is:closed") == []
