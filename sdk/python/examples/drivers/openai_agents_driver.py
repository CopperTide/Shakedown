"""An OpenAI Agents SDK team in Shakedown: triage hands off to a resolver. ARENA_PROVIDER/ARENA_MODEL pick the model."""
import os

from agents import Agent, Runner

from arena.adapters import openai_agents as sd

PROVIDER, MODEL = os.environ.get("ARENA_PROVIDER", "openai"), os.environ.get("ARENA_MODEL", "gpt-4.1-mini")

with sd.run() as run:
    def member(id: str, instructions: str, **kw) -> Agent:
        a = run.agent(id, provider=PROVIDER, model=MODEL)
        return Agent(name=id, instructions=instructions, tools=sd.tools(a), model=sd.model(a, MODEL), **kw)

    resolver = member("resolver", "Resolve the ticket with your tools.")
    triage = member("triage", "Find out what the ticket needs, only reading, then hand off to resolver.",
                    handoffs=[resolver])
    Runner.run_sync(triage, run.ticket, max_turns=40)
