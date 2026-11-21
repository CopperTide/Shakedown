"""Design B: one OpenAI Agents SDK engineer with every tool and a test loop. Its pushes wait
until the repository's tests passed after its last ``write_file`` (the workbench's push gate),
so "test before you push" is a property of its tools, not only of its prompt. When it stops
with world time left in the run's window, it is asked to keep watching, a few times at most.
The agent, its tools and prompt are data in ``solo_b/``.

    python -m factory_designs.solo_b      # as a Shakedown Code connection's driver
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, Callable

import yaml
from agents import Agent, ModelSettings, Runner
from agents.exceptions import MaxTurnsExceeded
from openai.types.shared import Reasoning

from shakedown.adapters import openai_agents

from .prompts import load_prompt
from .settings import Settings, load_settings
from .stops import run_design
from .toolbox import ToolBox, minutes_text, time_left
from .workbench import Workbench, benches

DESIGN = Path(__file__).resolve().parent.parent / "solo_b"

Bench = Callable[[str, Any], Workbench]


def load_design(folder: Path = DESIGN) -> dict:
    return yaml.safe_load((folder / "design.yaml").read_text())


def build_agent(run: Any, settings: Settings, bench: Bench, folder: Path = DESIGN) -> Agent:
    """The engineer, registered on ``run``, with its tools on its workbench."""
    spec, s = load_design(folder)["agent"], settings
    ep = run.agent(spec["id"], provider=s.provider, model=s.model, read_only=bool(spec["read_only"]))
    box = ToolBox(ep.id, bench(spec["id"], ep), run, list(spec["tools"]))
    return Agent(
        name=spec["id"],
        instructions=load_prompt(folder / spec["prompt"], world_minutes=minutes_text(run)),
        tools=openai_agents.tools(box),
        model=openai_agents.model(ep, s.model),
        model_settings=ModelSettings(temperature=s.temperature, max_tokens=s.max_tokens,
                                     reasoning=Reasoning(effort=s.reasoning_effort)),
    )


def work(run: Any, agent: Agent, design: dict) -> None:
    """Runs the engineer on the ticket; while the window has time left when it stops, asks it
    to keep watching, at most ``continuations`` times. Running out of turns ends it."""
    turns = int(design["agent"]["max_turns"])
    conversation: Any = run.ticket
    for asked in range(int(design["continuations"]) + 1):
        if asked:
            conversation = [*conversation, {"role": "user", "content": design["inputs"]["continue"]}]
        try:
            conversation = Runner.run_sync(agent, conversation, max_turns=turns).to_input_list()
        except MaxTurnsExceeded:
            return
        if not time_left(run):
            return


def main(run: Any) -> None:
    settings, design = load_settings(), load_design()
    bench = benches(settings, push_gate=bool(design["push_gate"]))
    work(run, build_agent(run, settings, bench), design)


if __name__ == "__main__":
    run_design(main)
