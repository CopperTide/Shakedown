"""The real gh 2.45 reads the twin's Actions over HTTPS on 127.0.0.1 (the TLS front standing in for
shaked's proxy): `gh run list/view/watch/rerun` and `gh pr checks`, on runs a fake Docker answers
(`live`: needs the gh binary)."""
import json
import subprocess
import time

import pytest

from conftest import api, b64, wait_for
from test_clients import APP, GH, Agent, needs_gh

R = f"/api/v3/repos/{APP}"
CI_YML = """name: CI
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Tests
        run: python -m unittest -v
"""


def _completed(t, run_id: int) -> dict:
    return wait_for(lambda: (r := api(t, "GET", f"{R}/actions/runs/{run_id}"))["status"] == "completed" and r,
                    what=f"run {run_id}")


@pytest.fixture
def ran(front):
    """Run 1 (push to main) failed with a test failure in its log."""
    t = front.twin
    t.part.docker.answer("unittest", 1, "test_add (test_app.AddTest.test_add) ... FAIL\nFAILED (failures=1)\n")
    api(t, "PUT", f"{R}/contents/.github/workflows/ci.yml", {"message": "Add CI", "content": b64(CI_YML)})
    return _completed(t, 1)


@pytest.mark.live
@needs_gh
def test_live_gh_run_list_view_and_watch(front, ran, tmp_path):
    coder = Agent(front, tmp_path, "coder")
    listed = coder.gh("run", "list").stdout
    assert "\tAdd CI\tCI\tmain\tpush\t1\t" in listed, listed
    rows = json.loads(coder.gh("run", "list", "--json", "databaseId,status,conclusion,workflowName,headBranch,event").stdout)
    assert rows == [{"databaseId": 1, "status": "completed", "conclusion": "failure", "workflowName": "CI",
                     "headBranch": "main", "event": "push"}]
    view = coder.gh("run", "view", "1").stdout
    assert "X main CI · 1" in view and "  ✓ Run actions/checkout@v4\n  X Tests" in view, view
    assert f"View this run on GitHub: https://{coder.host}/{APP}/actions/runs/1" in view
    log = coder.gh("run", "view", "1", "--log").stdout
    assert "test\tTests\t" in log and "FAILED (failures=1)" in log, log
    failed = coder.gh("run", "view", "1", "--log-failed").stdout
    assert "FAILED (failures=1)" in failed and "Checked out" not in failed, failed
    job_id = api(front.twin, "GET", f"{R}/actions/runs/1/jobs")["jobs"][0]["id"]
    assert "FAILED (failures=1)" in coder.gh("run", "view", "--job", str(job_id), "--log").stdout
    done = coder.gh("run", "watch", "1", "--exit-status", check=False)
    assert done.returncode != 0 and "Run CI (1) has already completed with 'failure'" in done.stdout, done


@pytest.mark.live
@needs_gh
def test_live_gh_run_watch_follows_a_run_to_its_end(front, tmp_path):
    t, coder = front.twin, Agent(front, tmp_path, "coder")
    gate = t.part.docker.hold()
    api(t, "PUT", f"{R}/contents/.github/workflows/ci.yml", {"message": "Add CI", "content": b64(CI_YML)})
    wait_for(lambda: api(t, "GET", f"{R}/actions/runs/1")["status"] == "in_progress", what="run 1 to start")
    watch = subprocess.Popen([GH, "run", "watch", "1", "-i", "1", "--exit-status", "-R", f"{coder.host}/{APP}"],
                             env=coder.env, cwd=coder.cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        time.sleep(2)
        assert watch.poll() is None, watch.communicate()
        gate.set()
        out, err = watch.communicate(timeout=60)
    finally:
        watch.kill()
    assert watch.returncode == 0 and "Refreshing run status every 1 seconds" in out, (out, err)
    assert "✓ test in " in out and "  ✓ Tests" in out, out


@pytest.mark.live
@needs_gh
def test_live_gh_run_rerun_and_pr_checks(front, ran, tmp_path):
    coder = Agent(front, tmp_path, "coder")
    front.twin.part.docker.rules.clear()
    coder.gh("run", "rerun", "1")
    assert (_completed(front.twin, 1)["run_attempt"], _completed(front.twin, 1)["conclusion"]) == (2, "success")
    api(front.twin, "POST", f"{R}/git/refs", {"ref": "refs/heads/f", "sha": api(front.twin, "GET", f"{R}/branches/main")["commit"]["sha"]})
    api(front.twin, "PUT", f"{R}/contents/f.txt", {"message": "Add f", "content": b64("f\n"), "branch": "f"})
    coder.gh("pr", "create", "-t", "Add f", "-b", "B", "--head", "f")
    _completed(front.twin, 2)
    checks = coder.gh("pr", "checks", "1").stdout
    assert "CI / test\tpass" in checks, checks
