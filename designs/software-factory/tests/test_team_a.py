import json

import pytest

from conftest import FakeDocker
from fake_model import FakeModel, FakeRun, say, tool_call
from factory_designs.prompts import load_prompt
from factory_designs.team_a import DESIGN, build_graph, initial_state, load_design, parse_answer
from factory_designs.workbench import Workbench

ITEM = {"ticket": "ACME-1", "repo": "acme/app", "summary": "Add a field", "risks": [], "contracts": []}
PR = "https://github.acme.test/acme/app/pull/1"


def plan(*items):
    return say(json.dumps({"items": list(items) or [ITEM]}), reasoning="Two services share it.")


def verdict(v):
    return say(f'Looked at it.\n```json\n{{"verdict": "{v}", "pr": "{PR}", "notes": "n"}}\n```')


SCRIPT = {
    "planner": [tool_call("shell", cmd="gh issue list"), plan()],
    "coder": [tool_call("write_file", path="app/x.py", text="x = 1\n"), say(json.dumps({"pr": PR, "notes": "ok"}))],
    "reviewer": [verdict("approve")],
    "release": [say("Merged and watched.")],
}


def run_team(tmp_path, script, settings, **run_kw):
    docker = FakeDocker()
    with FakeModel(script) as fake:
        run = FakeRun(fake, **run_kw)
        bench = lambda role, ep: Workbench(role, ep.env, tmp_path / "ws", tmp_path / "s.sock", settings, docker)  # noqa: E731
        build_graph(run, settings, bench).invoke(initial_state(run))
    return fake, run, docker


def tool_names(body):
    return [t["function"]["name"] for t in body.get("tools", [])]


def test_roles_tools_and_read_only_follow_the_design_file(tmp_path, settings):
    fake, run, _ = run_team(tmp_path, SCRIPT, settings)
    design = load_design()
    assert run.registered["planner"]["read_only"] is True
    for role, spec in design["roles"].items():
        assert run.registered[role]["read_only"] is spec["read_only"]
        assert run.registered[role]["provider"] == settings.provider
        assert {tuple(tool_names(b)) for b in fake.bodies(role)} == {tuple(spec["tools"])}


def test_one_item_goes_plan_code_review_release_monitor(tmp_path, settings):
    fake, run, docker = run_team(tmp_path, SCRIPT, settings)
    assert fake.starts() == ["planner", "coder", "reviewer", "release", "release"]
    assert (tmp_path / "ws" / "coder" / "app" / "x.py").read_text() == "x = 1\n"
    assert docker.commands == ["gh issue list"]


def test_two_items_are_worked_in_order(tmp_path, settings):
    second = {**ITEM, "ticket": "ACME-2"}
    script = {**SCRIPT, "planner": [plan(ITEM, second)]}
    fake, _, _ = run_team(tmp_path, script, settings)
    assert fake.starts() == ["planner"] + ["coder", "reviewer", "release"] * 2 + ["release"]
    coder_inputs = [b["messages"][1]["content"] for b in fake.bodies("coder") if len(b["messages"]) == 2]
    assert "ACME-1" in coder_inputs[0] and "ACME-2" in coder_inputs[1]


def test_changes_requested_loops_back_at_most_twice(tmp_path, settings):
    script = {**SCRIPT, "coder": [say(json.dumps({"pr": PR, "notes": "ok"}))],
              "reviewer": [verdict("changes")]}
    fake, run, _ = run_team(tmp_path, script, settings)
    assert fake.starts() == ["planner"] + ["coder", "reviewer"] * 3 + ["release"]
    monitor_input = fake.bodies("release")[0]["messages"][1]["content"]
    assert "review rounds used up" in monitor_input


def test_a_rejected_item_is_not_released(tmp_path, settings):
    fake, _, _ = run_team(tmp_path, {**SCRIPT, "reviewer": [verdict("reject")]}, settings)
    assert fake.starts() == ["planner", "coder", "reviewer", "release"]
    assert "monitor" in fake.bodies("release")[0]["messages"][1]["content"]


def test_a_plan_that_is_not_json_becomes_one_item(tmp_path, settings):
    script = {**SCRIPT, "planner": [say("We should fix the checkout."), say("Still prose.")]}
    fake, run, _ = run_team(tmp_path, script, settings, ticket="Ship the acme sprint.")
    planner = fake.bodies("planner")
    assert len(planner) == 2
    assert planner[1]["messages"][-1] == {"role": "user", "content": "Answer with the JSON plan only."}
    assert fake.starts() == ["planner", "coder", "reviewer", "release", "release"]
    assert "Ship the acme sprint." in fake.bodies("coder")[0]["messages"][1]["content"]
    plans = [json.loads(t) for a, k, t in run.reasoning if k == "plan"]
    assert len(plans[0]["items"]) == 1 and plans[0]["items"][0]["summary"] == "Ship the acme sprint."


def test_the_corrective_turn_can_save_the_plan(tmp_path, settings):
    fake, run, _ = run_team(tmp_path, {**SCRIPT, "planner": [say("prose"), plan()]}, settings)
    plans = [json.loads(t) for a, k, t in run.reasoning if k == "plan"]
    assert plans == [{"items": [ITEM]}]


def test_requests_carry_the_shared_settings(tmp_path, settings):
    fake, _, _ = run_team(tmp_path, SCRIPT, settings)
    for _, body in fake.requests:
        assert body["model"] == settings.model
        assert body["reasoning_effort"] == "medium"
        assert body.get("max_tokens", body.get("max_completion_tokens")) == 4096
        assert body["temperature"] == 1.0
        assert body.get("stream") is not True


def test_prompts_come_from_the_files(tmp_path, settings):
    fake, _, _ = run_team(tmp_path, SCRIPT, settings, world_minutes=60.0)
    roles = ", ".join(load_design()["roles"])
    for role in ("planner", "coder", "reviewer", "release"):
        want = load_prompt(DESIGN / f"{role}.md", world_minutes="60", role_ids=roles)
        assert fake.bodies(role)[0]["messages"][0] == {"role": "system", "content": want}


def test_the_plan_is_reported_as_reasoning(tmp_path, settings):
    _, run, _ = run_team(tmp_path, SCRIPT, settings)
    assert ("planner", "plan", json.dumps({"items": [ITEM]}, indent=2)) in run.reasoning
    steps = [(a, t) for a, k, t in run.reasoning if k == "step"]
    assert steps == [("reviewer", "approve: n")]


@pytest.mark.parametrize("text,want", [
    ('{"pr": "a"} then {"pr": "b", "notes": 1}', {"pr": "b", "notes": 1}),
    ('```json\n{"verdict": "approve", "x": {"y": 1}}\n```', {"verdict": "approve", "x": {"y": 1}}),
    ('{"other": 1}', None),
    ("no json {", None),
])
def test_parse_answer_reads_the_last_object_with_the_keys(text, want):
    keys = {"verdict"} if "verdict" in text else {"pr"}
    assert parse_answer(text, keys) == want
