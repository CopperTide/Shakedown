"""The code sample on coppertide.ai's Shakedown page, run as written against a local shaked.

The sample (``SDK_CODE`` in the website repo's ``_build/build_site.py``, 2026-11-19) is copied below
as it stands there, with one change: the URL is the shaked these tests started. Its agent is
Claude Code; the shaked the tests start finds a stand-in ``claude`` first on its PATH
(``tests/fake_bin/claude``: no model, it calls one read tool of the world 45 times), so no model is
called and the run has more than 40 events to rewind to.
"""

from __future__ import annotations

import pytest

from shakedown import Shakedown

SAMPLE = '''\
from shakedown import Shakedown, Agent

sd = Shakedown("http://127.0.0.1:4510")   # local, or your cloud workspace
run = sd.run("72",
    team=[Agent.claude_code("support", model="haiku")],
    seed=7)
for event in run.events():          # every event, live
    print(event.summary())
print(run.check())                    # verdict, named checks, safety
run.rewind(event=40).fork(model="sonnet")  # back in time, change one thing
'''


def _forks(sd: Shakedown, run_id: str, first: str) -> list[str]:
    return [b["id"] for r in sd.runs() if r.id == run_id for b in r.branches if b["id"] != first]


@pytest.mark.core
def test_the_website_sample_runs_against_a_local_shaked(arena, fake_claude, cleanup, capsys):
    ns: dict = {}
    exec(SAMPLE.replace("http://127.0.0.1:4510", arena.url), ns)
    run = ns["run"]
    cleanup.append(run.run_id)
    *lines, verdict = capsys.readouterr().out.splitlines()
    rec = run.record()
    assert rec.ended and run.final["status"] == rec.status
    assert len([e for e in rec.events if e.kind == "tool_call"]) == 45
    assert [int(line.split()[0]) for line in lines] == [e.seq for e in rec.events]
    assert run.check() is not None and verdict == str(run.check())
    (fork_id,) = _forks(arena, run.run_id, run.id)
    fork = arena.branch(fork_id).wait(timeout=300)
    assert fork.start_seq <= 40
    assert [(a.id, a.model) for a in fork.team.agents] == [("support", "sonnet")]
    assert fork.ended
    ns["sd"].close()
