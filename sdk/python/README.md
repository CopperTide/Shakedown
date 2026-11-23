# Shakedown Python SDK

`shakedown-sdk` (import `shakedown`) drives a [Shakedown](https://github.com/CopperTide/Shakedown) engine from
Python. Start runs, follow every step live, fork from any step, run suites, and export runs as datasets. It
talks to shaked's HTTP API, so no UI is needed.

## What it needs

- Python 3.10 or newer. One dependency, `httpx`.
- A shaked to talk to: one already running (`Shakedown(url)`), or a build of this repository plus the Postgres
  of `deploy/compose.yml` for `Shakedown.local(...)`. `Shakedown.local` looks for the `shaked` binary in
  `bin_dir`, then `$SHAKEDOWN_BIN_DIR`, then on `PATH`. Build it with `cd core && cargo build --release -p shaked`.
- An API token only when shaked asks for one: pass `token=` or set `SHAKEDOWN_TOKEN`. A local shaked serves the
  person at its machine without one.
- Agents are real models: Claude Code or Codex signed in, or a model key in shaked's environment.

## Install

```bash
uv add "shakedown-sdk @ git+https://github.com/CopperTide/Shakedown#subdirectory=sdk/python"
# from a checkout of this repository: uv pip install -e sdk/python
```

The framework adapters need their extra: `shakedown-sdk[langgraph]`, `[crewai]` or `[openai-agents]`.

## Use

```python
from shakedown import Shakedown, Agent

with Shakedown.local(benchmark="retail", bin_dir="core/target/release") as sd:   # or Shakedown("http://127.0.0.1:4510")
    run = sd.run("72", team=[Agent.claude_code("support", model="haiku")], seed=7)
    for event in run.events():                    # every step, live, until the run ends
        print(event.summary())
    print(run.check())                            # passed, named checks, safety violations
    fork = run.rewind(event=10).fork(model="sonnet")   # the world as it was just before step 10
    run.export("datasets/run-72")                 # events.jsonl, branches.jsonl and SCHEMA.md
```

| Task | Calls |
|---|---|
| Connect | `Shakedown(url, token=...)`, `Shakedown.local(benchmark=..., packages=..., pg_url=..., saves=...)`, `sd.health()`, `sd.providers()` |
| Set up | `sd.benchmark()`, `sd.tasks()`, `sd.default_team()`, `Agent.claude_code(...)`, `Agent.codex(...)`, `Agent.api(id, provider=..., model=...)` |
| Run | `sd.run(task, team, runner="chain" \| "concurrent", scenario=[...], seed=...)` |
| Your own agents | `sd.external_run(task, agents=[...])`: each agent gets `mcp_url` (the world's tools) and `gateway_url` (OpenAI-compatible). `shakedown.claude_code.run(agent, prompt)` runs Claude Code on one. |
| Your own framework code | `shakedown.driver.run()` and the adapters `shakedown.adapters.langgraph`, `.crewai` and `.openai_agents` |
| Follow | `branch.events()` (or `stream()`), `astream()`, `record()`, `views()`, `rows()`, `metrics()`, `check()` |
| Control | `finish()`, `rewind(event=N).fork(model=..., prompt=..., team=..., agents=...)`, `fork(at_seq, changes=...)`, `restore_plan(seq)`, and `pause()`, `resume()`, `stop()` where `sd.supports_pause()` |
| Compare | `sd.compare(a, b)`, `sd.suite(...)`, `sd.compare_suites(a, b)` |
| Export | `branch.export(dir)`, `suite.export(dir)`: JSONL with a schema document |
| Step loop for RL | `shakedown.env.ShakedownParallelEnv`, PettingZoo-style `reset`, `step` and `close`. Your policy picks the tool calls. |

Errors from shaked raise subclasses of `ApiError`: `BadRequest`, `Unauthorized`, `NotFound`, `Conflict`,
`DiskFull`.

The full guide is [docs/sdk.md](../../docs/sdk.md). Runnable scripts are in [examples/](examples).

## Tests

```bash
cd sdk/python && uv run pytest -q
```

The unit tests run anywhere. The tests marked `core` drive a real shaked: one at `SHAKEDOWN_SDK_URL`, or one they
start from `SHAKEDOWN_SDK_BIN_DIR` (default `core/target/release`, then `core/target/debug`) on the Postgres at
`SHAKEDOWN_SDK_PG_URL`. Without either they are skipped. Only `tests/test_adapters_live.py` calls a model: it
runs each driver in [examples/drivers](examples/drivers) through a real shaked's gateway, skips without the
framework's extra or a provider key, and costs credits.

## Limits

- Pause, resume and stop need a shaked that has them. Check `sd.supports_pause()`.
- The unit tests check the adapters against a stand-in agent endpoint. Only the live test above runs them on a
  real model.
