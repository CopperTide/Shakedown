# Bring your own agent

Shakedown can test an agent system that is already deployed, wherever it runs, against its simulated plant. The
agent stays where it is; Shakedown orchestrates the run, records every tool call it makes, scores the world with
the benchmark's checks and keeps the run. Two kinds of team member do this:

| Kind | Team member | What runs the agent |
|---|---|---|
| `http` | `{kind: "http", url, headers?}` | Your service, behind a URL |
| `bedrock` | `{kind: "bedrock", model_id, region}` | shaked's own tool-use loop on Amazon Bedrock's Converse API |

Either can be one member of a team next to Claude Code or API-mode agents, under the `chain` or `concurrent`
runner, in a single run or an experiment. In the web UI: **Connect your agent** in the team picker of New
experiment and Run once, then **Test connection** and **Use this agent**.

## Your own code: drivers and adapters

An agent system written in LangGraph, CrewAI, the OpenAI Agents SDK or anything else comes in through a
**driver**: a short program that registers each of your agents with Shakedown and hands your framework the
agent's tools and model. Your agent code stays as it is. The adapters (`shakedown.adapters`, one extra each:
`pip install 'shakedown-sdk[langgraph]'`, `[crewai]`, `[openai-agents]`) turn an agent's tools into the
framework's own tool objects and its model gateway into the framework's chat model. Every tool call runs in
the world as that agent and is recorded; a refused call reaches the agent as `error: …`, so it can react.

```python
from langgraph.graph import END, START, MessagesState, StateGraph
from langgraph.prebuilt import create_react_agent
from shakedown.adapters import langgraph as sd

with sd.run() as run:
    def member(id, prompt):
        a = run.agent(id, provider="openai", model="gpt-4.1-mini")
        return create_react_agent(sd.chat_model(a, "gpt-4.1-mini"), sd.tools(a), prompt=prompt)

    graph = StateGraph(MessagesState)
    graph.add_node("investigate", member("investigator", "Find out what the ticket needs. Only read."))
    graph.add_node("resolve", member("resolver", "Resolve the ticket, using what the investigator found."))
    graph.add_edge(START, "investigate"); graph.add_edge("investigate", "resolve"); graph.add_edge("resolve", END)
    graph.compile().invoke({"messages": [("user", run.ticket)]})
```

Each of your framework's agents is one Shakedown agent (`run.agent("<id>")`), so the run shows who did what.
The model's key stays in shaked: the gateway holds it and the framework's client sends a placeholder. The
CrewAI adapter's `step_callback(agent)` records each step's thought; Shakedown also records reasoning from the
model responses it passes on (reasoning and thinking content, and what the model says alongside its tool
calls). The CrewAI adapter turns CrewAI's own telemetry off; the OpenAI Agents adapter turns the SDK's trace
export off when there is no `OPENAI_API_KEY` to send it with. The same team in each framework is in
`sdk/python/examples/drivers/`. Run one by hand (`SHAKEDOWN_URL=… python langgraph_driver.py`) and it starts a
run of its own; see [the SDK](sdk.md#drivers) for what `run` does at the end and when the driver raises.

### Code connections: Shakedown starts the driver

Save the command that starts your driver once, as a **Code connection**, and Shakedown launches it for every
run that uses it: a single run (`POST /api/runs` with `connection`), each run of an experiment setup, or a
connection test.

```bash
curl -s localhost:4510/api/connections -H 'content-type: application/json' -d '{
  "name": "Our LangGraph team",
  "command": ["uv", "run", "--extra", "langgraph", "python", "examples/drivers/langgraph_driver.py"],
  "workdir": "/path/to/sdk/python",
  "env": {"SHAKEDOWN_PROVIDER": "openai", "SHAKEDOWN_MODEL": "gpt-4.1-mini"}
}'
```

The driver gets the run's ids (`SHAKEDOWN_URL`, `SHAKEDOWN_RUN`, `SHAKEDOWN_BRANCH`, `SHAKEDOWN_TASK`, `SHAKEDOWN_CONNECTION`), the
connection's `env`, and only `PATH`, `HOME`, `USER`, `LANG`, `LC_ALL`, `TMPDIR`, `UV_CACHE_DIR`,
`XDG_CACHE_HOME` and `RUST_LOG` of shaked's own environment: model keys stay in shaked, behind the gateway.
If the driver exits before it finishes the run, the run is scored and ends `failed` with the exit code and the
end of its stderr. If it is still running 30 s after its run ended, it is killed with whatever it started.
Its exit is recorded as a `driver` world event. `POST /api/connections/{id}/test` runs the benchmark's first
task with it (spend limit $0.50) and says whether its agents worked. Shakedown starts drivers only because it
listens on 127.0.0.1 alone, and the routes that start one refuse requests a web page could make (anything not
JSON, or from another origin). Any program on this computer can still reach it, so run Shakedown with Code
connections only on a computer whose other accounts you trust. A driver that drives its run longer than
`SHAKEDOWN_CODE_RUN_SECS` (default 900) has its run ended `failed`, scored; a run with a world time window
(`world_minutes`) allows at least the window's wall time plus the driver grace, and is scored when the window ends
([concepts](concepts.md#world-time-windows)).

### The world's time

Every driver Shakedown launches gets `SHAKEDOWN_WORLD_EPOCH_MS` (the unix ms of the world clock's 0: the task's
`clock_start` read as UTC, else the run's start; a fork keeps its run's) and `SHAKEDOWN_CLOCK_SPEED` (world ms per wall
ms), and `SHAKEDOWN_WORLD_MINUTES` when the run has a world time window. `GET /api/branches/{id}/clock` (SDK: `Branch.clock()`) answers `{clock_ms, speed, paused, world_epoch_ms,
world_ms}` for a live branch (409 once it ended). Simulated traffic and twins stamp what they emit with
`world_epoch_ms + clock_ms` (the kits' `WorldTime`), so dashboards follow the world clock and its pauses.

### Reaching the world's services

In a world whose parts serve HTTP (a code host, an issue tracker: see `docs/protocol.md`, "Serving HTTP"), your
agents' own clients (`git`, `gh`, `curl`, SDKs) reach those services through Shakedown's recording proxy, and each
request is recorded as an `http` step of the part that serves its host: inspectable and forkable like a tool call.

- A driver Shakedown launches gets `HTTP_PROXY`, `HTTPS_PROXY` (and their lower-case forms), `SHAKEDOWN_PROXY`,
  `NO_PROXY=127.0.0.1,localhost,::1`, `NODE_USE_ENV_PROXY=1`, and the variables the parts' routes name (a
  base URL such as `GITHUB_API_URL`). Set them in the environment of the clients your agents run.
- An agent's register answer (`POST /api/branches/{id}/agents`) has its own `proxy_url` and `env`; the SDK gives them
  as `AgentEndpoint.proxy_url` and `AgentEndpoint.env`. Requests with an agent's proxy are recorded as that agent's;
  requests with the driver's are recorded as the agent whose model call through the gateway came last
  (`args.attributed: "latest model call"`), else as no agent's.
- The proxy listens on 127.0.0.1 only (`SHAKEDOWN_PROXY_PORT`, default a free port). Its URL carries a ticket of the
  run; a request without one is answered 407. A request to a host no part serves is refused (403) and recorded. A
  read-only agent's writes are refused. Requests wait while the run is paused and are answered 409 once it ended.
- HTTPS to a served host goes through the proxy too (`CONNECT host:443`): the proxy terminates TLS with a CA of the
  run's branch, and the same variables give `SSL_CERT_FILE`, `REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE`,
  `NODE_EXTRA_CA_CERTS` and `GIT_SSL_CAINFO` naming its certificate. Shakedown never installs it in a system trust
  store; a client that keeps its own trust store and ignores these variables fails TLS. A fork's branch has its
  own CA. `CONNECT` to another host, or to a served host on another port, is refused (403) and recorded.
- A world that serves no HTTP gives none of these variables: drivers reach their providers as before.

## HTTP agents (`shakedown-agent/1`)

**Run.** When the agent's turn starts, shaked sends one `POST` to its URL, with its headers:

```json
{
  "protocol": "shakedown-agent/1",
  "type": "run",
  "run_id": "0abe2f29…",
  "branch_id": "20ba9b19…",
  "agent": "planner",
  "task": "<the agent's prompt, the ticket, the instructions and any handoff notes>",
  "mcp_url": "http://127.0.0.1:4510/mcp/<branch>/<agent>",
  "gateway_url": "http://127.0.0.1:4510/gateway/<branch>/<agent>/v1",
  "token": "<the agent's token (sda_…) for mcp_url and gateway_url>",
  "deadline": "2026-11-09T04:18:33Z"
}
```

The agent works through `mcp_url`, an MCP endpoint (streamable HTTP, JSON answers) with the world's tools and
the talk tools, sending `Authorization: Bearer <token>` on every call; a call without it gets 401. The same
token opens `gateway_url`, the recording model gateway (OpenAI-compatible: `{gateway_url}/chat/completions`).
Every tool call is recorded as for any other agent, refusals included.

**Every agent has a token.** Since 2026-11-20 no MCP or gateway call gets in without the agent's token
(`sda_…`), whatever kind the agent is: shaked's own loop and Bedrock agents send it in process; Claude Code reads
it from the `headers` of its `--mcp-config` file (written 0600 in its working folder, never in argv); Codex from
`SHAKEDOWN_AGENT_TOKEN` (`bearer_token_env_var`); an external system gets each agent's `token` in the register
answer and the run's start, and a Code connection's driver gets `SHAKEDOWN_AGENT_TOKEN_<AGENT>` for each agent of
its team. A token dies with its agent and branch, and a pause does not count against its lifetime. An external
run's start also answers `run_token` (`sdr_…`), which registers agents, reports and finishes in hosted mode.

**Done is the HTTP answer.** The agent answers the `POST` when it has finished: any 2xx means done, optionally
with `{"ok": true, "usage": {"input_tokens": n, "output_tokens": n}, "cost_usd": x}`, which the run records as
the agent's tokens and cost (labelled `agent_reported`). `{"ok": false, "error": "…"}` says it failed. There is
no `finish` tool: one request, one result, no extra tool in the agent's list, and an error status means what it
says. The cost is that the request stays open while the agent works (up to 15 minutes, the `deadline`), so a
gateway in front of the agent with a shorter timeout (AWS API Gateway cuts at 29 s) needs a direct URL.

**How a run ends.** Each case is the agent's `agent_end` event and the branch's reason:

| What happened | Branch | Reason recorded |
|---|---|---|
| 2xx | scored by the checks | |
| `{"ok": false}` | `failed` | `the agent says it failed: …` |
| No answer by the deadline | `failed` | `the agent at <host> did not answer within 900 s` |
| An error status | `infra_error` | `the agent at <host> answered HTTP 502: <start of its body>` |
| Not reachable | `infra_error` | `could not reach the agent at <host>: …` |

Time the branch spends paused doesn't count. Once shaked stops waiting, the token no longer opens the endpoint.

**Hello.** **Test connection** (`POST /api/agents/check`) sends `{"protocol": "shakedown-agent/1", "type": "hello"}`
with the same headers and waits up to 10 s; any 2xx is a handshake, and `{"name": "…"}` names the agent.

**Secrets.** Header values never reach a log, an API answer or a saved run. shaked keeps each value in memory the
moment it reads it, and everywhere else it stands as a reference, `{"secret": "<ref>"}`, which can be sent back in
its place. A reference lasts until shaked stops; a team that names one from before is refused with "connect the
agent again". Errors name the URL's host, never its path or query, but the URL itself is stored with the team:
put credentials in headers, not in the URL.

**Where the agent runs.** shaked listens on 127.0.0.1 only. An agent on another machine reaches `mcp_url` through
a tunnel you run; set `SHAKEDOWN_AGENT_BASE` to the tunnel's base URL and `mcp_url` uses it.

**Example.** `sdk/python/examples/agent_server.py` is a complete HTTP agent in about 70 lines: it takes the run
and works the task with a real model, through the Claude API (`anthropic` package) when `ANTHROPIC_API_KEY` is
set, else through Claude Code (`claude -p` with the MCP endpoint and token in its MCP config).

```bash
(cd sdk/python && uv run python examples/agent_server.py 8765)   # then connect http://127.0.0.1:8765
```

**Verified (2026-11-08):** the example on Claude Code, model `haiku`, one run of the Factory-01 task
`supplier-delay` through a local shaked: it read the schedule, stock and suppliers, reserved the 24 free motors,
was refused the USD 2,000 purchase (`PO_LIMIT_EXCEEDED`), raised a flag for a human, and the run passed every
check, in 24 s, 43k tokens, USD 0.0048 as Claude Code reported it. The header value used was in no record. The
example's Claude API path has not been run (no key on that machine). The protocol is tested in
`core/crates/shaked/tests/http_agent.rs` with a protocol client that calls the MCP tools (no model, no decisions).

## Bedrock agents

`{kind: "bedrock", model_id, region}` becomes an API-mode agent on provider `bedrock:<region>` (stored as
`{kind: "open", provider: "bedrock:us-east-1", model: "<model_id>"}`): shaked's own tool-use loop, with every
model call recorded through its gateway, translated to the Converse API (`POST
https://bedrock-runtime.<region>.amazonaws.com/model/<model_id>/converse`) and signed with Signature Version 4.

Credentials come from the user's own AWS setup, read when a call is made and never stored:
`AWS_BEARER_TOKEN_BEDROCK` (a Bedrock API key), else `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` (and
`AWS_SESSION_TOKEN`), else whatever the AWS CLI resolves (`aws configure export-credentials`, honouring
`AWS_PROFILE`: shared files, IAM Identity Center, `aws login`, assumed and instance roles), kept in memory until
five minutes before they expire. **Test connection** makes one Converse call of one output token.

**Untested live.** No Bedrock call has been made. The signing passes AWS's own test vector (`get-vanilla`), and
the request and answer translation, the signed request and the error path are tested against a local server
standing in for Bedrock (`core/crates/shaked/src/providers/bedrock*`). On the machine it was built on, the AWS
session had expired, and Test connection said so: "no usable AWS credentials: the aws CLI says: … Your session
has expired". Costs of Bedrock calls are not known to shaked and show as unknown. Claude's thinking blocks are
not sent back across turns (the loop doesn't ask for thinking).
