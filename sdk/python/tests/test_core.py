"""Integration tests against a real shaked (see conftest.py). They check the API contracts the SDK
relies on by driving the API directly; no agent, scripted or otherwise, takes part."""

from __future__ import annotations

import json
import os
import threading

import httpx
import pytest

from shakedown import BadRequest, NotFound, NotSupported
from shakedown.env import END, ShakedownParallelEnv
from shakedown.export import read_jsonl

pytestmark = pytest.mark.core


def _read_tool(branch, agent) -> str:
    """A read tool of the world (calling it with no arguments is recorded, error or not)."""
    effects = branch.record().run_start.get("tools", {})
    names = [t.name for t in agent.tools if effects.get(t.name, {}).get("effect") == "read"]
    assert names, "the world has no read tool"
    return names[0]


def test_core_describes_itself(shakedown):
    h = shakedown.health()
    assert "benchmark" in h and "providers" in h
    b = shakedown.benchmark()
    assert b.name and b.tasks
    tasks = shakedown.tasks()
    assert tasks[0].ticket and tasks[0].instructions
    assert shakedown.task(tasks[0].id).id == tasks[0].id
    assert shakedown.default_team(model="haiku").agents
    assert all(a.model == "haiku" for a in shakedown.default_team(model="haiku").agents)
    assert shakedown.world()
    assert isinstance(shakedown.supports_pause(), bool)


def test_bad_requests_are_typed(shakedown):
    with pytest.raises((NotFound, BadRequest)):
        shakedown.external_run("no-such-task")
    with pytest.raises(BadRequest):
        shakedown.external_run(shakedown.tasks()[0].id, seed=2**60)
    with pytest.raises(NotFound):
        shakedown.branch("no-such-branch")


@pytest.fixture(scope="module")
def ended(shakedown):
    """An external run with one agent that made one read call, finished; deleted afterwards."""
    branch = shakedown.external_run(shakedown.tasks()[0], agents=["probe"], seed=7, name="sdk test")
    agent = branch.agents["probe"]
    tool = _read_tool(branch, agent)
    agent.call(tool, {})
    agent.end(ok=True)
    live = branch.record()
    result = branch.finish()
    yield {"branch": branch, "agent": agent, "tool": tool, "live": live, "result": result}
    shakedown.delete_run(branch.run_id)


def test_external_run_endpoints_and_record(shakedown, ended):
    branch, agent, live = ended["branch"], ended["agent"], ended["live"]
    assert agent.mcp_url.endswith(f"/mcp/{branch.id}/probe") and "/gateway/" in agent.gateway_url
    assert agent.mcp_config()["mcpServers"]["shakedown"]["url"] == agent.mcp_url
    assert {"handoff", "message", "inbox"} <= {t.name for t in agent.tools}
    assert live.status == "running" and live.seed == 7 and live.run_start["runner"] == "external"
    assert [c.tool for c in live.tool_calls("probe")] == [ended["tool"]]
    assert any(r.id == branch.run_id and r.name == "sdk test" for r in shakedown.runs())
    assert ended["result"]["status"] in ("passed", "failed")
    check = branch.check()
    assert check is not None and check.checks


def test_stream_of_an_ended_branch(ended):
    branch = ended["branch"]
    seen = list(branch.stream())
    assert [e.seq for e in seen] == [e.seq for e in branch.record().events]
    assert branch.final["status"] == ended["result"]["status"]


def test_fork_restore_plan_and_compare(shakedown, ended):
    branch = ended["branch"]
    at = branch.record().tool_calls("probe")[0].seq
    assert branch.restore_plan(at)["parts"]
    fork = branch.fork(at)  # same run: deleted with it
    assert fork.fork_info.start_seq == at and "probe" in fork.agents
    assert all(e.seq < at for e in fork.agents["probe"].history)
    assert fork.record().status == "running"
    fork.finish()
    assert "first_divergent_seq" in shakedown.compare(branch, fork)


def test_export_dataset(ended, tmp_path):
    branch = ended["branch"]
    out = branch.export(str(tmp_path / "ds"))
    rows = read_jsonl(os.path.join(out, "branches.jsonl"))
    events = read_jsonl(os.path.join(out, "events.jsonl"))
    assert rows[0]["branch_id"] == branch.id and rows[0]["seed"] == 7
    assert rows[0]["events"] == len(events) and events[0]["kind"] == "run_start"
    assert "shakedown-dataset/1" in open(os.path.join(out, "SCHEMA.md")).read()


def test_live_stream_follows_a_running_branch(shakedown, cleanup):
    branch = shakedown.external_run(shakedown.tasks()[0], agents=["probe"])
    cleanup.append(branch.run_id)
    got: list = []
    t = threading.Thread(target=lambda: got.extend(branch.stream()))
    t.start()
    agent = branch.agents["probe"]
    agent.call(_read_tool(branch, agent), {})
    agent.end()
    branch.finish()
    t.join(60)
    assert not t.is_alive()
    kinds = [e.kind for e in got]
    assert kinds[0] == "run_start" and "tool_call" in kinds and kinds[-1] == "check"
    assert [e.seq for e in got] == sorted(e.seq for e in got)
    assert branch.final["status"] in ("passed", "failed")


def test_pause_is_feature_detected(shakedown, cleanup):
    branch = shakedown.external_run(shakedown.tasks()[0], agents=["probe"])
    cleanup.append(branch.run_id)
    try:
        if shakedown.supports_pause():
            assert branch.pause().get("paused") is True
            assert branch.record().paused is True
            assert branch.resume().get("paused") is False
        else:
            with pytest.raises(NotSupported):
                branch.pause()
    finally:
        branch.finish()


def test_cors_origin_is_configurable(shakedown):
    if not hasattr(shakedown, "process"):
        pytest.skip("only for a shaked the tests started")
    ok = httpx.options(f"{shakedown.url}/api/runs", headers={
        "origin": "http://127.0.0.1:5173", "access-control-request-method": "POST"})
    assert ok.status_code == 204
    assert ok.headers["access-control-allow-origin"] == "http://127.0.0.1:5173"
    other = httpx.get(f"{shakedown.url}/api/health", headers={"origin": "http://evil.example"})
    assert "access-control-allow-origin" not in other.headers


def test_step_env_contract(shakedown, cleanup):
    env = ShakedownParallelEnv(shakedown, shakedown.tasks()[0].id, agents=["probe"], max_steps=5)
    obs, infos = env.reset(seed=11)
    cleanup.append(env.branch.run_id)
    assert env.agents == ["probe"] and infos["probe"]["seed"] == 11
    assert obs["probe"]["ticket"] and obs["probe"]["tools"]
    tool = _read_tool(env.branch, env.branch.agents["probe"])
    obs, rewards, terms, truncs, infos = env.step({"probe": {"name": tool, "arguments": {}}})
    assert obs["probe"]["tool_result"] is not None and env.agents == ["probe"]
    assert rewards == {"probe": 0.0} and not terms["probe"] and not truncs["probe"]
    obs, rewards, terms, truncs, infos = env.step({"probe": END})
    assert env.agents == [] and terms["probe"] and infos["probe"]["check"] is not None
    assert rewards["probe"] in (0.0, 1.0)
    calls = env.branch.record().tool_calls("probe")
    env2, obs2, infos2 = env.fork(calls[0].seq)
    assert env2.agents == ["probe"] and "history" in obs2["probe"]
    env2.close()
    assert env2.branch.record().status in ("passed", "failed")


def test_external_suite_claim_and_export(shakedown, cleanup, tmp_path):
    task = shakedown.tasks()[0].id
    suite = shakedown.suite(team=[], tasks=[task], repeats=1, runner="external", name="sdk suite")
    branch = suite.claim()
    assert branch is not None and branch.task_id == task
    cleanup.append(branch.run_id)
    branch.register("probe").end()
    branch.finish()
    assert suite.claim() is None
    report = suite.refresh()
    assert report["summary"]["runs"] == 1 and report["summary"]["ended"] == 1
    assert suite.compare(suite)["verdict"] in ("same", "incomparable")
    out = suite.export(str(tmp_path / "suite"))
    assert json.load(open(os.path.join(out, "suite.json")))["id"] == suite.id
    assert len(read_jsonl(os.path.join(out, "branches.jsonl"))) == 1


def _live_model(shakedown):
    """A real model to drive an agent with: a provider shaked's gateway can reach, else Claude Code
    signed in on this machine. Skips, saying why, with neither."""
    from shakedown import claude_code

    models = {"openai": "gpt-4.1-mini", "anthropic": "claude-haiku-4-5",
              "openrouter": "anthropic/claude-haiku-4.5"}
    for p in shakedown.providers():
        if p.get("available") and p["id"] in models:
            return p["id"], models[p["id"]]
    if claude_code.available():
        return "claude-code", "haiku"
    pytest.skip("no model: no provider key in shaked's environment and no `claude` on PATH")


def test_driver_round_trip(shakedown, monkeypatch):
    """A driver starts a run, a real model works the ticket as one agent, and the run is scored."""
    from shakedown import claude_code, driver

    via, model = _live_model(shakedown)
    for k in ("SHAKEDOWN_BRANCH", "SHAKEDOWN_RUN", "SHAKEDOWN_TASK", "SHAKEDOWN_FORK_OF"):
        monkeypatch.delenv(k, raising=False)
    with driver.run(task=shakedown.tasks()[0].id, url=shakedown.url) as run:
        if via == "claude-code":
            agent = run.agent("agent", model=model)
            res = claude_code.run(agent, run.ticket, model=model, max_turns=8)
            assert res.ok, res
        else:
            agent = run.agent("agent", provider=via, model=model)
            with httpx.Client(base_url=agent.gateway_url, timeout=300) as llm:
                msgs = [{"role": "user", "content": run.ticket}]
                r = llm.post("/chat/completions", json={"model": model, "messages": msgs})
                r.raise_for_status()
        branch = run.branch
    try:
        rec = shakedown.branch(branch.id).record()
        assert rec.status in ("passed", "failed"), rec.status
        assert any(e.kind == "check" for e in rec.events)
        assert any(e.kind == "agent_end" and e.agent == "agent" for e in rec.events)
        calls = [e for e in rec.events if e.kind == "model_call" and e.agent == "agent"]
        assert calls, [e.summary() for e in rec.events]
    finally:
        shakedown.delete_run(branch.run_id)


def test_driver_that_raises_ends_the_run_failed_and_scored(shakedown, monkeypatch):
    from shakedown import driver

    for k in ("SHAKEDOWN_BRANCH", "SHAKEDOWN_RUN", "SHAKEDOWN_TASK", "SHAKEDOWN_FORK_OF"):
        monkeypatch.delenv(k, raising=False)
    with pytest.raises(RuntimeError):
        with driver.run(task=shakedown.tasks()[0].id, url=shakedown.url) as run:
            run.agent("agent")
            branch = run.branch
            raise RuntimeError("the graph broke")
    try:
        rec = shakedown.branch(branch.id).record()
        assert (rec.status, rec.error) == ("failed", "driver raised RuntimeError: the graph broke")
        assert any(e.kind == "check" for e in rec.events)
    finally:
        shakedown.delete_run(branch.run_id)
