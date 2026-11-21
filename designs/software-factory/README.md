# Software-factory agent designs

Two agent designs for Shakedown's software-factory world, compared on what reached production:

- **A**, a hierarchical LangGraph team: a read-only planner, a coder, a reviewer and release (`team_a/`).
- **B**, one OpenAI Agents SDK engineer whose pushes wait for passing tests (`solo_b/`).

Both are customer-style agent code: each is a driver (`python -m factory_designs.team_a`,
`python -m factory_designs.solo_b`) that joins the run Shakedown started for it through
`shakedown.driver`, and builds its framework's model and tools with part 1's adapters
(`shakedown.adapters.langgraph`, `shakedown.adapters.openai_agents`) unchanged.

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

## The comparison experiment

`experiment.yaml` is the experiment as data: both designs on the world's `sprint` task, 3 runs each, base seed
20261119 (run i of both designs starts its world from the same paired seed), a 60-minute world time window and a
$1.50 spend cap per run. `key_steps.yaml` says which recorded steps matter (merge, approval, revert, deploy,
incident) by GitHub's own API shapes and the world's events.

```sh
uv run --project designs/software-factory python -m factory_designs.experiment create \
    [--provider local:runpod] [--runs N] [--world-minutes M] [--task T] [--benchmark W] [--url http://127.0.0.1:4510]
uv run --project designs/software-factory python -m factory_designs.experiment export <id> \
    [--ledger deploy/runpod/ledger.jsonl] [--out comparison.json]
```

`create` makes each design a Code connection (`<setup> · <model> on <provider>`, reused by name; the driver is
`uv run --project designs/software-factory python -m factory_designs.<design>` from the repository root, with
`FACTORY_PROVIDER` and `FACTORY_MODEL`) and posts the experiment. `export` writes `comparison.json`, what stage 6's
page reads: per design and run the checks as scored, what reached production (each service's version against its
main, the error rate, open incidents, failed contracts), tokens, hosted-equivalent cost, wall and world minutes,
GPU minutes and dollars (the ledger's pod session open at the run's start), the key steps with the reasoning
their agent recorded just before, and why a run stopped; each design's pass rates and medians; and whether the
runs shared one world, paired seeds and the same caps.

`python tests/fake_model.py --port N` serves the scripted model (every conversation looks once with
`gh api user`, then answers with JSON every role parses) as an OpenAI-compatible server, so the designs run end to
end against a real world with no real model: start shaked with `SHAKEDOWN_LOCAL_MODELS=runpod=http://127.0.0.1:N/v1`.

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
lower-case `http_proxy`; shaked gives both forms. Not yet verified: a run against a live shaked, and the
designs on a real model.
