from conftest import FakeDocker
from fake_model import FakeModel, FakeRun, say, tool_call
from factory_designs.prompts import load_prompt
from factory_designs.solo_b import DESIGN, build_agent, load_design, work
from factory_designs.toolbox import TOOL_NAMES
from factory_designs.workbench import Completed, Workbench


def run_solo(tmp_path, script, settings, docker=None, **run_kw):
    docker = docker or FakeDocker()
    gate = bool(load_design()["push_gate"])
    with FakeModel({"engineer": script}) as fake:
        run = FakeRun(fake, **run_kw)
        bench = lambda i, ep: Workbench(i, ep.env, tmp_path / "ws", tmp_path / "s.sock", settings, docker, gate)  # noqa: E731
        work(run, build_agent(run, settings, bench), load_design())
    return fake, run, docker


def test_one_agent_with_every_tool(tmp_path, settings):
    fake, run, _ = run_solo(tmp_path, [say("Nothing to do.")], settings, world_minutes=None)
    assert list(run.registered) == ["engineer"]
    assert run.registered["engineer"] == {"provider": settings.provider, "model": settings.model,
                                          "read_only": False}
    names = [t["function"]["name"] for t in fake.requests[0][1]["tools"]]
    assert names == list(TOOL_NAMES)


def test_the_test_loop(tmp_path, settings):
    docker = FakeDocker([Completed(1, "1 failed"), Completed(0, "3 passed"), Completed(0, "pushed")])
    script = [tool_call("run_tests", cwd="app"), tool_call("write_file", path="app/x.py", text="x = 2\n"),
              tool_call("run_tests", cwd="app"), tool_call("shell", cmd="git push", cwd="app"),
              say("Pushed.")]
    run_solo(tmp_path, script, settings, docker=docker, world_minutes=None)
    assert [c for c in docker.commands if "pytest" in c] == ["python -m pytest -q"] * 2
    assert [c for c in docker.commands if "push" in c] == ["git push"]


def test_a_push_before_passing_tests_is_refused_as_text(tmp_path, settings):
    docker = FakeDocker()
    fake, _, _ = run_solo(tmp_path, [tool_call("shell", cmd="git push", cwd="app"), say("ok")],
                          settings, docker=docker, world_minutes=None)
    tool_msg = fake.requests[1][1]["messages"][-1]
    assert tool_msg["role"] == "tool"
    assert "error: run the tests (run_tests) in app and see them pass before pushing" in tool_msg["content"]
    assert docker.argvs == []


def test_requests_carry_the_shared_settings(tmp_path, settings):
    fake, _, _ = run_solo(tmp_path, [tool_call("read_file", path="x"), say("ok")], settings,
                          world_minutes=None)
    for _, body in fake.requests:
        assert body["model"] == settings.model
        assert body["reasoning_effort"] == "medium"
        assert body.get("max_tokens", body.get("max_completion_tokens")) == 4096
        assert body["temperature"] == 1.0
        assert body.get("stream") is not True


def test_the_prompt_comes_from_the_file(tmp_path, settings):
    fake, _, _ = run_solo(tmp_path, [say("ok")], settings, world_minutes=60.0)
    want = load_prompt(DESIGN / "engineer.md", world_minutes="60")
    assert fake.requests[0][1]["messages"][0] == {"role": "system", "content": want}


def test_time_left_continues_at_most_three_times(tmp_path, settings):
    fake, _, _ = run_solo(tmp_path, [say("Done for now.")], settings, world_minutes=60.0)
    assert len(fake.requests) == 4
    last = fake.requests[-1][1]["messages"]
    asks = [m for m in last if m.get("role") == "user" and "keep watching" in str(m.get("content"))]
    assert len(asks) == 3


def test_no_time_left_ends_the_run(tmp_path, settings):
    with FakeModel({"engineer": [say("Done.")]}) as fake:
        run = FakeRun(fake, world_minutes=1.0, clock_ms=60_000)
        bench = lambda i, ep: Workbench(i, ep.env, tmp_path / "ws", tmp_path / "s.sock", settings, FakeDocker())  # noqa: E731
        work(run, build_agent(run, settings, bench), load_design())
    assert len(fake.requests) == 1
