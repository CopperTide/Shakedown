"""People never call a twin: each act is a world event that the world's link hands to the twin, and each notice
comes back the same way, so every act is a recorded step that a replay repeats and a fork reproduces."""
import re
import socket

import pytest
import yaml

from arena_part.serve import dispatch
from people_part.part import People
from support import EPOCH, FIXTURES, HOUR, MIN
from world import World

PEOPLE = [{"id": "asker-001", "name": "Ask One", "login": "ask1", "role": "asker"},
          {"id": "answerer-001", "name": "Ans One", "login": "ans1", "role": "answerer"}]


def people_config(seed=3):
    rules = {r: yaml.safe_load((FIXTURES / "links" / f"{r}.yaml").read_text()) for r in ("asker", "answerer")}
    return {"seed": seed, "people": PEOPLE, "rules": rules, "systems": ["board"]}


@pytest.fixture
def no_connections(monkeypatch):
    def refuse(*a, **k):
        raise AssertionError("a part opened a connection")
    monkeypatch.setattr(socket.socket, "connect", refuse)
    monkeypatch.setattr(socket, "create_connection", refuse)


@pytest.fixture
def world(no_connections):
    w = World.start(people_config())
    yield w
    w.stop()


def act_steps(w: World) -> list:
    return [(s["args"]["person"], s["args"]["action"], s["args"]["args"]) for s in w.steps if s["tool"] == "board_act"]


def test_people_reach_twins_only_through_recorded_link_steps(world):
    world.run(3 * HOUR)
    people, _ = world.parts["people"]
    log = people.inspect({"instance": "p"}, ["log/acts"])["docs"]["log/acts"]["entries"]
    assert len(log) >= 15 and [(e["person"], e["action"]) for e in log] == [s[:2] for s in act_steps(world)]
    board, _ = world.parts["board"]
    posts = board.inspect({"instance": "b"}, [])["docs"]
    assert len(posts) == len(log) and {p["by"] for p in posts.values()} == {"ask1", "ans1"}
    notices = [s for s in world.steps if s["tool"] == "people_notice"]
    assert len(notices) == len([p for p in posts.values() if p["kind"] == "ask"])
    assert all(s["actor"] == "link" for s in world.steps)


def test_the_engine_imports_nothing_that_talks_to_the_network():
    code = "\n".join(p.read_text() for p in (FIXTURES.parents[1] / "people_part").glob("*.py"))
    assert not re.search(r"^\s*(import|from)\s+(socket|http|urllib|requests|httpx|ssl)\b", code, re.M)


def test_a_fork_from_a_save_reproduces_the_steps(no_connections, saves_root):
    straight = World.start(people_config())
    straight.run(70 * MIN)
    saves, done = straight.save("s", saves_root), len(straight.steps)
    straight.run(3 * HOUR)
    fork = World.start(people_config(), saves=saves, clock_ms=70 * MIN, suffix="f")
    try:
        fork.run(3 * HOUR)
        assert fork.steps == straight.steps[done:] and len(fork.steps) > 10
    finally:
        straight.stop()
        fork.stop()


def test_a_replay_of_the_recorded_inputs_gives_the_same_answers(world):
    world.run(2 * HOUR)
    again = People()
    dispatch(again, "start", {"instance": "r", "config": people_config(), "seed": 1, "clock_ms": 0,
                                    "world_time": {"epoch_ms": EPOCH, "speed": 1.0}})
    for kind, clock, *rest in world.inputs["people"]:
        if kind == "advance":
            assert again.advance("r", clock) == rest[0]
        else:
            tool, args, answer = rest
            assert again.call_tool("r", tool, args, clock) == answer
