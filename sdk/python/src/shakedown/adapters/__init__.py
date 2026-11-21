"""Adapters: an agent's Shakedown tools and gateway as your framework's own objects.

- :mod:`shakedown.adapters.langgraph`: LangChain tools and a ``ChatOpenAI`` (LangGraph, LangChain agents)
- :mod:`shakedown.adapters.crewai`: CrewAI tools, an ``LLM`` and a step callback that records reasoning
- :mod:`shakedown.adapters.openai_agents`: OpenAI Agents SDK ``FunctionTool``\\ s and a chat-completions model

Each needs its extra (``pip install 'shakedown-sdk[langgraph]'``) and re-exports :func:`shakedown.driver.run`.
The model's key stays in shaked: the gateway holds it, and the clients send a placeholder.
"""
