import threading
import time

from arena.adapters import langgraph, openai_agents
from fake_model import FakeModel, FakeRun
from factory_designs.toolbox import TOOL_NAMES, ToolBox, wait_minutes


def _box(bench, names=TOOL_NAMES):
    with FakeModel({}) as fake:
        return ToolBox("a", bench, FakeRun(fake), list(names))


def test_toolbox_tools_work_through_both_adapters(bench):
    (bench.workspace / "notes.txt").write_text("ship it")
    box = _box(bench)
    lg = langgraph.tools(box)
    oa = openai_agents.tools(box)
    assert [t.name for t in lg] == list(TOOL_NAMES)
    assert [t.name for t in oa] == list(TOOL_NAMES)
    assert lg[TOOL_NAMES.index("read_file")].invoke({"path": "notes.txt"}) == "ship it"
    tool = oa[TOOL_NAMES.index("read_file")]
    import asyncio
    assert asyncio.run(tool.on_invoke_tool(None, '{"path": "notes.txt"}')) == "ship it"


def test_a_box_has_only_its_tools_in_their_order(bench):
    box = _box(bench, ["read_file", "shell"])
    assert [t.name for t in box.tools] == ["read_file", "shell"]
    assert box.call("write_file", {"path": "x", "text": "y"}).is_error


def test_bad_arguments_are_an_error_result(bench):
    r = _box(bench).call("read_file", {"file": "x"})
    assert r.is_error and "read_file" in r.text


def test_wait_minutes_waits_for_world_time():
    with FakeModel({}) as fake:
        run = FakeRun(fake)
        stop = threading.Event()

        def tick():
            while not stop.is_set():
                run.advance(30_000)
                time.sleep(0.01)

        t = threading.Thread(target=tick)
        t.start()
        try:
            start = run.clock()["clock_ms"]
            text = wait_minutes(run, 2, poll_s=0.01)
            assert run.clock()["clock_ms"] - start >= 120_000
            assert text.startswith("World time is 2026-11-19T09:") and text.endswith("Z.")
        finally:
            stop.set()
            t.join()


def test_wait_minutes_is_capped_at_thirty():
    with FakeModel({}) as fake:
        run = FakeRun(fake)
        run.advance(0)
        polls = []

        def clock():
            polls.append(1)
            run.advance(60_000 * 10)
            return FakeRun.clock(run)

        run.clock = clock
        wait_minutes(run, 500, poll_s=0)
        assert len(polls) == 1 + 3
