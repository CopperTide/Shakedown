"""Real clients against the twin over HTTPS on 127.0.0.1 (a TLS front with a test CA standing in for
arenad's proxy): PyGithub with `base_url`, and gh in GHES mode (`live`: needs the gh binary)."""
import shutil
import subprocess

import pytest
from github import Auth, Github, GithubException

from conftest import TOKEN
from tls_front import client_env

APP = "acme/app"


def pygithub(front, host: str) -> Github:
    return Github(base_url=f"https://{host}/api/v3", auth=Auth.Token(TOKEN), verify=str(front.ca), retry=None)


def test_pygithub_drives_a_pull_request_unchanged(front):
    coder = pygithub(front, front.listen("coder"))
    reviewer = pygithub(front, front.listen("reviewer"))
    try:
        repo = coder.get_repo(APP)
        repo.create_git_ref("refs/heads/f", repo.get_branch("main").commit.sha)
        repo.create_file("f.txt", "Add f", "f\n", branch="f")
        pr = repo.create_pull(title="F", body="Adds f", head="f", base="main")
        with pytest.raises(GithubException) as own:
            pr.create_review(event="APPROVE")
        assert own.value.status == 422
        reviewer.get_repo(APP).get_pull(pr.number).create_review(event="APPROVE", body="Good")
        assert pr.merge(merge_method="squash").merged is True
    finally:
        coder.close()
        reviewer.close()
    doc = front.twin.docs()[f"pull/{APP}/1"]
    assert (doc["merged"], doc["merged_by"], doc["reviewed_at_merge"], doc["approvals_at_merge"]) == (
        True, "coder", True, ["reviewer"])


GH = shutil.which("gh")
needs_gh = pytest.mark.skipif(GH is None, reason="the gh binary is not on PATH")


class Agent:
    """gh and git as one agent, through its own listener on the front."""

    def __init__(self, front, tmp_path, name: str, readonly: bool = False):
        self.host = front.listen(name, readonly)
        self.env = client_env(tmp_path / name, self.host, front.ca, TOKEN, name)
        self.cwd = tmp_path / name

    def run(self, *args, check=True) -> subprocess.CompletedProcess:
        r = subprocess.run(args, env=self.env, cwd=self.cwd, capture_output=True, text=True, timeout=120)
        assert not check or r.returncode == 0, (args, r.stdout, r.stderr)
        return r

    def gh(self, *args, check=True) -> subprocess.CompletedProcess:
        """`gh` on acme/app: `-R` names the host, as gh cannot match a remote whose URL has a port."""
        return self.run(GH, *args, "-R", f"{self.host}/{APP}", check=check)


@pytest.mark.live
@needs_gh
def test_live_gh_drives_a_pull_request_as_two_agents(front, tmp_path):
    print(subprocess.run([GH, "--version"], capture_output=True, text=True).stdout.splitlines()[0])
    coder, reviewer = Agent(front, tmp_path, "coder"), Agent(front, tmp_path, "reviewer")
    assert "Logged in to" in coder.run(GH, "auth", "status").stdout + coder.run(GH, "auth", "status").stderr
    coder.run(GH, "repo", "clone", APP, "app")  # GH_HOST names the host
    coder.cwd = coder.cwd / "app"
    coder.run("git", "checkout", "-q", "-b", "f")
    (coder.cwd / "f.txt").write_text("f\n")
    coder.run("git", "add", "f.txt")
    coder.run("git", "commit", "-q", "-m", "Add f")
    coder.run("git", "push", "-q", "-u", "origin", "f")
    url = coder.gh("pr", "create", "-t", "T", "-b", "B", "--head", "f").stdout.strip()
    assert url == f"https://{coder.host}/{APP}/pull/1"
    view = coder.gh("pr", "view", "1", "--json", "number,state,reviewDecision,statusCheckRollup,mergeable").stdout
    assert '"state":"OPEN"' in view and '"mergeable":"MERGEABLE"' in view
    assert "Can not approve your own pull request" in coder.gh("pr", "review", "1", "--approve", check=False).stderr
    reviewer.gh("pr", "review", "1", "--approve", "-b", "Good")
    coder.gh("pr", "merge", "1", "--squash", "--delete-branch")
    docs = front.twin.docs()
    doc = docs[f"pull/{APP}/1"]
    assert (doc["merged"], doc["merged_by"], doc["reviewed_at_merge"]) == (True, "coder", True)
    assert f"branch/{APP}/f" not in docs


@pytest.mark.live
@needs_gh
def test_live_a_read_only_agents_gh_reads_but_cannot_merge(front, tmp_path):
    coder, auditor = Agent(front, tmp_path, "coder"), Agent(front, tmp_path, "auditor", readonly=True)
    coder.run(GH, "repo", "clone", APP, "app")  # GH_HOST names the host
    coder.cwd = coder.cwd / "app"
    coder.run("git", "checkout", "-q", "-b", "f")
    (coder.cwd / "f.txt").write_text("f\n")
    coder.run("git", "add", "f.txt")
    coder.run("git", "commit", "-q", "-m", "Add f")
    coder.run("git", "push", "-q", "-u", "origin", "f")
    coder.gh("pr", "create", "-t", "T", "-b", "B", "--head", "f")
    assert "\tT\tf\tOPEN" in auditor.gh("pr", "list").stdout
    refused = auditor.gh("pr", "merge", "1", "--merge", check=False)
    assert refused.returncode != 0 and "Resource not accessible by integration" in refused.stderr
    assert front.twin.docs()[f"pull/{APP}/1"]["merged"] is False
