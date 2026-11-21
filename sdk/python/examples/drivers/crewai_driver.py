"""A CrewAI crew in Shakedown: an investigator, then a resolver. SHAKEDOWN_PROVIDER/SHAKEDOWN_MODEL pick the model."""
import os

from shakedown.adapters import crewai as sd  # first: it turns CrewAI's own telemetry off
from crewai import Agent, Crew, Task
from shakedown.settings import setting

PROVIDER, MODEL = setting("SHAKEDOWN_PROVIDER", "openai"), setting("SHAKEDOWN_MODEL", "gpt-4.1-mini")

with sd.run() as run:
    def member(id: str, role: str, goal: str) -> Agent:
        a = run.agent(id, provider=PROVIDER, model=MODEL)
        return Agent(role=role, goal=goal, backstory="You are one of a team working a ticket.",
                     tools=sd.tools(a), llm=sd.llm(a, MODEL), step_callback=sd.step_callback(a))

    investigator = member("investigator", "Investigator", "Find out what the ticket needs, only reading")
    resolver = member("resolver", "Resolver", "Resolve the ticket")
    Crew(agents=[investigator, resolver], tasks=[
        Task(description=f"Investigate:\n\n{run.ticket}", expected_output="The facts the fix needs", agent=investigator),
        Task(description=f"Resolve:\n\n{run.ticket}", expected_output="What you changed and why", agent=resolver),
    ]).kickoff()
