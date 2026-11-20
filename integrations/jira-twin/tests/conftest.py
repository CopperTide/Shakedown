import base64
import json
import urllib.error
import urllib.request
from dataclasses import dataclass
from urllib.parse import urlencode, urlsplit

import pytest

from jira_twin.part import JiraTwin

HOST = "jira.acme.test"
TOKEN = "t0ken-for-tests"
START_MS = 1_773_122_400_000   # world now in the tests
DAY_MS = 86_400_000

WORKFLOW = {"statuses": [{"name": "To Do", "category": "new"}, {"name": "In Progress", "category": "indeterminate"},
                         {"name": "In Review", "category": "indeterminate"}, {"name": "Done", "category": "done"}],
            "transitions": [{"id": "11", "name": "To Do", "to": "To Do"},
                            {"id": "21", "name": "In Progress", "to": "In Progress"},
                            {"id": "31", "name": "Review", "from": ["In Progress"], "to": "In Review"},
                            {"id": "41", "name": "Done", "to": "Done"}]}

ISSUES = [
    {"project": "AC", "summary": "Add the login form", "type": "Story", "status": "In Progress", "priority": "Medium",
     "assignee": "dev", "reporter": "lead", "sprint": 1, "rank": 2, "updated": "-1d", "created": "-20d",
     "description": "Users sign in with email."},
    {"project": "AC", "summary": "Fix the API timeout", "type": "Bug", "status": "Done", "priority": "High",
     "assignee": "rev", "labels": ["backend"], "sprint": 1, "rank": 3, "updated": "-2d", "created": "-20d"},
    {"project": "AC", "summary": "Checkout fails on retry", "type": "Bug", "status": "To Do", "priority": "Highest",
     "assignee": "rev", "reporter": "lead", "sprint": 1, "rank": 1, "updated": "-3d", "created": "-10d"},
    {"project": "AC", "summary": "Tidy the readme", "type": "Task", "status": "To Do", "priority": "Low",
     "assignee": "dev", "rank": 4, "updated": "-30d", "created": "-40d"},
]


def config(**extra):
    c = {"hosts": [{"host": HOST}], "deployment": "cloud", "token": TOKEN, "default_user": "dev",
         "email_domain": "acme.test",
         "projects": [{"key": "AC", "name": "Acme", "type": "software", "lead": "lead"},
                      {"key": "OPS", "name": "Operations", "type": "software", "lead": "lead"}],
         "users": [{"name": "dev", "display_name": "Dev One"}, {"name": "rev", "display_name": "Rev Two"},
                   {"name": "lead", "display_name": "Lead Three"}],
         "workflow": WORKFLOW,
         "boards": [{"id": 1, "name": "AC board", "type": "scrum", "project": "AC"}],
         "sprints": [{"id": 1, "board": 1, "name": "AC Sprint 1", "state": "active", "start": "-7d", "end": "7d",
                      "goal": "Ship sign-in"},
                     {"id": 2, "board": 1, "name": "AC Sprint 2", "state": "future"}],
         "issues": ISSUES}
    c.update(extra)
    return c


class Clock:
    def __init__(self):
        self.ms = START_MS

    def __call__(self):
        return self.ms


@dataclass
class Answer:
    status: int
    headers: dict
    body: bytes

    def json(self):
        return json.loads(self.body) if self.body else None


@dataclass
class Twin:
    part: JiraTwin
    instance: str = "i"
    token: str = TOKEN

    @property
    def up(self):
        return self.part.http_routes(self.instance)["routes"][0]["upstream"]

    def docs(self):
        return self.part.inspect({"instance": self.instance}, [])["docs"]

    def act(self, person, action, args):
        return self.part.call_tool(self.instance, "jira_act", {"person": person, "action": action, "args": args}, 0)

    def notices(self):
        return [e["data"] for e in self.part.advance(self.instance, 0)["events"]]


def basic(user="dev@acme.test", token=TOKEN):
    return "Basic " + base64.b64encode(f"{user}:{token}".encode()).decode()


def raw(up, method, path, body=None, headers=None) -> Answer:
    data = body if isinstance(body, (bytes, type(None))) else json.dumps(body).encode()
    req = urllib.request.Request(up + path, data=data, method=method, headers=dict(headers or {}))
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return Answer(r.status, {k.lower(): v for k, v in r.headers.items()}, r.read())
    except urllib.error.HTTPError as e:
        return Answer(e.code, {k.lower(): v for k, v in e.headers.items()}, e.read())


def call(twin, method, path, body=None, as_user=None, auth=None, headers=None, forwarded=True) -> Answer:
    """A request as the proxy forwards it: the public host in X-Forwarded-*, the agent in X-Arena-Agent."""
    h = {"Content-Type": "application/json", "Accept": "application/json"}
    if auth != "":
        h["Authorization"] = auth or basic()
    if forwarded:
        h.update({"X-Forwarded-Proto": "https", "X-Forwarded-Host": HOST})
    if as_user:
        h["X-Arena-Agent"] = as_user
    h.update(headers or {})
    return raw(twin.up, method, path, body, h)


def api(twin, method, path, body=None, v="2", **kw):
    r = call(twin, method, f"/rest/api/{v}/{path}", body, **kw)
    assert r.status < 400, (r.status, r.body[:500])
    return r.json()


def agile(twin, method, path, body=None, **kw):
    r = call(twin, method, f"/rest/agile/1.0/{path}", body, **kw)
    assert r.status < 400, (r.status, r.body[:500])
    return r.json()


def local(twin, url: str) -> str:
    """The path of an absolute URL the twin minted (which names the public host)."""
    parts = urlsplit(url)
    assert (parts.scheme, parts.netloc) == ("https", HOST), url
    return parts.path + (f"?{parts.query}" if parts.query else "")


def search(twin, jql, **params):
    return api(twin, "GET", "search/jql?" + urlencode({"jql": jql, "fields": "key", **params}))["issues"]


def search_error(twin, jql):
    r = call(twin, "GET", "/rest/api/2/search/jql?" + urlencode({"jql": jql}))
    assert r.status == 400, r.body
    return r.json()


@pytest.fixture(autouse=True)
def saves_root(monkeypatch, tmp_path):
    root = tmp_path / "saves"
    monkeypatch.setenv("SHAKEDOWN_SAVES", str(root))
    return root


def started(cfg=None) -> Twin:
    part = JiraTwin(clock=Clock())
    part.start("i", cfg or config(), 1, 0)
    return Twin(part)


@pytest.fixture
def jira():
    t = started()
    yield t
    t.part.stop("i")


@pytest.fixture
def server_jira():
    t = started(config(deployment="server"))
    yield t
    t.part.stop("i")


@pytest.fixture
def jira_150_issues():
    many = [{"project": "OPS", "summary": f"Chore {i}", "type": "Task", "status": "To Do"} for i in range(150)]
    t = started(config(issues=many))
    yield t
    t.part.stop("i")
