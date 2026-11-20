import json
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass

import pytest

from pagerduty_twin.part import PagerDutyTwin

API_HOST = "api.pagerduty.acme.test"
EVENTS_HOST = "events.pagerduty.acme.test"
TOKEN = "pd-test-token"
KEY = "0123456789abcdef0123456789abcdef"   # the checkout service's Events v2 integration key
START_MS = 1_773_122_400_000
HOUR_MS = 3_600_000


def config(**extra):
    c = {"hosts": [{"host": API_HOST}, {"host": EVENTS_HOST}], "token": TOKEN, "default_user": "ops",
         "email_domain": "acme.test",
         "users": [{"name": "ops", "display_name": "Ops One"}, {"name": "dev", "display_name": "Dev Two"},
                   {"name": "lead", "display_name": "Lead Three"}],
         "schedules": [{"name": "Primary", "rotation": ["dev", "ops"], "shift_hours": 12, "start": "-1h"}],
         "services": [{"name": "checkout", "schedule": "Primary", "integration_key": KEY},
                      {"name": "search", "schedule": "Primary", "integration_key": "f" * 32}]}
    c.update(extra)
    return c


class Clock:
    def __init__(self):
        self.ms = START_MS

    def __call__(self):
        return self.ms


@dataclass
class Twin:
    part: PagerDutyTwin
    instance: str = "i"
    token: str = TOKEN

    @property
    def up(self):
        return self.part.http_routes(self.instance)["routes"][0]["upstream"]

    def docs(self):
        return self.part.inspect({"instance": self.instance}, [])["docs"]

    def tool(self, name, args):
        return self.part.call_tool(self.instance, name, args, 0)

    def notices(self):
        return [e["data"] for e in self.part.advance(self.instance, 0)["events"]]


def raw(twin, method, path, body=None, host=API_HOST, token=TOKEN, sender="ops@acme.test", as_user=None, headers=None):
    h = {"X-Forwarded-Proto": "https", "X-Forwarded-Host": host, "Content-Type": "application/json",
         "Accept": "application/vnd.pagerduty+json;version=2", **(headers or {})}
    if token:
        h["Authorization"] = f"Token token={token}"
    if sender:
        h["From"] = sender
    if as_user:
        h["X-Arena-Agent"] = as_user
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(twin.up + path, data=data, method=method, headers=h)
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, {k.lower(): v for k, v in r.headers.items()}, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        return e.code, {k.lower(): v for k, v in e.headers.items()}, json.loads(e.read() or b"null")


def rest(twin, method, path, body=None, **kw):
    status, _, out = raw(twin, method, path, body, **kw)
    assert status < 400, (status, out)
    return out


def enqueue(twin, action, dedup_key=None, summary="Checkout 5xx above 2%", key=KEY):
    body = {"routing_key": key, "event_action": action}
    if dedup_key:
        body["dedup_key"] = dedup_key
    if action == "trigger":
        body["payload"] = {"summary": summary, "source": "prometheus", "severity": "critical"}
    status, _, out = raw(twin, "POST", "/v2/enqueue", body, host=EVENTS_HOST, token=None, sender=None)
    assert status == 202, out
    return out


def list_incidents(twin, **params):
    query = urllib.parse.urlencode([(f"{k}[]" if isinstance(v, list) else k, x) for k, v in params.items()
                                    for x in (v if isinstance(v, list) else [v])])
    return rest(twin, "GET", f"/incidents?{query}")["incidents"]


@pytest.fixture(autouse=True)
def saves_root(monkeypatch, tmp_path):
    root = tmp_path / "saves"
    monkeypatch.setenv("SHAKEDOWN_SAVES", str(root))
    return root


def started(cfg=None) -> Twin:
    part = PagerDutyTwin(clock=Clock())
    part.start("i", cfg or config(), 1, 0)
    return Twin(part)


@pytest.fixture
def pd():
    t = started()
    yield t
    t.part.stop("i")
