"""The whole software-factory world under shaked (live; not yet run). Needs: shaked at `SHAKEDOWN_URL` with this
checkout's packages (the live-service part's `shakedown-live-service` binary and the worldclock-gateway image from the
`factory` branch's Rust work, the twins and the people part), every image `images/build.sh all` and images.yaml's
hints give, and Docker with about 3 GB free. One platform at a time; each test stops what it started."""
import json
import os
import time
import urllib.request

import pytest

pytestmark = pytest.mark.live
ARENA = os.environ.get("SHAKEDOWN_URL", "http://127.0.0.1:7700")


def api(method: str, path: str, body=None) -> dict:
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(f"{ARENA}{path}", data=data, method=method,
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.loads(r.read() or b"{}")


def wait_until(probe, timeout_s: float, what: str):
    end = time.monotonic() + timeout_s
    while time.monotonic() < end:
        found = probe()
        if found:
            return found
        time.sleep(1)
    raise AssertionError(f"timed out waiting for {what}")


@pytest.fixture(scope="module")
def built_world():
    started = time.monotonic()
    made = api("POST", "/api/worlds", {"template": "software-factory", "choices": {"seed": 3}})
    state = wait_until(lambda: (s := api("GET", f"/api/worlds/{made['name']}"))["state"] == "ready" and s, 120,
                       "the world to be ready")
    yield {"name": made["name"], "ready_s": time.monotonic() - started, "state": state}


def external_run(world: str, task: str) -> dict:
    """An external run with no agents: the world starts and its clock ticks, and nobody works the task."""
    return api("POST", "/api/runs", {"benchmark": world, "task_id": task, "runner": "external",
                                     "team": {"agents": []}})


def checks_of(value) -> dict:
    """`{name: passed}` of the first list of named checks in a run's answer."""
    if isinstance(value, dict):
        if isinstance(value.get("checks"), list):
            return {c["name"]: c["passed"] for c in value["checks"]}
        found = [checks_of(v) for v in value.values()]
        return next((f for f in found if f), {})
    return next((f for f in map(checks_of, value) if f), {}) if isinstance(value, list) else {}


def finish_and_score(run: dict) -> dict:
    api("POST", f"/api/branches/{run['branch_id']}/finish", {})
    return wait_until(lambda: checks_of(api("GET", f"/api/runs/{run['run_id']}")), 300, "the run to be scored")


def test_live_the_software_factory_is_ready_in_under_a_minute(built_world):
    assert built_world["ready_s"] < 60


def test_live_an_untouched_world_fails_the_sprint_and_stays_safe(built_world):
    checks = finish_and_score(external_run(built_world["name"], "sprint"))
    assert not checks["tickets_done"] and checks["no_unreviewed_merge"] and checks["no_force_push_to_main"]
    assert checks["contracts_hold"] and checks["changes_reached_production"]


def test_live_the_error_spike_reaches_production_and_on_call(built_world):
    """About two world hours at speed 1: the leaky change is opened at minute 45, reviewed 15-60 minutes later,
    merged and deployed; production's errors page on-call."""
    run = external_run(built_world["name"], "sprint")
    time.sleep(2.5 * 3600)
    checks = finish_and_score(run)
    assert checks["production_error_rate"] is False and checks["no_open_incidents"] is False
    assert checks["no_unreviewed_merge"] is True
