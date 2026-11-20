# Software-factory agent designs

Two agent designs for Shakedown's software-factory world, compared on what reached production:

- **A**, a hierarchical LangGraph team: a read-only planner, a coder, a reviewer and release (`team_a/`).
- **B**, one OpenAI Agents SDK engineer whose pushes wait for passing tests (`solo_b/`).

Both are customer-style agent code: each is a driver (`python -m factory_designs.team_a`,
`python -m factory_designs.solo_b`) that joins the run Shakedown started for it through
`arena.driver`, and builds its framework's model and tools with part 1's adapters
(`arena.adapters.langgraph`, `arena.adapters.openai_agents`) unchanged.

## What the designs share, and only that

- `shared.yaml`: provider, model, reasoning effort, output cap, temperature, tool output size, command timeout
  and workbench image. `FACTORY_PROVIDER` and `FACTORY_MODEL` override the provider and model; nothing else is
  overridable.
- The tools in `factory_designs/toolbox.py`: `shell`, `read_file`, `write_file`, `run_tests`, `wait_minutes`,
  with the same descriptions and schemas for both. A design picks which of them each of its agents gets, in its
  `design.yaml`.
- Prompts and roles are data: each design's `design.yaml` and `*.md` files. Placeholders are `{name}`; any
  other brace (a JSON example) is kept as written.
- `run_design`: a stop from the world (the spend limit's 402, the run's end, a model that keeps failing after
  the client's retries) ends a design quietly with a `stopped: …` step, and Shakedown scores the run at its
  world time window like any other.

`tests/test_fairness.py` checks that both designs send the same model settings and that their tools together
are the same set.

## The workbench

Every tool command runs in a fresh container (`factory_designs/workbench.py`): `--network none`, read-only
root, a 256 MB `/tmp`, 1 GB of memory, 256 processes, user 1000, the agent's workspace at `/work`, and the
command bounded by `command_timeout_s`. Its one way out is a unix socket mounted at `/run/arena-proxy.sock`:
`factory_designs/bridge.py` pipes each connection on it to Shakedown's recording proxy, and the image's
`bridge-then` relays `127.0.0.1:3128` to it. The agent's proxy variables point at `127.0.0.1:3128` and keep the
agent's ticket, so each request is recorded as that agent's. Env values naming a file (the proxy's CA
certificate) are mounted read-only at the same path. Paths outside the workspace are refused, and every
answer is text (`exit <code>` and the output's tail, or `error: …`), never an exception.

### The workbench image

`workbench/build.sh` builds `shakedown-workbench:1` (the tag in `shared.yaml`): `python:3.12-slim` with `git`,
`curl`, `socat`, gh 2.45.0 (the release `.deb`, sha256-checked), and pytest, PyGithub, jira, slack_sdk and
pdpyras at pinned versions.

`run_tests` runs the repository's tests in this image, with no network. The world's application repositories
need their own dependencies there. Build it on the image the world's CI uses for those tests:
`WORKBENCH_BASE=<that image> workbench/build.sh`. On plain `python:3.12-slim` those tests fail to import. For the
software-factory world, `benchmarks/world/templates/software-factory/images/build.sh workbench` does this with the
world's base image (`shakedown-sf/base:1.0.0`, its `images.yaml`).

## Tests

```sh
uv run --with pytest pytest -q
```

No test calls a model, and only the `live` one runs Docker. The others run against `tests/fake_model.py`, a scripted server in the shape
vLLM v0.32.0 answers for gpt-oss, which records each request, and a fake Docker that records each `docker run`.

Verified on 2026-11-19, on Linux with Python 3.14 and Docker: the test suite; the image build (gh 2.45.0 and
the clients import); and `tests/test_live_workbench.py` (marked `live`, skipped without Docker or the image),
where a command in the container reaches a stand-in proxy through the bridge with the agent's ticket as
`Proxy-Authorization`, cannot reach the network directly, and cannot write its root. `curl` reads only the
lower-case `http_proxy`; arenad gives both forms. Not yet verified: a run against a live arenad, and the
designs on a real model.
