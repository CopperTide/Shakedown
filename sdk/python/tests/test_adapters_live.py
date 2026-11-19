"""Each example driver (examples/drivers) against a real arenad and a real model through the gateway:
every agent it registered worked in the world, the run was scored, and some reasoning was recorded.

Needs the framework's extra and a provider arenad's gateway can reach (its key in arenad's environment);
each test skips, saying which is missing. Real model calls: it costs credits.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import pytest

pytestmark = pytest.mark.core

DRIVERS = Path(__file__).resolve().parents[1] / "examples" / "drivers"
MODELS = {"openai": "gpt-4.1-mini", "openrouter": "openai/gpt-4.1-mini", "anthropic": "claude-haiku-4-5"}


def _provider(arena) -> tuple[str, str]:
    for p in arena.providers():
        if p.get("available") and p["id"] in MODELS:
            return p["id"], MODELS[p["id"]]
    pytest.skip("no gateway provider: set OPENAI_API_KEY (or OPENROUTER_API_KEY, ANTHROPIC_API_KEY) "
                "in arenad's environment")


@pytest.mark.parametrize("framework,module", [
    ("langgraph", "langgraph"), ("crewai", "crewai"), ("openai_agents", "agents"),
])
def test_example_driver_runs_a_scored_run(arena, framework, module):
    pytest.importorskip(module, reason=f"needs the {framework.replace('_', '-')} extra")
    provider, model = _provider(arena)
    before = {r.id for r in arena.runs()}
    env = {k: v for k, v in os.environ.items() if not k.startswith("ARENA_")}
    env.update(ARENA_URL=arena.url, ARENA_TASK=arena.tasks()[0].id, ARENA_PROVIDER=provider,
               ARENA_MODEL=model)
    done = subprocess.run([sys.executable, str(DRIVERS / f"{framework}_driver.py")], env=env,
                          capture_output=True, text=True, timeout=900)
    new = [r for r in arena.runs() if r.id not in before]
    assert len(new) == 1, f"the driver started one run: {done.stderr[-2000:]}"
    run_id = new[0].id
    try:
        assert done.returncode == 0, done.stderr[-2000:]
        rec = arena.branch(new[0].branches[0]["id"]).record()
        agents = {e.agent for e in rec.events if e.kind == "agent_start"}
        assert agents, "the driver registered its agents"
        worked = {e.agent for e in rec.events if e.kind == "tool_call"}
        assert agents <= worked, f"every agent called a tool: {agents - worked} did not"
        assert any(e.kind == "check" for e in rec.events)
        assert any(e.kind == "reasoning" for e in rec.events), "no reasoning recorded"
    finally:
        arena.delete_run(run_id)
