"""Unit tests: parsing, models, errors, export rows. No shaked needed."""

import json

import pytest

from shakedown import Agent, Change, Disruption, Scenario, Team
from shakedown.errors import BadRequest, Conflict, DiskFull, NotFound, from_status, ApiError
from shakedown.export import branch_row, event_rows
from shakedown.models import BranchRecord, CheckResult, Event, as_scenario, as_team
from shakedown.sse import parse
from shakedown.mcp import Tool, ToolResult
from shakedown.env import _messages_for, _parse_action, END
from shakedown.client import Branch, Suite, _poll, wait_review


def test_sse_parse_events_comments_and_multiline():
    lines = [
        ": keep-alive",
        "event: event",
        'data: {"seq": 1, "kind": "run_start"}',
        "",
        "event: status",
        "data: {",
        'data: "status": "passed"}',
        "",
        "data: tail",
    ]
    got = list(parse(lines))
    assert got == [("event", {"seq": 1, "kind": "run_start"}), ("status", {"status": "passed"}),
                   ("message", "tail")]


def test_agent_and_team_dicts():
    a = Agent.api("solver", provider="openai", model="gpt-x", prompt="p", read_only=True)
    d = a.to_dict()
    assert d == {"id": "solver", "name": "solver", "kind": "open", "model": "gpt-x", "prompt": "p",
                 "read_only": True, "provider": "openai"}
    assert Agent.from_dict(d) == Agent("solver", "open", "gpt-x", "p", "solver", "openai", True)
    t = Team(agents=[Agent.claude_code("a"), Agent.codex("b")]).with_all(model="haiku")
    assert [x.model for x in t.agents] == ["haiku", "haiku"]
    assert as_team([Agent.external("x")])["agents"][0]["kind"] == "external"
    assert as_team(None) == {"id": "team", "name": "Team", "agents": []}


def test_scenarios_and_changes():
    assert as_scenario(["lane", Disruption("cap", {"after": 2})]) == {
        "disruptions": [{"id": "lane"}, {"id": "cap", "params": {"after": 2}}]
    }
    assert as_scenario(Scenario([Disruption("x")])) == {"disruptions": [{"id": "x"}]}
    assert as_scenario(None) is None
    assert Change(model="", prompt="new").to_dict() == {"model": "", "prompt": "new"}


def test_check_result_named_and_gold():
    c = CheckResult.from_payload({"passed": False, "checks": [
        {"name": "released", "passed": True},
        {"name": "shipped", "passed": False, "detail": "no shipment"},
        {"name": "no_unapproved_write", "passed": False, "safety": True},
    ], "differing_rows": ["shipped"]})
    assert c.score == 0.5
    assert [v.name for v in c.safety_violations] == ["no_unapproved_write"]
    g = CheckResult.from_payload({"passed": True, "differing_rows": []})
    assert [x.name for x in g.checks] == ["matches_gold"] and g.score == 1.0


def test_errors_by_status():
    assert isinstance(from_status(400, "x"), BadRequest)
    assert isinstance(from_status(404, "x"), NotFound)
    assert isinstance(from_status(409, "x"), Conflict)
    assert isinstance(from_status(507, "x"), DiskFull)
    e = from_status(500, "boom", "GET", "/api/x")
    assert type(e) is ApiError and e.status == 500 and "GET /api/x" in str(e)


def _record(start_seq=None):
    return BranchRecord.from_dict({
        "id": "b2", "run_id": "r1", "status": "failed", "error": None, "parent_id": "b1" if start_seq else None,
        "start_seq": start_seq, "team": {"id": "t", "name": "t", "agents": [{"id": "a", "kind": "external"}]},
        "created_at": "2026-11-07T23:17:50.759274Z", "ended_at": "2026-11-07T23:17:58Z",
        "restore_mode": "exact" if start_seq else None, "restored": [], "fork_note": None,
        "metrics": {"measured_s": 7.8},
        "events": [
            {"branch_id": "b2", "seq": 1, "at": "2026-11-07T23:17:51Z", "agent": None, "kind": "run_start",
             "payload": {"task_id": "72", "seed": 42, "benchmark": "retail", "runner": "external"}},
            {"branch_id": "b2", "seq": 2, "at": "2026-11-07T23:17:52Z", "agent": "a", "kind": "tool_call",
             "payload": {"name": "get_user_details", "args": {"user_id": "u"}, "error": "User not found",
                         "writes": []}},
            {"branch_id": "b2", "seq": 3, "at": "2026-11-07T23:17:57Z", "agent": None, "kind": "check",
             "payload": {"passed": False, "differing_rows": ["orders/#W1"]}},
            {"branch_id": "b2", "seq": 4, "at": "2026-11-07T23:17:58Z", "agent": None, "kind": "pause",
             "payload": {}},
        ],
    })


def test_branch_record_and_export_rows():
    rec = _record(start_seq=3)
    assert rec.seed == 42 and rec.ended and rec.check.differing_rows == ["orders/#W1"]
    assert rec.tool_calls("a")[0].tool == "get_user_details"
    row = branch_row(rec)
    assert row["format"] == "shakedown-dataset/1" and row["task_id"] == "72" and row["passed"] is False
    assert row["checks"] == [{"name": "matches_gold", "passed": False, "detail": "", "safety": False}]
    rows = list(event_rows(rec))
    assert [r["inherited"] for r in rows] == [True, True, False, False]
    json.dumps(rows)  # serialisable
    assert "get_user_details" in rec.events[1].summary()
    assert rec.events[3].summary().strip().endswith("pause")  # unknown kinds are kept


def test_tools_and_results():
    t = Tool.from_dict({"name": "n", "description": "d", "inputSchema": {"type": "object"}})
    assert t.openai_schema()["function"]["parameters"] == {"type": "object"}
    assert t.anthropic_schema()["input_schema"] == {"type": "object"}
    assert ToolResult('{"a": 1}', False).value == {"a": 1}
    assert ToolResult("plain", True).value == "plain"


def test_env_helpers():
    assert _parse_action({"name": "x", "arguments": {"a": 1}}) == ("x", {"a": 1})
    assert _parse_action(("x", None)) == ("x", {})
    with pytest.raises(ValueError):
        _parse_action("x")
    assert END == "__end__"
    ev = [Event("b", 5, None, "a", "message", {"to": "b", "text": "hi"}),
          Event("b", 6, None, "c", "message", {"to": "*", "text": "all"}),
          Event("b", 7, None, "a", "handoff", {"note": "n"}),
          Event("b", 8, None, "b", "message", {"to": "*", "text": "own"})]
    assert [m["text"] for m in _messages_for("b", ev)] == ["hi", "all", "n"]


def test_wait_review_follows_a_running_review_to_its_end():
    answers = iter([{"status": "running"}, {"status": "done", "review": {"verdict": "v"}}])
    slept = []
    r = wait_review(lambda: next(answers), {"status": "running", "key": "branch:b"}, 60, 1.5, slept.append)
    assert r["review"] == {"verdict": "v"} and slept == [1.5, 1.5]
    assert wait_review(lambda: None, {"status": "done"}, 60, 1, slept.append) == {"status": "done"}
    with pytest.raises(TimeoutError):
        wait_review(lambda: {"status": "running"}, {"status": "running"}, -1, 0, lambda _: None)



def test_poll_fetches_until_done_or_the_deadline():
    answers, slept = iter([1, 2, 3]), []
    assert _poll(lambda: next(answers), lambda n: n >= 3, None, 0.5, str, sleep=slept.append) == 3
    assert slept == [0.5, 0.5]
    assert _poll(lambda: 9, lambda n: n > 0, 60, 1, str, first=5, sleep=slept.append) == 5
    with pytest.raises(TimeoutError, match="^still 0$"):
        _poll(lambda: 0, bool, -1, 0, lambda n: f"still {n}", sleep=lambda _: None)


class _Answers:
    """An arena whose GETs answer `bodies` in turn (the last one again once they run out)."""

    def __init__(self, *bodies):
        self.bodies, self.paths = list(bodies), []

    def get(self, path, **params):
        self.paths.append(path)
        return self.bodies.pop(0) if len(self.bodies) > 1 else self.bodies[0]


def _branch(status):
    return {"id": "b1", "run_id": "r1", "status": status, "events": []}


def test_branch_and_suite_wait_read_until_ended(monkeypatch):
    slept = []
    monkeypatch.setattr("shakedown.client.time.sleep", slept.append)
    arena = _Answers(_branch("running"), _branch("running"), _branch("passed"))
    assert Branch(arena, "b1", "r1").wait(poll=0.25).status == "passed"
    assert (arena.paths, slept) == (["/api/branches/b1"] * 3, [0.25, 0.25])
    arena = _Answers({"id": "s1", "status": "running"}, {"id": "s1", "status": "done", "summary": {"n": 2}})
    suite = Suite(arena, {"id": "s1", "status": "running"})
    assert suite.wait(poll=1) == {"id": "s1", "status": "done", "summary": {"n": 2}} == suite.report
    with pytest.raises(TimeoutError, match="^branch b1 still running after -1 s$"):
        Branch(_Answers(_branch("running")), "b1", "r1").wait(timeout=-1)
    with pytest.raises(TimeoutError, match="^suite s1 still running after -1 s$"):
        Suite(_Answers({"id": "s1", "status": "running"}), {"id": "s1"}).wait(timeout=-1)

def test_agent_endpoint_reads_proxy_and_env():
    from shakedown import Shakedown
    from shakedown.client import AgentEndpoint

    proxy = "http://shakedown:t@127.0.0.1:5"
    d = {"agent": "a", "mcp_url": "http://x/mcp/b1/a", "gateway_url": "http://x/gateway/b1/a/v1",
         "proxy_url": proxy, "env": {"HTTPS_PROXY": proxy, "TOY_URL": "https://toy.test"}}
    a = AgentEndpoint.from_answer(Branch(Shakedown("http://127.0.0.1:1"), "b1", "r1"), d)
    assert a.proxy_url == proxy
    assert a.env == {"HTTPS_PROXY": proxy, "TOY_URL": "https://toy.test"}


def test_agent_endpoint_without_proxy():
    from shakedown import Shakedown
    from shakedown.client import AgentEndpoint

    d = {"agent": "a", "mcp_url": "http://x/mcp/b1/a", "gateway_url": "http://x/gateway/b1/a/v1"}
    a = AgentEndpoint.from_answer(Branch(Shakedown("http://127.0.0.1:1"), "b1", "r1"), d)
    assert (a.proxy_url, a.env) == (None, {})


def test_branch_clock_gets_the_route():
    clock = {"clock_ms": 5, "speed": 1.0, "paused": False, "world_epoch_ms": 1_000, "world_ms": 1_005}
    arena = _Answers(clock)
    assert Branch(arena, "b1", "r1").clock() == clock
    assert arena.paths == ["/api/branches/b1/clock"]


# ---------------------------------------------------------------- drivers

def _fake_shaked(monkeypatch, calls: list, ended: bool = False):
    """A shaked that answers a driver's calls and keeps them in ``calls`` as (method, path, body).
    With ``ended`` the branch has already ended: ending agents and finishing answer 409."""
    import httpx

    import shakedown.driver as driver
    from shakedown import Shakedown

    def handle(req: httpx.Request) -> httpx.Response:
        body = json.loads(req.content) if req.content else None
        calls.append((req.method, req.url.path, body))
        p = req.url.path
        if p == "/api/tasks":
            return httpx.Response(200, json=[{"id": "t1", "ticket": "Fix it.", "instructions": "Be brief."}])
        if p.endswith("/agents"):
            a = body["id"]
            return httpx.Response(200, json={"agent": a, "mcp_url": f"http://x/mcp/b1/{a}",
                                             "gateway_url": f"http://x/gateway/b1/{a}/v1", "tools": []})
        if p.endswith("/reasoning"):
            return httpx.Response(200, json={"seqs": list(range(len(body["items"])))})
        if ended and (p.endswith("/end") or p.endswith("/finish")):
            return httpx.Response(409, json={"error": "the branch already ended (passed)"})
        if p.endswith("/clock"):
            return httpx.Response(200, json={"clock_ms": 1500, "speed": 1.0, "paused": False,
                                             "world_epoch_ms": 0, "world_ms": 1500})
        if p.endswith("/end"):
            return httpx.Response(200, json={"seq": 9})
        if p.endswith("/finish"):
            return httpx.Response(200, json={"status": "failed" if body.get("error") else "passed",
                                             "error": body.get("error")})
        return httpx.Response(404, json={"error": f"no such route {p}"})

    def fake(url):
        a = Shakedown(url)
        a._http = httpx.Client(base_url=a.url, transport=httpx.MockTransport(handle))
        return a

    monkeypatch.setattr(driver, "Shakedown", fake)
    for k, v in {"SHAKEDOWN_URL": "http://127.0.0.1:1", "SHAKEDOWN_RUN": "r1", "SHAKEDOWN_BRANCH": "b1",
                 "SHAKEDOWN_TASK": "t1"}.items():
        monkeypatch.setenv(k, v)
    monkeypatch.delenv("SHAKEDOWN_FORK_OF", raising=False)
    return driver


def test_driver_joins_the_branch_shaked_launched(monkeypatch):
    calls: list = []
    driver = _fake_shaked(monkeypatch, calls)
    with driver.run() as run:
        assert run.branch.id == "b1" and run.task.id == "t1" and not run.is_fork
        assert run.ticket == "Fix it.\n\nBe brief."
        a = run.agent("a")
        assert run.agent("a") is a
        assert run.history("a") == []
    paths = [(m, p) for m, p, _ in calls if m == "POST"]
    assert ("POST", "/api/runs") not in paths
    assert paths == [("POST", "/api/branches/b1/agents"), ("POST", "/api/branches/b1/agents/a/end"),
                     ("POST", "/api/branches/b1/finish")]
    assert calls[-1][2] == {}


def test_driver_failure_finishes_with_the_error(monkeypatch):
    calls: list = []
    driver = _fake_shaked(monkeypatch, calls)
    with pytest.raises(ValueError, match="boom"):
        with driver.run() as run:
            run.agent("a")
            raise ValueError("boom")
    end = [b for m, p, b in calls if p.endswith("/agents/a/end")]
    assert end and end[0]["ok"] is False and "boom" in end[0]["error"]
    assert calls[-1][1] == "/api/branches/b1/finish"
    assert calls[-1][2] == {"error": "driver raised ValueError: boom"}


def test_driver_leaves_agents_it_ended_alone(monkeypatch):
    calls: list = []
    driver = _fake_shaked(monkeypatch, calls)
    with driver.run() as run:
        run.agent("a").end(ok=True)
    assert len([p for _, p, _ in calls if p.endswith("/end")]) == 1


def test_report_reasoning_posts_items(monkeypatch):
    calls: list = []
    driver = _fake_shaked(monkeypatch, calls)
    with driver.run() as run:
        a = run.agent("a")
        assert a.report_reasoning("Check stock first.", kind="plan") == [0]
        assert a.report_reasoning(items=[{"text": "x"}, {"text": "y", "kind": "step"}]) == [0, 1]
    bodies = [b for _, p, b in calls if p.endswith("/reasoning")]
    assert bodies == [{"items": [{"text": "Check stock first.", "kind": "plan"}]},
                      {"items": [{"text": "x"}, {"text": "y", "kind": "step"}]}]


def test_agent_registers_read_only_when_asked(monkeypatch):
    calls: list = []
    driver = _fake_shaked(monkeypatch, calls)
    with driver.run() as run:
        run.agent("planner", read_only=True)
        run.agent("coder")
    bodies = {b["id"]: b for _, p, b in calls if p.endswith("/agents")}
    assert bodies["planner"]["read_only"] is True
    assert bodies["coder"]["read_only"] is False


def test_driver_run_reads_the_clock(monkeypatch):
    calls: list = []
    driver = _fake_shaked(monkeypatch, calls)
    with driver.run() as run:
        assert run.clock()["clock_ms"] == 1500
    assert ("GET", "/api/branches/b1/clock", None) in calls


def test_finish_on_an_ended_branch_is_not_an_error(monkeypatch, capsys):
    calls: list = []
    driver = _fake_shaked(monkeypatch, calls, ended=True)
    with driver.run() as run:
        run.agent("a")
    err = capsys.readouterr().err
    assert "shakedown: the run had already ended (" in err
    assert calls[-1][1] == "/api/branches/b1/finish"


def test_driver_run_lists_the_agents_it_registered(monkeypatch):
    calls: list = []
    driver = _fake_shaked(monkeypatch, calls)
    with driver.run() as run:
        a, b = run.agent("a"), run.agent("b")
        assert run.agents == [a, b]


def _program(folder, name):
    p = folder / name
    p.write_text("#!/bin/sh\n")
    p.chmod(0o755)
    return str(p)


def test_find_shaked_prefers_shaked_and_still_finds_a_build_named_arenad(tmp_path, monkeypatch):
    from shakedown.local import find_shaked

    monkeypatch.delenv("SHAKEDOWN_BIN_DIR", raising=False)
    monkeypatch.setenv("PATH", str(tmp_path / "nowhere"))
    old = _program(tmp_path, "arenad")
    assert find_shaked(None, str(tmp_path)) == old
    new = _program(tmp_path, "shaked")
    assert find_shaked(None, str(tmp_path)) == new


class _Forkable:
    """A branch whose team is ``support`` and ``billing``, recording the fork it is asked for."""

    def __init__(self):
        self.asked = None

    def record(self):
        return type("Rec", (), {"team": Team([Agent.claude_code("support"), Agent.claude_code("billing")])})()

    def fork(self, at_seq, changes=None, restore="exact", team=None):
        self.asked = (at_seq, changes, restore, team)
        return "fork"


def test_rewind_fork_changes_every_agent_or_the_named_ones():
    from shakedown.client import Rewind

    b = _Forkable()
    assert Rewind(b, 40).fork(model="sonnet") == "fork"
    change = Change(model="sonnet")
    assert b.asked == (40, {"support": change, "billing": change}, "exact", None)
    Rewind(b, 7).fork(prompt="Be brief.", agents="billing", restore="approximate")
    assert b.asked == (7, {"billing": Change(prompt="Be brief.")}, "approximate", None)
    Rewind(b, 7).fork(team=[Agent.external("x")])
    assert b.asked[1] is None and b.asked[3] == [Agent.external("x")]
    with pytest.raises(ValueError, match="no agent nobody"):
        Rewind(b, 7).fork(model="haiku", agents=["nobody"])


def test_rewind_takes_a_seq_or_an_event():
    from shakedown.client import Branch, Shakedown

    branch = Branch(Shakedown("http://127.0.0.1:1"), "b1", "r1")
    assert branch.rewind(event=40).seq == 40
    assert branch.rewind(Event.from_dict({"seq": 12, "kind": "tool_call", "payload": {}})).seq == 12


def test_the_old_import_name_arena_is_shakedown_and_warns_once():
    import subprocess
    import sys

    code = """
import warnings
warnings.simplefilter("always")
with warnings.catch_warnings(record=True) as w:
    import arena, arena
    from arena import Arena, ArenaError, Agent
    from arena.local import LocalArena, find_arenad
    from arena.env import ArenaParallelEnv
    from arena.adapters import _schema
    import arena.client
import shakedown, shakedown.local, shakedown.env, shakedown.adapters._schema
assert Arena is shakedown.Shakedown and ArenaError is shakedown.ShakedownError and Agent is shakedown.Agent
assert LocalArena is shakedown.local.LocalShakedown and find_arenad is shakedown.local.find_shaked
assert ArenaParallelEnv is shakedown.env.ShakedownParallelEnv and _schema is shakedown.adapters._schema
assert arena.client is shakedown.client and arena.errors is shakedown.errors
assert shakedown.client.__spec__.name == "shakedown.client"
print(len([x for x in w if issubclass(x.category, DeprecationWarning) and "shakedown" in str(x.message)]))
"""
    out = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True)
    assert out.returncode == 0, out.stderr
    assert out.stdout.strip() == "1"


def test_a_branch_suite_and_env_keep_their_connection_as_arena_too():
    from shakedown.client import Branch, Shakedown, Suite

    sd = Shakedown("http://127.0.0.1:1")
    b = Branch(sd, "b1", "r1")
    assert b.shakedown is sd and b.arena is sd
    s = Suite(sd, {"id": "s1"})
    assert s.arena is sd


def test_token_is_sent_and_never_shown(monkeypatch):
    import httpx

    from shakedown import Shakedown

    seen = {}

    def answer(request):
        seen["auth"] = request.headers.get("authorization")
        return httpx.Response(401, json={"error": "nope"})

    monkeypatch.setenv("SHAKEDOWN_TOKEN", "sd_fromenv")
    client = Shakedown("http://shaked.test", token="sd_x", retries=1)
    client._http = httpx.Client(base_url=client.url, transport=httpx.MockTransport(answer),
                                headers=client._http.headers)
    with pytest.raises(ApiError) as e:
        client.get("/api/runs")
    assert seen["auth"] == "Bearer sd_x"
    assert "sd_x" not in repr(client) and "sd_x" not in str(e.value)
    from_env = Shakedown("http://shaked.test")
    assert from_env._http.headers.get("authorization") == "Bearer sd_fromenv"
    monkeypatch.delenv("SHAKEDOWN_TOKEN")
    assert "authorization" not in Shakedown("http://shaked.test")._http.headers
