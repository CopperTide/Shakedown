"""Actions on real Docker: the fixture repo's tests run in `python:3.12-slim` (pinned by digest in
`images.json`) without network, memory-, CPU- and time-capped; a passing push succeeds, a failing
test fails the check with unittest's words in the log, and nothing is left behind."""
import json
import shutil
import subprocess
from pathlib import Path

import pytest

from conftest import actions_config, actions_twin, api, api_raw, drain, push_commit
from github_twin.actions.runner import LABEL, RealDocker

IMAGES = json.loads((Path(__file__).parent / "images.json").read_text())
R = "/api/v3/repos/acme/app"
FAILING = """import unittest

from app import add


class AddTest(unittest.TestCase):
    def test_add(self):
        self.assertEqual(add(2, 3), 6)
"""


def _docker(*args, timeout=600):
    return subprocess.run(["docker", *args], capture_output=True, text=True, timeout=timeout)


def _leftovers() -> list:
    return _docker("ps", "-aq", "--filter", f"label={LABEL}").stdout.split()


@pytest.fixture
def python_image():
    if shutil.which("docker") is None or _docker("info", timeout=30).returncode != 0:
        pytest.skip("Docker cannot be reached")
    image = IMAGES["python"]
    if _docker("image", "inspect", image["ref"]).returncode != 0:
        pulled = _docker(*image["fetch"].split()[1:])
        assert pulled.returncode == 0, pulled.stderr
    return image["ref"]


@pytest.fixture
def real_twin(tmp_path, python_image):
    cfg = actions_config(runners={"ubuntu-latest": python_image}, job_memory="256m", step_timeout_s=120)
    t = actions_twin(tmp_path, docker=RealDocker(), cfg=cfg)
    try:
        yield t
    finally:
        work = t.part.ctx("i").files.parent / "actions"
        t.part.stop("i")
        left = _leftovers()
        if left:
            _docker("rm", "-f", *left)
        assert not left and not work.exists(), (left, work)


def _log(t, run_id: int) -> str:
    [job] = api(t, "GET", f"{R}/actions/runs/{run_id}/jobs")["jobs"]
    return _job_log(t, job["id"])


@pytest.mark.live
def test_live_a_push_runs_the_repos_tests_in_docker(real_twin):
    t = real_twin
    push_commit(t, "main", "b.py", "B = 2\n")
    [run] = drain(t, timeout_s=180)
    assert run["conclusion"] == "success", _log(t, run["id"])
    assert "Ran 1 test" in _log(t, run["id"])
    sha = push_commit(t, "main", "test_app.py", FAILING, "Expect 6")
    last = max(drain(t, timeout_s=180), key=lambda r: r["id"])
    assert last["conclusion"] == "failure"
    log = _log(t, last["id"])
    assert "AssertionError: 5 != 6" in log and "FAILED (failures=1)" in log, log
    checks = api(t, "GET", f"{R}/commits/{sha}/check-runs")["check_runs"]
    assert [(c["name"], c["conclusion"]) for c in checks] == [("CI / test", "failure")]


BOUNDED = """name: Bounds
on: push
jobs:
  net:
    runs-on: ubuntu-latest
    steps:
      - run: python -c "import socket; socket.create_connection(('1.1.1.1', 53), 3)"
  slow:
    runs-on: ubuntu-latest
    steps:
      - run: sleep 60
"""


@pytest.mark.live
def test_live_a_job_has_no_network_and_a_step_past_its_time_is_removed(tmp_path, python_image):
    cfg = actions_config(runners={"ubuntu-latest": python_image}, job_memory="256m", step_timeout_s=3)
    t = actions_twin(tmp_path, docker=RealDocker(), cfg=cfg)
    try:
        push_commit(t, "main", ".github/workflows/ci.yml", BOUNDED)
        [run] = drain(t, timeout_s=120)
        net, slow = api(t, "GET", f"{R}/actions/runs/{run['id']}/jobs")["jobs"]
        assert (net["conclusion"], slow["conclusion"]) == ("failure", "failure")
        assert "OSError" in _job_log(t, net["id"]) and "the step timed out after 3.0 s" in _job_log(t, slow["id"])
    finally:
        t.part.stop("i")
        left = _leftovers()
        if left:
            _docker("rm", "-f", *left)
    assert not left


def _job_log(t, job_id: int) -> str:
    loc = api_raw(t, "GET", f"{R}/actions/jobs/{job_id}/logs").headers["location"]
    return api_raw(t, "GET", loc.split("github.acme.test", 1)[1]).body.decode()
