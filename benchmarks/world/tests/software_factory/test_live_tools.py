"""Real clients inside a software-factory run (live): gh and PyGithub reach the GitHub twin through shaked's recording
proxy over HTTPS on 443 (`CONNECT`), trusting the run's own CA from the environment an agent's registration gives,
and each request is a step of that agent on the run's timeline. Needs shaked at `SHAKEDOWN_URL` as for
test_live_world.py, `gh` on PATH, and the github-twin package's environment (`uv sync` there; PyGithub is its dev
dependency), whose python runs the PyGithub script so nothing is installed through the agent's proxy."""
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from shaked_api import api, events, external_run, factory_world

GH = shutil.which("gh")
pytestmark = [pytest.mark.live, pytest.mark.skipif(GH is None, reason="the gh binary is not on PATH")]
REPO_ROOT = Path(__file__).resolve().parents[4]
PYGITHUB_FLOW = Path(__file__).parent / "fixtures" / "clients" / "pygithub_flow.py"
TWIN_PYTHON = REPO_ROOT / "integrations" / "github-twin" / ".venv" / "bin" / "python"


class Agent:
    """One registered agent's clients: its own environment from shaked, a home and gh config of its own."""

    def __init__(self, run: dict, home: Path, agent: dict):
        joined = api("POST", f"/api/branches/{run['branch_id']}/agents", agent)
        home.mkdir(parents=True)
        self.cwd = home
        self.env = {"PATH": os.environ["PATH"], "HOME": str(home), "GH_CONFIG_DIR": str(home / "gh"),
                    "XDG_CACHE_HOME": str(home / "cache"), "GIT_AUTHOR_NAME": agent["name"],
                    "GIT_AUTHOR_EMAIL": f"{agent['id']}@example.test", "GIT_COMMITTER_NAME": agent["name"],
                    "GIT_COMMITTER_EMAIL": f"{agent['id']}@example.test", **joined["env"]}

    def run(self, *args, check=True, cwd=None) -> subprocess.CompletedProcess:
        r = subprocess.run(args, env=self.env, cwd=cwd or self.cwd, capture_output=True, text=True, timeout=180)
        assert not check or r.returncode == 0, (args, r.stdout, r.stderr)
        return r

    def gh(self, repo: str, *args, check=True) -> subprocess.CompletedProcess:
        return self.run(GH, *args, "-R", f"{self.env['GH_HOST']}/{repo}", check=check)


@pytest.fixture(scope="module")
def world():
    return factory_world({"seed": 3})


@pytest.fixture(scope="module")
def run(world):
    run = external_run(world["name"], "sprint")
    yield run
    api("POST", f"/api/branches/{run['branch_id']}/finish", {})


@pytest.fixture(scope="module")
def agents(run, tmp_path_factory):
    home = tmp_path_factory.mktemp("agents")
    return {"coder": Agent(run, home / "coder", {"id": "coder", "name": "Coder"}),
            "auditor": Agent(run, home / "auditor", {"id": "auditor", "name": "Auditor", "read_only": True})}


@pytest.fixture(scope="module")
def opened(world, agents):
    """The coder's pull request on orders, from a branch pushed over git: `{repo, number}`."""
    coder, repo = agents["coder"], f"{world['name']}/orders"
    coder.run("git", "clone", "-q", f"https://{coder.env['GH_HOST']}/{repo}.git", "orders")
    work = coder.cwd / "orders"
    coder.run("git", "switch", "-q", "-c", "smoke", cwd=work)
    (work / "SMOKE.md").write_text("A line from the smoke test.\n")
    coder.run("git", "add", "SMOKE.md", cwd=work)
    coder.run("git", "commit", "-q", "-m", "Add a smoke line", cwd=work)
    coder.run("git", "push", "-q", "-u", "origin", "smoke", cwd=work)
    url = coder.gh(repo, "pr", "create", "-t", "Smoke", "-b", "From the smoke test.", "--head", "smoke").stdout
    return {"repo": repo, "number": int(url.strip().rsplit("/", 1)[1])}


def test_live_gh_lists_and_opens_pull_requests_through_the_proxy(agents, opened):
    print(subprocess.run([GH, "--version"], capture_output=True, text=True).stdout.splitlines()[0])
    listed = agents["coder"].gh(opened["repo"], "pr", "list", "--json", "number,title,headRefName").stdout
    assert f'"number":{opened["number"]}' in listed.replace(" ", "") and '"headRefName":"smoke"' in listed


def test_live_a_read_only_agent_reads_but_its_gh_merge_is_refused(agents, opened):
    auditor = agents["auditor"]
    assert "\tSmoke\tsmoke\tOPEN" in auditor.gh(opened["repo"], "pr", "list").stdout
    refused = auditor.gh(opened["repo"], "pr", "merge", str(opened["number"]), "--merge", check=False)
    assert refused.returncode != 0, refused.stdout
    view = agents["coder"].gh(opened["repo"], "pr", "view", str(opened["number"]), "--json", "state").stdout
    assert '"OPEN"' in view


def test_live_pygithub_reads_the_pull_request_and_cannot_merge_it_read_only(agents, opened):
    assert TWIN_PYTHON.exists(), "run `uv sync` in integrations/github-twin"
    for name, role, expect in (("coder", "writer", "listed Smoke"), ("auditor", "readonly", "refused 403")):
        out = agents[name].run(str(TWIN_PYTHON), str(PYGITHUB_FLOW), opened["repo"], str(opened["number"]), role)
        assert expect in out.stdout, out


def test_live_each_request_is_a_step_of_its_agent_on_the_timeline(run, agents, opened):
    steps = [e for e in events(run) if e["kind"] == "tool_call" and e["payload"].get("name") == "http"]
    assert {e["payload"]["part"] for e in steps} == {"github"}
    assert {"coder", "auditor"} <= {e.get("agent") for e in steps}
    hosts = {e["payload"]["args"]["url"].split("/")[2] for e in steps}
    assert hosts == {agents["coder"].env["GH_HOST"]}, hosts
    audited = {(e["payload"]["args"]["method"], e["payload"]["result"]["status"]) for e in steps
               if e.get("agent") == "auditor"}
    assert ("PUT", 403) in audited, audited
