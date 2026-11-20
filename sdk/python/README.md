# shakedown

Build, drive, observe and export [Shakedown](https://github.com/CopperTide/Shakedown) environments from Python.
Shakedown's core (shaked) is headless; this SDK talks to its HTTP API. No UI needed.

```bash
uv add "shakedown @ git+https://github.com/CopperTide/Shakedown#subdirectory=sdk/python"   # or: uv pip install -e sdk/python
```

```python
from shakedown import Shakedown, Agent

with Shakedown.local(benchmark="retail", bin_dir="core/target/release") as sd:   # or Shakedown("http://127.0.0.1:4510")
    run = sd.run("72", team=[Agent.claude_code("support", model="haiku")], seed=7)
    for event in run.events():             # every event, live, until the run ends
        print(event.summary())
    print(run.check())                     # passed?, named checks, safety violations
    run.rewind(event=40).fork(model="sonnet")   # back to just before event 40, one thing changed
    run.export("datasets/run-72")          # events.jsonl + branches.jsonl + SCHEMA.md
```

- **Connect or start a core:** `Shakedown(url)`, `Shakedown.local(benchmark=..., packages=..., pg_url=..., saves=...)`.
- **Build an environment:** `sd.benchmark()` (tasks, `scenario_options`), `sd.run(task, team, scenario=[...], seed=...)`.
- **Run teams:** Shakedown's runners (`chain`, `concurrent`; Claude Code, Codex or API agents with real models), or
  `sd.external_run(task, agents=["a", "b"])`: each agent gets an MCP endpoint and an OpenAI-compatible gateway
  for any agent framework (`shakedown.claude_code.run(agent, prompt)` runs Claude Code on one).
- **Observe:** `branch.events()` (live; also `stream()`) / `astream()`, `record()`, `views()`, `rows()`, `metrics()`, `check()`.
- **Control:** `finish()`, `rewind(event=N).fork(model=..., prompt=..., team=..., agents=...)`, `fork(at_seq, changes=..., team=...)`, `pause()`/`resume()`/`stop()` on cores that have
  them (`sd.supports_pause()`), suites (`sd.suite(...)`, `compare_suites`).
- **Export:** `branch.export(dir)`, `suite.export(dir)`: JSONL with a schema doc.
- **Step loop for RL:** `shakedown.env.ShakedownParallelEnv`, PettingZoo-style `reset`/`step`/`close`; your policy picks
  the tool calls.

Full guide: [docs/sdk.md](../../docs/sdk.md). Examples: [examples/](examples/). Python 3.10 or newer; one
dependency (`httpx`). The old import name `arena` still works ([Old names](../../README.md#old-names)).
