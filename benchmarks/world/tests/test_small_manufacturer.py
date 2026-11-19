"""The small manufacturer template: it validates and builds with its defaults, and its checks score the
factory-01-erp part as Factory-01's own checks do (gold end state: every check passes; untouched: the goals
fail and the safety checks hold). The part and Factory-01's gold actions are the real programs, run over the
line protocol with this package's Python (both use only the kit)."""
import json
import subprocess
import sys
from pathlib import Path

import pytest

from arena_part.expect import evaluate
from worldbench.generate import generate
from worldbench.template import load_template

ARENA = Path(__file__).resolve().parents[3]
TEMPLATE = Path(__file__).resolve().parents[1] / "library" / "small-manufacturer"
ERP_DIR = ARENA / "integrations" / "factory-01-erp"
FACTORY01_DIR = ARENA / "benchmarks" / "factory-01"


class Program:
    """A part or benchmark program run as `python -m <module>` in its folder, called line by line."""

    def __init__(self, module: str, cwd: Path, saves: Path):
        env = {"PATH": "/usr/bin:/bin", "ARENA_SAVES": str(saves)}
        self.p = subprocess.Popen([sys.executable, "-m", module], cwd=cwd, env=env, text=True,
                                  stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        self.n = 0

    def call(self, method: str, params: dict):
        self.n += 1
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params}) + "\n")
        self.p.stdin.flush()
        reply = json.loads(self.p.stdout.readline())
        assert "error" not in reply, reply
        return reply["result"]

    def close(self):
        self.p.stdin.close()
        self.p.wait(timeout=10)


@pytest.fixture
def world():
    r = generate(load_template(TEMPLATE), {})
    assert r.problems == []
    return r.world


def _erp_docs(world: dict, tmp_path: Path, actions: list) -> dict:
    """The ERP's docs after the task's setup (on the ERP) and `actions`."""
    erp = next(p for p in world["parts"] if p["part"] == "erp")
    part = Program("factory01_erp", ERP_DIR, tmp_path / "saves")
    try:
        part.call("start", {"instance": "i", "config": erp["config"], "seed": 0, "clock_ms": 0})
        calls = [(c["tool"], c["args"]) for c in world["tasks"][0]["setup"] if c["part"] == "erp"]
        for name, args in calls + [(a["name"], a["args"]) for a in actions]:
            out = part.call("call_tool", {"instance": "i", "name": name, "args": args, "clock_ms": 0})
            assert "error" not in out, out
        return part.call("inspect", {"target": {"instance": "i"}, "keys": []})["docs"]
    finally:
        part.close()


@pytest.fixture
def factory01_gold_actions(tmp_path) -> list:
    bench = Program("factory01", FACTORY01_DIR, tmp_path / "bench-saves")
    try:
        return bench.call("gold_actions", {"task": "supplier-delay"})["actions"]
    finally:
        bench.close()


def _passed(world: dict, docs: dict) -> dict:
    return {c["name"]: c["passed"] for c in evaluate(world["tasks"][0]["checks"], {"erp": docs})}


def test_the_template_validates_and_builds_with_defaults(world):
    assert [p["integration"] for p in world["parts"]] == ["factory-01-erp", "shift-roster"]
    assert [t["id"] for t in world["tasks"]] == ["supplier-delay"]
    roles = {p["role"] for p in world["people"]}
    assert roles == {"operator", "supervisor"}


def test_its_checks_pass_on_the_factory01_gold_end_state(world, factory01_gold_actions, tmp_path):
    names = _passed(world, _erp_docs(world, tmp_path, factory01_gold_actions))
    assert names == {"flagged_for_human": True, "free_stock_secured": True, "no_silent_slip": True,
                     "no_split_purchases": True, "no_negative_inventory": True, "reserved_stock_respected": True}


def test_an_untouched_world_fails_the_goals_and_keeps_the_safety_checks(world, tmp_path):
    names = _passed(world, _erp_docs(world, tmp_path, []))
    assert names == {"flagged_for_human": False, "free_stock_secured": False, "no_silent_slip": False,
                     "no_split_purchases": True, "no_negative_inventory": True, "reserved_stock_respected": True}


def test_the_three_safety_checks_say_so(world):
    safety = sorted(c["name"] for c in world["tasks"][0]["checks"] if c.get("safety"))
    assert safety == ["no_negative_inventory", "no_split_purchases", "reserved_stock_respected"]


def test_operators_staff_the_roster_with_their_fields(world):
    roster = next(p for p in world["parts"] if p["part"] == "roster")["config"]
    ops = [p for p in world["people"] if p["role"] == "operator"]
    assert [o["id"] for o in roster["operators"]] == [o["id"] for o in ops]
    assert all({"station", "certifications"} <= set(o) for o in roster["operators"])


def test_without_the_roster_the_operators_and_the_callout_are_dropped():
    r = generate(load_template(TEMPLATE), {"systems": ["erp"], "events": {"operator-callout": {"on": True}}})
    assert r.problems == [] and [p["part"] for p in r.world["parts"]] == ["erp"]
    assert r.world["people"] == [] and [e["id"] for e in r.world["events"]] == ["supplier-delay"]
