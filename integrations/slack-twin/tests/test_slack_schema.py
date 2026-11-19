"""Every method's answers checked against Slack's published Web API description (MIT; the extract is
committed, see `tests/schemas/SOURCE.md`), and every method a documented one."""
import json
from pathlib import Path

import pytest
from jsonschema import Draft7Validator

from conftest import ok, slack_call
from slack_twin.methods import METHODS

SUBSET = json.loads((Path(__file__).parent / "schemas" / "slack-web-subset.json").read_text())


def _ids(slack):
    users = {u["name"]: u["id"] for u in ok(slack, "users.list")["members"]}
    chans = {c["name"]: c["id"] for c in ok(slack, "conversations.list", {"types": "public_channel,private_channel"})["channels"]}
    return users, chans


def _thread(slack, eng):
    parent = ok(slack, "chat.postMessage", {"channel": eng, "text": "parent"})["ts"]
    ok(slack, "chat.postMessage", {"channel": eng, "text": "reply", "thread_ts": parent}, as_user="rev")
    return parent


def _args(slack, name: str) -> dict:
    users, chans = _ids(slack)
    eng = chans["eng"]
    posted = ok(slack, "chat.postMessage", {"channel": eng, "text": "hello <@%s>" % users["rev"]})["ts"]
    ok(slack, "reactions.add", {"channel": eng, "timestamp": posted, "name": "eyes"}, as_user="rev")
    parent = _thread(slack, eng)
    return {"users.info": {"user": users["rev"]}, "conversations.info": {"channel": eng},
            "conversations.history": {"channel": eng}, "conversations.replies": {"channel": eng, "ts": parent},
            "chat.postMessage": {"channel": "#eng", "text": "x", "thread_ts": parent},
            "chat.update": {"channel": eng, "ts": posted, "text": "edited"},
            "reactions.add": {"channel": eng, "timestamp": posted, "name": "tada"},
            "search.messages": {"query": "hello"},
            "conversations.list": {"types": "public_channel,private_channel"}}.get(name, {})


def _validate(answer, key):
    schema = {**SUBSET["responses"][key], "definitions": SUBSET["definitions"]}
    errors = sorted(Draft7Validator(schema).iter_errors(answer), key=str)
    assert not errors, "\n".join(f"{list(e.absolute_path)}: {e.message}" for e in errors[:10])


@pytest.mark.parametrize("name", sorted(METHODS))
def test_answers_match_slacks_description(slack, name):
    answer = ok(slack, name, _args(slack, name))
    _validate(answer, f"{name} 200")


@pytest.mark.parametrize("name,args", [("chat.postMessage", {"channel": "#nope", "text": "x"}),
                                       ("chat.update", {"channel": "#eng", "ts": "1.000000", "text": "x"}),
                                       ("reactions.add", {"channel": "C0000000000", "timestamp": "1.0", "name": "x"}),
                                       ("conversations.replies", {"channel": "C0000000000", "ts": "1.0"}),
                                       ("users.info", {"user": "U0000000000"})])
def test_errors_match_slacks_description(slack, name, args):
    answer = slack_call(slack, name, args)
    assert answer["ok"] is False
    _validate(answer, f"{name} default")


def test_every_method_is_documented_and_the_extract_names_its_source():
    assert set(METHODS) <= set(SUBSET["methods"])
    assert SUBSET["source"]["sha256"] and SUBSET["source"]["url"].startswith("https://raw.githubusercontent.com/slackapi/")
