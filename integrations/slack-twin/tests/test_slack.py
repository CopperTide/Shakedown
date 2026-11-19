import re

import pytest
from slack_sdk import WebClient
from slack_sdk.errors import SlackApiError

from conftest import FORWARDED, HOST, START_MS, chan_id, ok, raw_call, slack_call


def client(slack, agent="dev"):
    return WebClient(token=slack.token, base_url=slack.base + "/api/", headers={**FORWARDED, "X-Arena-Agent": agent})


def test_slack_sdk_round_trip(slack):
    c = client(slack)
    ts = c.chat_postMessage(channel="#eng", text="deploying")["ts"]
    c.chat_postMessage(channel="#eng", text="done", thread_ts=ts)
    assert [m["text"] for m in c.conversations_replies(channel=chan_id(c, "eng"), ts=ts)["messages"]] == ["deploying", "done"]


def test_slack_errors_are_ok_false(slack):
    assert slack_call(slack, "chat.postMessage", {"channel": "#nope", "text": "x"}) == {"ok": False, "error": "channel_not_found"}


@pytest.mark.parametrize("token,error", [(None, "not_authed"), ("xoxb-wrong", "invalid_auth")])
def test_tokens(slack, token, error):
    assert slack_call(slack, "auth.test", token=token) == {"ok": False, "error": error}


def test_the_token_may_be_a_form_field(slack):
    body = slack_call(slack, "auth.test", {"token": slack.token}, token=None, form=True)
    assert body["ok"] is True and body["user"] == "dev"


def test_unknown_method(slack):
    assert slack_call(slack, "nope.nothing") == {"ok": False, "error": "unknown_method"}


def test_auth_test_names_the_agent(slack):
    body = ok(slack, "auth.test", as_user="rev")
    assert (body["user"], body["team"], body["url"]) == ("rev", "Acme", f"https://{HOST}/")
    assert re.fullmatch(r"U[A-Z0-9]{8,}", body["user_id"]) and re.fullmatch(r"T[A-Z0-9]{8,}", body["team_id"])


def test_users_and_channels(slack):
    c = client(slack)
    names = [u["name"] for u in c.users_list()["members"]]
    assert names == ["dev", "ops", "rev"]
    rev = next(u for u in c.users_list()["members"] if u["name"] == "rev")
    assert c.users_info(user=rev["id"])["user"]["real_name"] == "Rev Two"
    public = [ch["name"] for ch in c.conversations_list()["channels"]]
    assert public == ["eng", "incidents"]
    info = c.conversations_info(channel=chan_id(c, "eng"))["channel"]
    assert (info["name"], info["topic"]["value"], info["is_general"]) == ("eng", "Engineering", True)


def test_conversations_need_an_id(slack):
    assert slack_call(slack, "conversations.history", {"channel": "eng"}) == {"ok": False, "error": "channel_not_found"}


def test_history_is_newest_first_with_limits(slack):
    c = client(slack)
    eng = chan_id(c, "eng")
    for i in range(3):
        slack.part._wall.ms += 1000
        c.chat_postMessage(channel=eng, text=f"m{i}")
    page = c.conversations_history(channel=eng, limit=2)
    assert [m["text"] for m in page["messages"]] == ["m2", "m1"] and page["has_more"] is True
    older = c.conversations_history(channel=eng, latest=page["messages"][-1]["ts"])
    assert [m["text"] for m in older["messages"]] == ["m0", "Morning all"] and older["has_more"] is False


def test_ts_is_world_time_and_unique(slack):
    c = client(slack)
    a = c.chat_postMessage(channel="#eng", text="a")["ts"]
    b = c.chat_postMessage(channel="#eng", text="b")["ts"]
    assert a == f"{START_MS // 1000}.000000" and b == f"{START_MS // 1000}.000001"


def test_threads_show_their_replies(slack):
    c = client(slack)
    eng = chan_id(c, "eng")
    ts = c.chat_postMessage(channel=eng, text="parent")["ts"]
    client(slack, "rev").chat_postMessage(channel=eng, text="reply", thread_ts=ts)
    top = c.conversations_history(channel=eng)["messages"][0]
    assert (top["text"], top["reply_count"], top["thread_ts"]) == ("parent", 1, ts)
    assert slack_call(slack, "conversations.replies", {"channel": eng, "ts": "1.000000"}) == \
        {"ok": False, "error": "thread_not_found"}


def test_update_only_your_own_message(slack):
    c = client(slack)
    posted = c.chat_postMessage(channel="#eng", text="typo")
    updated = c.chat_update(channel=posted["channel"], ts=posted["ts"], text="fixed")
    assert updated["text"] == "fixed" and c.conversations_history(channel=posted["channel"])["messages"][0]["text"] == "fixed"
    with pytest.raises(SlackApiError) as e:
        client(slack, "rev").chat_update(channel=posted["channel"], ts=posted["ts"], text="mine now")
    assert e.value.response["error"] == "cant_update_message"


def test_reactions(slack):
    c = client(slack)
    posted = c.chat_postMessage(channel="#eng", text="ship it?")
    c.reactions_add(channel=posted["channel"], timestamp=posted["ts"], name="thumbsup")
    client(slack, "rev").reactions_add(channel=posted["channel"], timestamp=posted["ts"], name="thumbsup")
    msg = c.conversations_history(channel=posted["channel"])["messages"][0]
    assert msg["reactions"] == [{"name": "thumbsup", "users": [msg["user"], msg["reactions"][0]["users"][1]], "count": 2}]
    again = slack_call(slack, "reactions.add", {"channel": posted["channel"], "timestamp": posted["ts"], "name": "thumbsup"})
    assert again == {"ok": False, "error": "already_reacted"}


def test_search_messages(slack):
    c = client(slack)
    c.chat_postMessage(channel="#incidents", text="checkout recovered after rollback")
    found = c.search_messages(query="checkout")["messages"]
    assert [m["text"] for m in found["matches"]] == ["checkout recovered after rollback", "Checkout errors rising"]
    assert found["total"] == 2 and found["matches"][0]["channel"]["name"] == "incidents"
    assert [m["text"] for m in c.search_messages(query="checkout from:@ops")["messages"]["matches"]] == ["Checkout errors rising"]
    assert c.search_messages(query="in:#eng checkout")["messages"]["total"] == 0


def test_mentions_are_notices(slack):
    c = client(slack)
    rev = next(u["id"] for u in c.users_list()["members"] if u["name"] == "rev")
    slack.notices()
    posted = c.chat_postMessage(channel="#eng", text=f"<@{rev}> and @ops please look")
    assert slack.notices() == [
        {"system": "slack", "kind": "mentioned", "data": {"channel": "eng", "user": "rev",
                                                           "text": f"<@{rev}> and @ops please look", "ts": posted["ts"]}},
        {"system": "slack", "kind": "mentioned", "data": {"channel": "eng", "user": "ops",
                                                           "text": f"<@{rev}> and @ops please look", "ts": posted["ts"]}}]


def test_acts_post_and_react(slack):
    made = slack.act("ops", "post", {"channel": "#incidents", "text": "Paging @dev"})
    assert made["writes"] == ["channel/incidents"]
    ts = made["result"]["ts"]
    assert slack.act("dev", "post", {"channel": "incidents", "text": "on it", "thread_ts": ts})["result"]["ts"]
    assert slack.act("dev", "react", {"channel": "#incidents", "ts": ts, "name": "eyes"})["result"] == {"ok": True}
    assert slack.act("dev", "post", {"channel": "#nope", "text": "x"})["error"] == "channel_not_found"
    assert [n["data"]["user"] for n in slack.notices()] == ["dev"]
    doc = slack.docs()["channel/incidents"]
    assert [m["text"] for m in doc["messages"]] == ["Checkout errors rising", "Paging @dev", "on it"]
    assert doc["messages"][2]["thread_ts"] == ts and doc["messages"][1]["user"] == "ops" and doc["last_ts"] == doc["messages"][2]["ts"]


def test_a_read_only_agent_reads_but_cannot_post(slack):
    assert slack.part.http_routes("i")["routes"][0]["honours_readonly"] is True
    ro = {"X-Arena-Readonly": "1"}
    status, headers, body = raw_call(slack, "conversations.list", {}, as_user="auditor", headers=ro)
    assert body["ok"] is True and headers["x-arena-effect"] == "read"
    status, headers, body = raw_call(slack, "chat.postMessage", {"channel": "#eng", "text": "x"}, as_user="auditor", headers=ro)
    assert (status, body["ok"], body["error"], headers["x-arena-effect"]) == (200, False, "missing_scope", "read")
    assert slack.part.advance("i", 0)["changed"] is False


def test_save_and_restore(slack, tmp_path):
    client(slack).chat_postMessage(channel="#eng", text="before the save")
    slack.part.save("i", "s1", tmp_path / "saves" / "slack-twin" / "s1")
    client(slack).chat_postMessage(channel="#eng", text="after the save")
    saved = slack.part.inspect({"save": "s1"}, ["channel/eng"])["docs"]["channel/eng"]
    assert [m["text"] for m in saved["messages"]] == ["Morning all", "before the save"]
    slack.part.start("j", slack.part.ctx("i").config, 1, 0, {"save": "s1"})
    try:
        assert len(slack.part.inspect({"instance": "j"}, ["channel/eng"])["docs"]["channel/eng"]["messages"]) == 2
    finally:
        slack.part.stop("j")
