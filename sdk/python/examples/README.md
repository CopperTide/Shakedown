# SDK examples

Runnable scripts for the Python SDK. Run each from `sdk/python` with `uv run python examples/<name>.py`.

## What they need

- A shaked. With `SHAKEDOWN_URL` set, an example connects to that shaked. Otherwise it starts one
  (`Shakedown.local`) from `SHAKEDOWN_BIN_DIR` (default `core/target/release`, so build shaked with
  `cd core && cargo build --release -p shaked`), on the Postgres at `SHAKEDOWN_PG_URL`, with the benchmark
  `SHAKEDOWN_BENCHMARK` (default `retail`). See `common.py`. A shaked an example starts stops when the script
  ends, so set `SHAKEDOWN_URL` when one example should see another's runs.
- Real agents. Claude Code (`claude` signed in) is the default. An API provider works when its key is in
  shaked's environment. An example that lacks what it needs says so and stops.
- Credits. Every example but `export_dataset.py` calls a model. The defaults use a Claude Haiku model.

## The examples

| Example | What it shows |
|---|---|
| `quickstart.py [task] [model]` | Run the benchmark's team of Claude Code agents, stream the timeline live, print the checks and metrics |
| `bring_your_own_agent.py [task] [claude-code\|openai\|openrouter\|anthropic] [model]` | An external run your code drives: Claude Code on the agent's MCP URL, or a minimal tool loop over the gateway |
| `agent_server.py [port]` | A deployed agent behind a URL (`kind: "http"`): takes the run's POST, works the task through its MCP endpoint with the Claude API or Claude Code, answers when done ([docs/agents.md](../../../docs/agents.md)) |
| `counterfactual_fork.py [branch] [model]` | Fork at the first write with a changed prompt and the world restored, then compare the branches |
| `suite.py [tasks] [repeats] [model] [compare_with]` | A suite: pass rates, safety, spread, and a comparison of two suites |
| `export_dataset.py <branch\|suite> [out]` | Export a run (with its forks) or a suite as JSONL and read it back. No model calls. |
| `step_env.py [task] [max_steps]` | `ShakedownParallelEnv` with a model as the policy (the Claude API with `ANTHROPIC_API_KEY`, else Claude Code), then "reset to step N" with a fork |
| `drivers/` | A LangGraph, a CrewAI and an OpenAI Agents SDK team, each a driver that joins a run Shakedown starts for it ([docs/agents.md](../../../docs/agents.md#your-own-code-drivers-and-adapters)). `SHAKEDOWN_PROVIDER` and `SHAKEDOWN_MODEL` pick the model (default `openai`, `gpt-4.1-mini`). |

## Last recorded results

On 2026-11-07, retail task 72 with `haiku`: quickstart passed (20 events, 134k tokens, $0.008 reported).
Bring-your-own-agent with Claude Code made 5 read calls and failed the task. The fork of the quickstart run at
its first write moved to the resolver's start (a CLI session restarts from the agent's start) and passed again.
A one-run suite failed (pass rate 0.0). The step-loop policy made 6 read calls, ended, and was scored failed.
Models vary between runs, and measuring that is the point.

The `drivers/` scripts run in `tests/test_adapters_live.py` (real models, skipped without a key). This README
has no recorded result for them.
