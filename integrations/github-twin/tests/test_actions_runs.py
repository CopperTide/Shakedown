"""Actions runs on a fake Docker: what starts a run, what its jobs run, the checks and statuses it
posts, deploys handed to the world, and a run caught by a save."""
import pytest

from conftest import (START_MS, actions_twin, advance_events, api, api_raw, create_pr, drain, push_commit, runs_of,
                      wait_for)

R = "/api/v3/repos/acme/app"
DEPLOY_WF = """name: Deploy
on:
  push:
    branches: [main]
jobs:
  ship:
    runs-on: ubuntu-latest
    steps:
      - uses: shakedown/build@v1
        with: {image: orders}
      - uses: shakedown/deploy@v1
        with: {service: prod-orders, image: orders}
"""
NEEDS_WF = """name: Chain
on: [push, pull_request]
jobs:
  a:
    runs-on: ubuntu-latest
    steps: [{run: make a}]
  b:
    needs: a
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    steps: [{run: make b}]
"""


def jobs(t, run_id, filter_="latest"):
    return api(t, "GET", f"{R}/actions/runs/{run_id}/jobs?filter={filter_}")["jobs"]


def checks(t, sha):
    return [(c["name"], c["conclusion"]) for c in api(t, "GET", f"{R}/commits/{sha}/check-runs")["check_runs"]]


def test_a_push_to_main_runs_the_workflow_and_posts_a_check(twin_fake_docker):
    t = twin_fake_docker
    sha = push_commit(t, "main", "b.py", "B = 2\n")
    [run] = drain(t)
    assert (run["name"], run["event"], run["head_sha"], run["conclusion"], run["run_number"]) == (
        "CI", "push", sha, "success", 1)
    assert checks(t, sha) == [("CI / test", "success")]
    status = api(t, "GET", f"{R}/commits/{sha}/status")
    assert [(s["context"], s["state"]) for s in status["statuses"]] == [("CI / test (push)", "success")]
    [argv] = [c for c in t.docker.calls if c[1] == "run"]
    assert argv[-4:] == ["base:1", "sh", "-euc", "python -m unittest -v"] and f"GITHUB_SHA={sha}" in argv


def test_a_push_to_another_branch_starts_no_push_run(twin_fake_docker):
    push_commit(twin_fake_docker, "feature", "b.py", "B = 2\n")
    assert runs_of(twin_fake_docker) == []


def test_a_failing_step_fails_the_job_keeps_its_log_and_tells_the_people(twin_fake_docker):
    t = twin_fake_docker
    t.docker.answer("unittest", 1, "FAIL: test_add\nFAILED (failures=1)\n")
    sha = push_commit(t, "main", "b.py", "B = 2\n")
    [run] = drain(t)
    assert run["conclusion"] == "failure" and checks(t, sha) == [("CI / test", "failure")]
    [job] = jobs(t, run["id"])
    assert [(s["name"], s["conclusion"]) for s in job["steps"]] == [
        ("Run actions/checkout@v4", "success"), ("Tests", "failure")]
    log = api_raw(t, "GET", f"{R}/actions/jobs/{job['id']}/logs")
    assert log.status == 302
    assert "FAILED (failures=1)" in api_raw(t, "GET", log.headers["location"].split("github.acme.test", 1)[1]).body.decode()
    kinds = [e["data"]["kind"] for e in advance_events(t) if e["kind"] == "notice"]
    assert "checks_completed" in kinds


def test_a_pull_request_runs_on_its_head(twin_fake_docker):
    t = twin_fake_docker
    sha = push_commit(t, "f", "b.py", "B = 2\n")
    assert create_pr(t, "f") == 1
    [run] = drain(t)
    assert (run["event"], run["head_branch"], run["head_sha"], [p["number"] for p in run["pull_requests"]]) == (
        "pull_request", "f", sha, [1])
    assert api(t, "GET", f"{R}/pulls/1")["mergeable_state"] == "clean"


def test_an_unsupported_workflow_fails_at_startup(twin_fake_docker):
    t = twin_fake_docker
    push_commit(t, "main", ".github/workflows/ci.yml",
                "on: push\njobs:\n  t:\n    runs-on: ubuntu-latest\n    strategy: {matrix: {a: [1]}}\n    steps: [{run: x}]\n")
    [run] = drain(t)
    assert (run["conclusion"], jobs(t, run["id"])) == ("startup_failure", [])


def test_a_job_waits_for_what_it_needs_and_its_condition(twin_fake_docker):
    t = twin_fake_docker
    push_commit(t, "main", ".github/workflows/ci.yml", NEEDS_WF)
    [on_main] = drain(t)
    assert [(j["name"], j["conclusion"]) for j in jobs(t, on_main["id"])] == [("a", "success"), ("b", "success")]
    push_commit(t, "f", "x.txt", "x\n")
    pr_run = [r for r in drain(t) if r["head_branch"] == "f"][0]
    assert [(j["name"], j["conclusion"]) for j in jobs(t, pr_run["id"])] == [("a", "success"), ("b", "skipped")]
    t.docker.answer("make a", 2, "boom\n")
    push_commit(t, "main", "y.txt", "y\n")
    last = max(drain(t), key=lambda r: r["id"])
    assert [(j["name"], j["conclusion"]) for j in jobs(t, last["id"])] == [("a", "failure"), ("b", "skipped")]


def _deploying(t):
    sha = push_commit(t, "main", ".github/workflows/deploy.yml", DEPLOY_WF)
    events = []
    wait_for(lambda: events.extend(e for e in advance_events(t) if e["kind"] == "deploy") or events, what="deploy")
    return sha, events[0]


def test_deploy_waits_for_its_result(twin_fake_docker):
    t = twin_fake_docker
    sha, ev = _deploying(t)
    ship = [r for r in runs_of(t) if r["name"] == "Deploy"][0]
    assert ev["data"] == {"service": "prod-orders", "image": f"shakedown/orders:{sha}", "sha": sha, "repo": "acme/app",
                          "run_id": ship["id"], "job_id": str(jobs(t, ship["id"])[0]["id"]), "by": "coder"}
    assert ship["status"] == "in_progress"
    [build] = [c for c in t.docker.calls if c[1] == "build"]
    assert build[:7] == ["docker", "build", "--pull=false", "--network", "none", "-t", f"shakedown/orders:{sha}"]
    out = t.part.call_tool("i", "github_act", {"person": "pipeline", "action": "deploy_result",
                                               "args": {"job_id": ev["data"]["job_id"], "ok": True}}, 0)
    assert "error" not in out, out
    runs = {r["name"]: r["conclusion"] for r in drain(t)}
    assert runs == {"CI": "success", "Deploy": "success"}


def test_a_failed_deploy_fails_its_job(twin_fake_docker):
    t = twin_fake_docker
    _, ev = _deploying(t)
    t.part.call_tool("i", "github_act", {"person": "pipeline", "action": "deploy_result",
                                         "args": {"job_id": ev["data"]["job_id"], "ok": False, "error": "unhealthy"}}, 0)
    ship = [r for r in drain(t) if r["name"] == "Deploy"][0]
    assert ship["conclusion"] == "failure"
    assert "deploy failed: unhealthy" in _job_log(t, jobs(t, ship["id"])[0]["id"])


def _job_log(t, job_id) -> str:
    loc = api_raw(t, "GET", f"{R}/actions/jobs/{job_id}/logs").headers["location"]
    return api_raw(t, "GET", loc.split("github.acme.test", 1)[1]).body.decode()


def test_deploy_times_out(twin_fake_docker):
    t = twin_fake_docker
    _, ev = _deploying(t)
    t.clock.ms += 299_000
    with pytest.raises(AssertionError):
        drain(t, timeout_s=0.5)
    t.clock.ms += 1_000
    ship = [r for r in drain(t) if r["name"] == "Deploy"][0]
    assert ship["conclusion"] == "failure"
    assert "deploy timed out after 300 s" in _job_log(t, jobs(t, ship["id"])[0]["id"])
    late = t.part.call_tool("i", "github_act", {"person": "pipeline", "action": "deploy_result",
                                                "args": {"job_id": ev["data"]["job_id"], "ok": True}}, 0)
    assert late["error"] == f"job {ev['data']['job_id']} is not waiting for a deploy"


def test_a_job_running_at_a_save_runs_again_after_restore(twin_fake_docker, saves_root):   # Review Focus 2
    t = twin_fake_docker
    gate = t.docker.hold()
    sha = push_commit(t, "main", "b.py", "B = 2\n")
    wait_for(lambda: any(c[1] == "run" for c in t.docker.calls), what="the job to start")
    t.part.save("i", "s1", str(saves_root / "github-twin" / "s1"))
    gate.set()
    drain(t)
    t.part.start("j", t.part.ctx("i").config, 1, 0, {"save": "s1", "mode": "exact"})
    try:
        [run] = drain(t, "j")
        assert (run["run_attempt"], run["conclusion"]) == (2, "success")
        tj = type(t)(t.part, "j")
        assert [j["conclusion"] for j in jobs(tj, run["id"], "all")] == ["cancelled", "success"]
        assert checks(tj, sha) == [("CI / test", "success")]
    finally:
        t.part.stop("j")


def test_cancel_stops_a_running_job(twin_fake_docker):
    t = twin_fake_docker
    t.docker.hold()
    push_commit(t, "main", "b.py", "B = 2\n")
    wait_for(lambda: any(c[1] == "run" for c in t.docker.calls), what="the job to start")
    [run] = runs_of(t)
    assert api_raw(t, "POST", f"{R}/actions/runs/{run['id']}/cancel").status == 202
    [run] = drain(t)
    assert run["conclusion"] == "cancelled"
    assert api_raw(t, "POST", f"{R}/actions/runs/{run['id']}/cancel").status == 409


def test_rerun_makes_a_new_attempt(twin_fake_docker):
    t = twin_fake_docker
    t.docker.answer("unittest", 1, "flaky\n")
    push_commit(t, "main", "b.py", "B = 2\n")
    [run] = drain(t)
    t.docker.rules.clear()
    assert api_raw(t, "POST", f"{R}/actions/runs/{run['id']}/rerun").status == 201
    [run] = drain(t)
    assert (run["run_attempt"], run["conclusion"]) == (2, "success")
    first = api(t, "GET", f"{R}/actions/runs/{run['id']}/attempts/1")
    assert (first["run_attempt"], first["conclusion"]) == (1, "failure")


def _history(tmp_path, name):
    t = actions_twin(tmp_path / name)
    try:
        t.docker.answer("unittest", 1, "FAILED\n")
        push_commit(t, "main", "b.py", "B = 2\n")
        drain(t)
        push_commit(t, "f", "c.py", "C = 3\n")
        create_pr(t, "f")
        drain(t)
        docs = t.docs()   # the clients' commits carry wall-clock dates, so shas differ between twins
        runs = {k: {f: x for f, x in v.items() if f != "head_sha"} for k, v in docs.items() if k.startswith("run/")}
        return {**runs, "checks": sorted(str(v) for k, v in docs.items() if k.startswith("check/"))}
    finally:
        t.part.stop("i")


def test_runs_are_deterministic(tmp_path):
    first, second = _history(tmp_path, "a"), _history(tmp_path, "b")
    assert first == second and len([k for k in first if k.startswith("run/")]) == 2
    assert first["run/acme/app/1"]["conclusion"] == "failure"


def test_runs_follow_world_time(twin_fake_docker):
    t = twin_fake_docker
    t.clock.ms = START_MS + 60_000
    push_commit(t, "main", "b.py", "B = 2\n")
    [run] = drain(t)
    assert run["created_at"] == "2026-03-10T06:01:00Z" and run["run_started_at"] == "2026-03-10T06:01:00Z"
