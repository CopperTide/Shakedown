"""shaked's HTTP API as the live software-factory tests use it: build the world, start an external run, score it."""
import json
import os
import time
import urllib.error
import urllib.request

SHAKED = os.environ.get("SHAKEDOWN_URL", "http://127.0.0.1:7700")


def api(method: str, path: str, body=None) -> dict:
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(f"{SHAKED}{path}", data=data, method=method,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            return json.loads(r.read() or b"{}")
    except urllib.error.HTTPError as e:
        raise AssertionError(f"{method} {path}: {e.code} {e.read().decode(errors='replace')}") from None


def wait_until(probe, timeout_s: float, what: str):
    end = time.monotonic() + timeout_s
    while time.monotonic() < end:
        found = probe()
        if found:
            return found
        time.sleep(1)
    raise AssertionError(f"timed out waiting for {what}")


def factory_world(choices: dict) -> dict:
    """The software-factory world shaked has built from `choices`, built now when it has none:
    `{name, ready_s (None when it was there already), state}`. Its name is the company's slug."""
    name = api("POST", "/api/templates/software-factory/preview", choices)["name"]
    if any(w["name"] == name for w in api("GET", "/api/worlds")):
        return {"name": name, "ready_s": None, "state": api("GET", f"/api/worlds/{name}")}
    started = time.monotonic()
    made = api("POST", "/api/worlds", {"template": "software-factory", "choices": choices})
    state = wait_until(lambda: (s := api("GET", f"/api/worlds/{made['name']}"))["state"] == "ready" and s, 120,
                       "the world to be ready")
    return {"name": made["name"], "ready_s": time.monotonic() - started, "state": state}


def external_run(world: str, task: str) -> dict:
    """An external run with no agents yet: the world starts and its clock ticks, and nobody works the task."""
    return api("POST", "/api/runs", {"benchmark": world, "task_id": task, "runner": "external",
                                     "team": {"id": "nobody", "name": "Nobody", "agents": []}})


def world_hours(run: dict) -> float:
    return api("GET", f"/api/branches/{run['branch_id']}/clock")["clock_ms"] / 3_600_000


def events(run: dict) -> list:
    return api("GET", f"/api/branches/{run['branch_id']}")["events"]


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
    return wait_until(lambda: checks_of(api("GET", f"/api/branches/{run['branch_id']}")), 300,
                      "the run to be scored")
