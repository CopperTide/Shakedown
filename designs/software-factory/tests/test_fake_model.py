"""The scripted model also stands in for the model server behind shaked's gateway (`local:<name>`'s
`/v1/chat/completions`), so both designs run end to end against a real world with no real model."""
import json
import urllib.request

from fake_model import FakeModel, look_then_answer

TOOLS = [{"type": "function", "function": {"name": "shell", "parameters": {}}}]


def post(url: str, body: dict) -> dict:
    req = urllib.request.Request(url, data=json.dumps(body).encode(), method="POST",
                                 headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as r:
        return json.loads(r.read())


def test_the_server_path_a_gateway_forwards_to_is_answered_for_no_named_agent():
    with FakeModel(lambda agent, body: look_then_answer(agent, body)) as fake:
        ans = post(f"{fake.url}/v1/chat/completions", {"messages": [{"role": "user", "content": "hi"}],
                                                        "tools": TOOLS})
    assert fake.requests[0][0] == "" and ans["object"] == "chat.completion"


def test_a_conversation_looks_once_then_answers_every_roles_keys():
    first = look_then_answer("", {"messages": [{"role": "user", "content": "the ticket"}], "tools": TOOLS})
    call = first["choices"][0]["message"]["tool_calls"][0]["function"]
    assert call["name"] == "shell" and "gh" in json.loads(call["arguments"])["cmd"]
    after = look_then_answer("", {"messages": [{"role": "user", "content": "t"},
                                               {"role": "assistant", "tool_calls": [{}]},
                                               {"role": "tool", "content": "exit 0"}], "tools": TOOLS})
    text = after["choices"][0]["message"]["content"]
    assert {"items", "pr", "verdict", "notes"} <= json.loads(text).keys()
    assert after["choices"][0]["message"]["reasoning"]


def test_without_a_shell_tool_it_answers_at_once():
    ans = look_then_answer("", {"messages": [{"role": "user", "content": "t"}], "tools": []})
    assert ans["choices"][0]["finish_reason"] == "stop"
