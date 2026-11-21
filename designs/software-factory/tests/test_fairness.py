"""Both designs get the same model, settings and tools; any difference comes from their design
files, and those carry no model settings, budgets or seeds."""

import json
import re
from pathlib import Path

from conftest import FakeDocker
from fake_model import FakeModel, FakeRun, say, tool_call
from factory_designs import solo_b, team_a
from factory_designs.settings import load_settings
from factory_designs.toolbox import TOOL_NAMES
from factory_designs.workbench import Workbench

ROOT = Path(__file__).resolve().parent.parent
FIXED_ELSEWHERE = {"provider", "model", "reasoning_effort", "max_tokens", "temperature", "seed",
                   "max_cost_usd", "world_minutes", "tool_output_chars", "command_timeout_s"}
SETTINGS_KEYS = ("model", "reasoning_effort", "temperature")


def _keys(d):
    if isinstance(d, dict):
        for k, v in d.items():
            yield k
            yield from _keys(v)


def _bench(tmp_path, settings):
    return lambda i, ep: Workbench(i, ep.env, tmp_path / "ws", tmp_path / "s.sock", settings, FakeDocker())  # noqa: E731


def _requests(tmp_path, settings):
    a_script = {"planner": [say(json.dumps({"items": [{"ticket": "ACME-1"}]}))],
                "coder": [tool_call("read_file", path="x"), say('{"pr": "p"}')],
                "reviewer": [say('{"verdict": "approve"}')], "release": [say("ok")]}
    with FakeModel(a_script) as fake:
        run = FakeRun(fake)
        team_a.build_graph(run, settings, _bench(tmp_path / "a", settings)).invoke(team_a.initial_state(run))
        a = list(fake.requests)
    with FakeModel({"engineer": [tool_call("read_file", path="x"), say("ok")]}) as fake:
        run = FakeRun(fake, world_minutes=None)
        solo_b.work(run, solo_b.build_agent(run, settings, _bench(tmp_path / "b", settings)), solo_b.load_design())
        b = list(fake.requests)
    return a, b


def _sent(body):
    cap = body.get("max_tokens", body.get("max_completion_tokens"))
    return tuple(body[k] for k in SETTINGS_KEYS) + (cap,)


def _tool_specs(requests):
    return {t["function"]["name"]: (t["function"]["description"], t["function"]["parameters"])
            for _, body in requests for t in body.get("tools", [])}


def test_designs_share_model_settings_and_tool_set(tmp_path):
    settings = load_settings()
    a_roles = team_a.load_design()["roles"].values()
    union = {t for r in a_roles for t in r["tools"]}
    b_tools = solo_b.load_design()["agent"]["tools"]
    assert union == set(b_tools) == set(TOOL_NAMES)
    a, b = _requests(tmp_path, settings)
    assert {_sent(body) for _, body in a} == {_sent(body) for _, body in b} == {
        (settings.model, settings.reasoning_effort, settings.temperature, settings.max_tokens)}
    a_specs, b_specs = _tool_specs(a), _tool_specs(b)
    assert set(a_specs) == set(b_specs) == set(TOOL_NAMES)
    for name in TOOL_NAMES:
        assert a_specs[name][0] == b_specs[name][0]
        assert a_specs[name][1]["properties"] == b_specs[name][1]["properties"]


def test_design_files_hold_no_model_settings_budgets_or_seeds():
    for design in (team_a.load_design(), solo_b.load_design()):
        assert FIXED_ELSEWHERE.isdisjoint(_keys(design))


def test_design_files_name_no_domain_data():
    words = [w for w in (ROOT / "tests" / "domain_words.txt").read_text().split("\n")
             if w and not w.startswith("#")]
    pattern = re.compile(r"\b(" + "|".join(words) + r")\b", re.I)
    files = [*ROOT.glob("factory_designs/*.py"), *ROOT.glob("team_a/*"), *ROOT.glob("solo_b/*"),
             ROOT / "shared.yaml"]
    hits = [f"{f.name}: {m.group(0)}" for f in files for m in pattern.finditer(f.read_text())]
    assert files and hits == []
