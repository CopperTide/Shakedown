"""Adapters: an agent's Shakedown tools and gateway as your framework's own objects.

- :mod:`arena.adapters.langgraph`: LangChain tools and a ``ChatOpenAI`` (LangGraph, LangChain agents)
- :mod:`arena.adapters.crewai`: CrewAI tools, an ``LLM`` and a step callback that records reasoning
- :mod:`arena.adapters.openai_agents`: OpenAI Agents SDK ``FunctionTool``\\ s and a chat-completions model

Each needs its extra (``pip install 'arena-sdk[langgraph]'``) and re-exports :func:`arena.driver.run`.
The model's key stays in arenad: the gateway holds it, and the clients send a placeholder.
"""
