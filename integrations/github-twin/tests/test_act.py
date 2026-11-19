from conftest import advance_events, api

R = "/api/v3/repos/acme/app"
PATCH = """diff --git a/app.py b/app.py
--- a/app.py
+++ b/app.py
@@ -1,2 +1,2 @@
 def add(a, b):
-    return a + b
+    return b + a
"""


def act(twin, person, action, **args):
    return twin.part.call_tool("i", "github_act", {"person": person, "action": action, "args": args}, 0)


def test_open_pr_act_applies_a_patch_as_the_person(twin):
    out = act(twin, "dana", "open_pr", repo="app", branch="swap", patch=PATCH, title="Swap", body="Order")
    assert out["result"] == {"number": 1}
    assert out["writes"] == ["pull/acme/app/1", "branch/acme/app/swap"]
    pr = api(twin, "GET", f"{R}/pulls/1")
    assert (pr["user"]["login"], pr["title"], pr["head"]["ref"]) == ("dana", "Swap", "swap")
    [f] = api(twin, "GET", f"{R}/pulls/1/files")
    assert f["patch"] == PATCH.split("\n", 3)[3].rstrip("\n")
    assert api(twin, "GET", f"{R}/commits/swap")["commit"]["author"]["name"] == "dana"
    assert advance_events(twin)[0]["data"]["kind"] == "pr_opened"


def test_review_merge_comment_and_close_acts(twin):
    act(twin, "dana", "open_pr", repo="acme/app", branch="swap", patch=PATCH, title="Swap", body="")
    assert act(twin, "dana", "review", repo="app", number=1, event="APPROVE", body="ok")["error"] == \
        "Can not approve your own pull request"
    assert act(twin, "lee", "review", repo="app", number=1, event="APPROVE", body="ok")["writes"] == ["pull/acme/app/1"]
    assert "error" not in act(twin, "lee", "comment", repo="app", number=1, body="Thanks")
    out = act(twin, "dana", "merge", repo="app", number=1, method="squash")
    assert len(out["result"]["sha"]) == 40
    d = twin.docs()["pull/acme/app/1"]
    assert (d["merged"], d["approvals_at_merge"]) == (True, ["lee"])
    api(twin, "POST", f"{R}/issues", {"title": "Old"})
    act(twin, "lee", "close", repo="app", number=2)
    assert twin.docs()["issue/acme/app/2"]["state"] == "closed"


def test_an_act_that_fails_changes_nothing(twin):
    out = act(twin, "dana", "open_pr", repo="app", branch="bad", patch="not a patch", title="X", body="")
    assert out["error"].startswith("the patch does not apply")
    assert "pull/acme/app/1" not in twin.docs() and "branch/acme/app/bad" not in twin.docs()
    assert act(twin, "dana", "dance", repo="app")["error"] == "unknown action dance"
