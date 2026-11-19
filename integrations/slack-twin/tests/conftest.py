import json
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass

import pytest

from slack_twin.part import SlackTwin

HOST = "slack.acme.test"
TOKEN = "xoxb-test-token"
START_MS = 1_773_122_400_000
FORWARDED = {"X-Forwarded-Proto": "https", "X-Forwarded-Host": HOST}


def config(**extra):
    c = {"hosts": [{"host": HOST}], "token": TOKEN, "default_user": "dev",
         "team": {"name": "Acme", "domain": "acme"},
         "users": [{"name": "dev", "real_name": "Dev One"}, {"name": "rev", "real_name": "Rev Two"},
                   {"name": "ops", "real_name": "Ops Three"}],
         "channels": [{"name": "eng", "topic": "Engineering", "general": True}, {"name": "incidents"},
                      {"name": "leads", "private": True}],
         "messages": [{"channel": "eng", "user": "rev", "text": "Morning all", "at": "-2h"},
                      {"channel": "incidents", "user": "ops", "text": "Checkout errors rising", "at": "-1h"}]}
    c.update(extra)
    return c


class Clock:
    def __init__(self):
        self.ms = START_MS

    def __call__(self):
        return self.ms


@dataclass
class Twin:
    part: SlackTwin
    instance: str = "i"
    token: str = TOKEN

    @property
    def up(self):
        return self.part.http_routes(self.instance)["routes"][0]["upstream"]

    @property
    def base(self):
        return self.up

    def docs(self):
        return self.part.inspect({"instance": self.instance}, [])["docs"]

    def act(self, person, action, args):
        return self.part.call_tool(self.instance, "slack_act", {"person": person, "action": action, "args": args}, 0)

    def notices(self):
        return [e["data"] for e in self.part.advance(self.instance, 0)["events"]]


def raw_call(twin, method, args=None, token=TOKEN, as_user=None, form=False, headers=None):
    h = {**FORWARDED, **(headers or {})}
    if token:
        h["Authorization"] = f"Bearer {token}"
    if as_user:
        h["X-Arena-Agent"] = as_user
    if form:
        data, h["Content-Type"] = urllib.parse.urlencode(args or {}).encode(), "application/x-www-form-urlencoded"
    else:
        data, h["Content-Type"] = json.dumps(args or {}).encode(), "application/json; charset=utf-8"
    req = urllib.request.Request(f"{twin.up}/api/{method}", data=data, method="POST", headers=h)
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, {k.lower(): v for k, v in r.headers.items()}, json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, {k.lower(): v for k, v in e.headers.items()}, json.loads(e.read() or b"null")


def slack_call(twin, method, args=None, **kw):
    status, _, body = raw_call(twin, method, args, **kw)
    assert status == 200
    return body


def ok(twin, method, args=None, **kw):
    body = slack_call(twin, method, args, **kw)
    assert body["ok"] is True, body
    return body


@pytest.fixture(autouse=True)
def saves_root(monkeypatch, tmp_path):
    root = tmp_path / "saves"
    monkeypatch.setenv("ARENA_SAVES", str(root))
    return root


def started(cfg=None) -> Twin:
    part = SlackTwin(clock=Clock())
    part.start("i", cfg or config(), 1, 0)
    return Twin(part)


@pytest.fixture
def slack():
    t = started()
    yield t
    t.part.stop("i")


def chan_id(client, name):
    return next(c["id"] for c in client.conversations_list(types="public_channel,private_channel")["channels"]
                if c["name"] == name)
