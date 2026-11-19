import base64
import json
import os
import subprocess
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path

import time

import pytest

from fake_docker import FakeDocker
from github_twin.part import GithubTwin

FIXTURES = Path(__file__).parent / "fixtures"
HOST = "github.acme.test"
TOKEN = "t0ken-for-tests"
START_MS = 1_773_122_400_000


ACTIONS = {"enabled": True, "runners": {"ubuntu-latest": "base:1"}, "image_prefix": "shakedown"}


def config(**extra):
    c = {"hosts": [{"host": HOST}], "org": "acme", "token": TOKEN, "default_user": "coder",
         "installed_version": "3.20.0", "actions": dict(ACTIONS),
         "repos": [{"name": "app", "source": str(FIXTURES / "repos" / "app"), "default_branch": "main",
                    "description": "A sample service", "private": True}]}
    c.update(extra)
    return c


class Clock:
    def __init__(self):
        self.ms = START_MS

    def __call__(self):
        self.ms += 1000
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
    part: GithubTwin
    instance: str = "i"
    token: str = TOKEN
    clone: Path | None = None

    @property
    def up(self):
        return self.part.http_routes(self.instance)["routes"][0]["upstream"]

    def git_url(self, owner, repo):
        return f"{self.up}/{owner}/{repo}.git"

    def docs(self):
        return self.part.inspect({"instance": self.instance}, [])["docs"]


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None   # a redirect is the answer the test reads


OPENER = urllib.request.build_opener(_NoRedirect)


def raw(up, method, path, body=None, headers=None) -> Answer:
    data = body if isinstance(body, (bytes, type(None))) else json.dumps(body).encode()
    req = urllib.request.Request(up + path, data=data, method=method, headers=dict(headers or {}))
    try:
        with OPENER.open(req, timeout=30) as r:
            return Answer(r.status, {k.lower(): v for k, v in r.headers.items()}, r.read())
    except urllib.error.HTTPError as e:
        return Answer(e.code, {k.lower(): v for k, v in e.headers.items()}, e.read())


def raw_get(up, path, headers=None):
    return raw(up, "GET", path, headers=headers)


def api_raw(twin, method, path, body=None, forwarded=("https", HOST), as_user=None, token=None, headers=None):
    h = {"Authorization": f"token {token or twin.token}", "Accept": "application/vnd.github+json"}
    if forwarded:
        h.update({"X-Forwarded-Proto": forwarded[0], "X-Forwarded-Host": forwarded[1]})
    if as_user:
        h["X-Arena-Agent"] = as_user
    h.update(headers or {})
    return raw(twin.up, method, path, body, h)


def api(twin, method, path, body=None, **kw):
    r = api_raw(twin, method, path, body, **kw)
    assert r.status < 400, (r.status, r.body[:500])
    return r.json()


def b64(text: str) -> str:
    return base64.b64encode(text.encode()).decode()


def git_env(tmp_path: Path, token=None) -> dict:
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update({"HOME": str(tmp_path), "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null",
                "GIT_TERMINAL_PROMPT": "0", "GIT_AUTHOR_NAME": "coder", "GIT_AUTHOR_EMAIL": "coder@acme.test",
                "GIT_COMMITTER_NAME": "coder", "GIT_COMMITTER_EMAIL": "coder@acme.test"})
    if token:
        env.update({"GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": "credential.helper",
                    "GIT_CONFIG_VALUE_0": f"!f() {{ echo username=x-access-token; echo password={token}; }}; f"})
    return env


def git(cwd: Path, *args, creds=None, check=True) -> subprocess.CompletedProcess:
    cwd.mkdir(parents=True, exist_ok=True)
    r = subprocess.run(["git", *args], cwd=cwd, env=git_env(cwd, creds), capture_output=True, text=True, timeout=60)
    assert not check or r.returncode == 0, r.stderr
    return r


def commit(repo: Path, name: str, text: str, message=None) -> str:
    (repo / name).parent.mkdir(parents=True, exist_ok=True)
    (repo / name).write_text(text)
    git(repo, "add", name)
    git(repo, "commit", "-q", "-m", message or f"Add {name}")
    return head(repo)


def head(repo: Path) -> str:
    return git(repo, "rev-parse", "HEAD").stdout.strip()


@pytest.fixture(autouse=True)
def saves_root(monkeypatch, tmp_path):
    root = tmp_path / "saves"
    monkeypatch.setenv("ARENA_SAVES", str(root))
    return root


def started(cfg=None) -> Twin:
    part = GithubTwin(clock=Clock(), docker=FakeDocker())
    part.start("i", cfg or config(), 1, 0)
    return Twin(part)


@pytest.fixture
def twin():
    t = started()
    yield t
    t.part.stop("i")


@pytest.fixture
def clone(twin, tmp_path):
    """A clone of acme/app with push credentials."""
    git(tmp_path, "clone", "-q", twin.git_url("acme", "app"), "app", creds=twin.token)
    return tmp_path / "app"


def push(repo: Path, token: str, *refspecs):
    return git(repo, "push", "-q", "origin", *refspecs, creds=token)


@pytest.fixture
def twin_with_45_commits(twin, clone):
    for i in range(44):
        git(clone, "commit", "-q", "--allow-empty", "-m", f"Commit {i}")
    push(clone, twin.token, "HEAD:main")
    return twin


def branch_from_main(t: Twin, branch: str, name: str, text: str, message=None) -> str:
    """A commit on a new branch off main, pushed; its sha (the clone is left on main)."""
    git(t.clone, "checkout", "-q", "-B", branch, "origin/main")
    sha = commit(t.clone, name, text, message)
    push(t.clone, t.token, f"HEAD:refs/heads/{branch}")
    git(t.clone, "checkout", "-q", "main")
    return sha


def create_pr(t: Twin, head: str, base: str = "main", as_user: str = "coder", title=None) -> int:
    body = {"title": title or f"Change {head}", "head": head, "base": base, "body": "Please review."}
    r = api_raw(t, "POST", "/api/v3/repos/acme/app/pulls", body, as_user=as_user)
    assert r.status == 201, r.body
    return r.json()["number"]


def advance_events(t: Twin) -> list:
    t.ms = getattr(t, "ms", 0) + 100
    return t.part.advance(t.instance, t.ms)["events"]


@pytest.fixture
def with_clone(twin, clone):
    twin.clone = clone
    return twin


@pytest.fixture
def twin_with_pr(with_clone):
    branch_from_main(with_clone, "add-b", "b.txt", "two\n", "Add b")
    assert create_pr(with_clone, "add-b", title="Add b") == 1
    advance_events(with_clone)
    return with_clone


def gql_raw(twin, query, variables=None, as_user=None, headers=None, token=None) -> Answer:
    body = {"query": query, **({"variables": variables} if variables is not None else {})}
    return api_raw(twin, "POST", "/api/graphql", body, as_user=as_user, headers=headers, token=token)


def gql(twin, query, variables=None, **kw) -> dict:
    r = gql_raw(twin, query, variables, **kw)
    assert r.status == 200, (r.status, r.body[:500])
    return r.json()


def gql_data(twin, query, variables=None, **kw) -> dict:
    out = gql(twin, query, variables, **kw)
    assert "errors" not in out, out["errors"]
    return out["data"]


@pytest.fixture
def front(twin, tmp_path):
    """The twin behind a TLS front on 127.0.0.1 with a test CA (see tls_front)."""
    from tls_front import TlsFront, make_ca
    ca, cert, key = make_ca(tmp_path / "ca")
    f = TlsFront(twin.up, cert, key)
    f.ca, f.twin = ca, twin
    yield f
    f.close()


# --- Actions ------------------------------------------------------------------------------


class ManualClock:
    """World time that moves only when a test moves it."""

    def __init__(self):
        self.ms = START_MS

    def __call__(self):
        return self.ms


def actions_config(**actions):
    c = config()
    c["repos"][0]["source"] = str(FIXTURES / "repos" / "ci-app")
    c["actions"] = {**ACTIONS, **actions}
    return c


def actions_twin(tmp_path, docker=None, cfg=None) -> Twin:
    """acme/app with a CI workflow, Actions on a fake Docker, a clone at `t.clone`, world time `t.clock`."""
    clock, docker = ManualClock(), docker or FakeDocker()
    part = GithubTwin(clock=clock, docker=docker)
    part.start("i", cfg or actions_config(), 1, 0)
    t = Twin(part)
    t.docker, t.clock = docker, clock
    git(tmp_path, "clone", "-q", t.git_url("acme", "app"), "app", creds=t.token)
    t.clone = tmp_path / "app"
    return t


@pytest.fixture
def twin_fake_docker(tmp_path):
    t = actions_twin(tmp_path)
    yield t
    t.part.stop("i")


def runs_of(t: Twin, instance: str = "i") -> list:
    return api(Twin(t.part, instance), "GET", "/api/v3/repos/acme/app/actions/runs?per_page=100")["workflow_runs"]


def drain(t: Twin, instance: str = "i", timeout_s: float = 15) -> list:
    """Wait until every run is completed; the runs."""
    deadline = time.monotonic() + timeout_s
    while True:
        runs = runs_of(t, instance)
        if all(r["status"] == "completed" for r in runs) or time.monotonic() > deadline:
            assert all(r["status"] == "completed" for r in runs), [(r["id"], r["status"]) for r in runs]
            return runs
        time.sleep(0.05)


def wait_for(cond, timeout_s: float = 15, what: str = "condition"):
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        out = cond()
        if out:
            return out
        time.sleep(0.05)
    raise AssertionError(f"timed out waiting for {what}")


def push_commit(t: Twin, branch: str, name: str, text: str, message=None) -> str:
    """A commit on `branch` (made from origin's, or main's when new), pushed; its sha."""
    git(t.clone, "fetch", "-q", "origin", creds=t.token)
    start = f"origin/{branch}" if git(t.clone, "rev-parse", "--verify", "-q", f"origin/{branch}", check=False).returncode == 0 \
        else "origin/main"
    git(t.clone, "checkout", "-q", "-B", branch, start)
    sha = commit(t.clone, name, text, message)
    push(t.clone, t.token, f"HEAD:refs/heads/{branch}")
    return sha
