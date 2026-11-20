"""Apps that post into channels what another system tells the world (`slack_app`, called by links), as the
GitHub and PagerDuty apps for Slack do."""
from conftest import config, started

APPS = [{"name": "pagerduty", "user": "pagerduty", "channel": "#incidents",
         "messages": {"incident_triggered": "{/incident} triggered on {/service}: {/summary} ({/assignee} paged)"}},
        {"name": "github", "user": "github", "channel": "#eng",
         "messages": {"pr_merged": "{/by} merged {/repo}#{/number}", "pr_opened": "{/author} opened {/missing}"}}]


def _tool(twin, args):
    return twin.part.call_tool(twin.instance, "slack_app", args, 0)


def test_an_app_posts_the_message_of_a_kind_it_knows():
    twin = started(config(apps=APPS))
    try:
        out = _tool(twin, {"app": "pagerduty", "kind": "incident_triggered",
                           "data": {"incident": "P1", "service": "orders", "summary": "5xx above 5%", "assignee": "kim"}})
        assert out["writes"] == ["channel/incidents"] and out["result"]["posted"] is True
        last = twin.docs()["channel/incidents"]["messages"][-1]
        assert last == {"user": "pagerduty", "text": "P1 triggered on orders: 5xx above 5% (kim paged)",
                        "ts": out["result"]["ts"]}
        _tool(twin, {"app": "github", "kind": "pr_opened", "data": {"author": "ana"}})
        assert twin.docs()["channel/eng"]["messages"][-1]["text"] == "ana opened "
    finally:
        twin.part.stop("i")


def test_a_kind_without_a_message_posts_nothing_and_an_unknown_app_is_refused():
    twin = started(config(apps=APPS))
    try:
        before = twin.docs()
        assert _tool(twin, {"app": "github", "kind": "checks_completed", "data": {}})["result"] == {"posted": False}
        assert twin.docs() == before
        assert _tool(twin, {"app": "jenkins", "kind": "x", "data": {}})["error"] == "no app jenkins"
    finally:
        twin.part.stop("i")


def test_the_tool_is_described_for_links():
    twin = started(config(apps=APPS))
    try:
        tools = {t["name"]: t for t in twin.part.describe()["tools"]}
        assert tools["slack_app"]["effect"] == "write"
    finally:
        twin.part.stop("i")
