"""A LangGraph team in Shakedown: an investigator, then a resolver. SHAKEDOWN_PROVIDER/SHAKEDOWN_MODEL pick the model."""
import os

from langgraph.graph import END, START, MessagesState, StateGraph
from langgraph.prebuilt import create_react_agent

from shakedown.adapters import langgraph as sd
from shakedown.settings import setting

PROVIDER, MODEL = setting("SHAKEDOWN_PROVIDER", "openai"), setting("SHAKEDOWN_MODEL", "gpt-4.1-mini")

with sd.run() as run:
    def member(id: str, prompt: str):
        a = run.agent(id, provider=PROVIDER, model=MODEL)
        return create_react_agent(sd.chat_model(a, MODEL), sd.tools(a), prompt=prompt)

    graph = StateGraph(MessagesState)
    graph.add_node("investigate", member("investigator", "Find out what the ticket needs. Only read; report what you found."))
    graph.add_node("resolve", member("resolver", "Resolve the ticket, using what the investigator found."))
    graph.add_edge(START, "investigate")
    graph.add_edge("investigate", "resolve")
    graph.add_edge("resolve", END)
    graph.compile().invoke({"messages": [("user", run.ticket)]}, {"recursion_limit": 60})
