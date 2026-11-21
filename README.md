<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/assets/banner-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset=".github/assets/banner-light.svg">
    <img alt="Shakedown: test agent systems in a simulated copy of their world" src=".github/assets/banner-light.svg" width="100%">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/CopperTide/Shakedown/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/CopperTide/Shakedown/actions/workflows/ci.yml/badge.svg"></a>
  <a href="LICENSE"><img alt="License: Apache 2.0" src="https://img.shields.io/badge/license-Apache%202.0-2f6f73"></a>
  <img alt="Rust stable" src="https://img.shields.io/badge/rust-stable-2f6f73?logo=rust">
  <img alt="Python 3.10 or newer" src="https://img.shields.io/badge/python-3.10%2B-2f6f73?logo=python&logoColor=white">
  <a href="docs/protocol.md"><img alt="Protocol: shakedown-part/1" src="https://img.shields.io/badge/protocol-shakedown--part%2F1-2f6f73"></a>
</p>

<p align="center">
  <a href="#use-shakedown-from-code">From code</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="docs/concepts.md">Concepts</a> ·
  <a href="docs/protocol.md">Protocol</a> ·
  <a href="integrations/README.md">Integrations</a> ·
  <a href="docs/reference.md">Reference</a> ·
  <a href="CONTRIBUTING.md">Contributing</a>
</p>

**Shakedown tests agent systems in a simulated copy of the world they will be deployed in.** An *environment* is the
systems the agents work with (an ERP, a database, suppliers, a plant), made of real software or simulators. A
*scenario* is what happens to it: a task and the disruptions along the way. An *agent setup* is a model, a team
structure and its rules. An **experiment** runs each of any number of setups as many times as needed on the same environment and
scenario, and shows them side by side: pass rate (with its range), the checks each run passed, cost, time, and
safety (actions the environment denied, flags raised for a human, safety checks violated). Limits live in the
environment, not in the agent's prompt, and are enforced deterministically; scoring is by deterministic checks,
never by a model. shaked, the Shakedown service, records every step of every run, so any run can be opened and read
step by step.

The service is `shaked`, the Python SDK `shakedown` and the settings `SHAKEDOWN_*`; the old `arena` names still
work ([Old names](#old-names)).

## Use Shakedown from code

The core runs headless; no UI is needed. With the Python SDK ([docs/sdk.md](docs/sdk.md), `sdk/python`):

```python
from shakedown import Shakedown, Agent

with Shakedown.local(benchmark="retail", bin_dir="core/target/release") as sd:   # or Shakedown("http://127.0.0.1:4510")
    run = sd.run("72", team=[Agent.claude_code("support", model="haiku")], seed=7)
    for event in run.events():                    # every event, live, until the run ends
        print(event.summary())
    print(run.check())                            # passed?, named checks, safety violations
    fork = run.rewind(event=40).fork(model="sonnet")   # back to just before event 40, one thing changed
    run.export("data/run-72")                     # JSONL dataset with a schema
```

Bring your own agents instead (`sd.external_run(...)`: each agent gets an MCP endpoint with the world's tools
and an OpenAI-compatible gateway), run experiments (`POST /api/experiments`), or drive a world step by step as a multi-agent RL environment
(`shakedown.env.ShakedownParallelEnv`). Any other language uses the [HTTP API](docs/reference.md#http-api).

**Project folder.** Worlds, scenarios, agent setups and experiments are YAML files in a project folder
(`SHAKEDOWN_PROJECT`), so they can be diffed, reviewed and committed like code. **From an agent:** every
operation is also a tool of the control MCP server, e.g. `claude mcp add shakedown -- shaked mcp --project
~/work/my-project`; see [Use Shakedown from an agent](docs/reference.md#use-shakedown-from-an-agent-mcp).

## Bring your own agent

Test the agent system you already run. Add it to a team as an `http` agent, `{kind: "http", url, headers?}`:
when the run starts, shaked POSTs the task, an MCP endpoint with the world's tools and a token to the URL; the
agent works through that endpoint, every call recorded, and answers the POST when it is done. Or name a model on
Amazon Bedrock, `{kind: "bedrock", model_id, region}`, and shaked runs the tool-use loop with your AWS
credentials. In the UI it is **Connect your agent** in the team picker, then **Test connection**. Header values
never reach a log or a saved run. `sdk/python/examples/agent_server.py` is a working example; the Bedrock path is
untested live. See [docs/agents.md](docs/agents.md).

## Why Shakedown

- **Experiments, not anecdotes.** One run of an LLM agent says little. An experiment runs each setup N times on
  the same seeded world and scenario, interleaving the setups so they meet the same conditions, and reports pass
  rate with a 95% range, checks, cost, time and safety per setup.
- **Hard limits outside the agent.** A purchasing cap, an approval gate or a sandbox policy is enforced by the
  environment and recorded: every denied action is a step with its reason, counted per run.
- **Deterministic scoring.** Each benchmark's checks (goal checks and safety checks, apart) decide pass or fail.
  An optional AI review explains a run; it never scores one.
- **Rewind the world, not just the transcript.** After each step that changes the world, shaked saves the parts
  that changed (a part that can only replay is replayed instead). A fork puts each part back as it was just
  before the fork point, and the team continues from there.
- **Honest restores.** Each integration declares what it can do: an exact save, a deterministic replay, an
  approximate save, or nothing. A fork that can't restore a part in the mode you asked for is refused, naming
  the part; approximately restored branches are labelled in the run page, the branch list and compare.
- **Any harness, one team.** Run agents as headless Claude Code or Codex CLIs that see only the world's tools
  through an MCP endpoint (CLI mode), or in shaked's own loop with every model call recorded (API mode). Mix them
  in one team and swap one for the other in a fork.
- **Any model provider.** Each API-mode agent picks its provider: OpenRouter, the OpenAI API, the Claude API, or a
  model on your own machine through any OpenAI-compatible server (Ollama, vLLM, LM Studio, llama.cpp's server).
  A fork can move an agent to another provider or model.
- **Integrations work like device drivers.** Each kind of system is a separate program, in any language, that
  speaks `shakedown-part/1`: JSON-RPC over stdin and stdout. Rust and Python kits do the plumbing, and
  `shakedown-part-check` checks a new integration against the protocol.
- **Benchmarks are packages too.** A benchmark brings the tasks, tools, seed data and gold actions, and names
  the world it runs in. The core depends on no integration and no benchmark
  (`core/crates/parts/tests/isolation.rs` checks this).
- **Compare what changed.** Outcome, time, tokens and cost side by side, the first step where two branches
  differ, and the state that differs at the end.

## Screenshots

<table>
  <tr>
    <td width="50%"><img src="docs/images/experiment.png" alt="Three agent setups side by side on the same pharma scenario: pass rate with its range, a square per run, checks passed, safety, cost and time, and the checks some run failed"></td>
    <td width="50%"><img src="docs/images/new-experiment.png" alt="New experiment: the scenario, two agent setups (team, model, rules) and runs per setup"></td>
  </tr>
  <tr>
    <td><sub><b>Experiment.</b> One column per setup: pass rate, every run, checks, safety, cost, time.</sub></td>
    <td><sub><b>New experiment.</b> A scenario, any number of setups, runs per setup.</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/images/run.png" alt="A run: its time, cost, checks and safety, the AI review's one line, and every step on a timeline"></td>
    <td width="50%"><img src="docs/images/world.png" alt="The world view of a pharma run: KPIs, a scrubber over the steps, and the plant's systems as an HMI overview"></td>
  </tr>
  <tr>
    <td><sub><b>Run.</b> Every step of every agent on one timeline.</sub></td>
    <td><sub><b>World view.</b> The simulated systems at any step.</sub></td>
  </tr>
</table>

<sub>Captured from Shakedown's web UI on 2026-11-08: real runs of the pharma benchmark (Claude Code on Sonnet; the setups
in the first capture are suites from before experiments, shown side by side), and the retail benchmark's New
experiment page.</sub>

## How it works

```mermaid
flowchart LR
  subgraph clients["Clients"]
    UI["Web UI<br/>ui/"]
    SC["CopperTide<br/>Shakedown control"]
  end

  subgraph team["Team of agents"]
    CLI["CLI mode<br/>Claude Code · Codex"]
    OPEN["API mode<br/>shaked's agent loop"]
  end

  subgraph engine["shaked · core/"]
    API["HTTP API + SSE<br/>127.0.0.1:4510"]
    MCP["MCP endpoint<br/>per agent"]
    GW["Model gateway"]
    ENG["Recorder · forks<br/>restore · compare"]
  end

  subgraph world["World · packages in any language"]
    BM["Benchmark<br/>tasks · tools · gold actions"]
    P1["postgres"]
    P2["folder"]
    P3["service · mariadb · erpnext · …"]
  end

  PG[("Postgres<br/>run records")]
  OR["Model providers<br/>OpenRouter · OpenAI · Claude API · local servers"]

  UI -->|HTTP| API
  SC -.->|same API| API
  CLI -->|world tools over MCP| MCP
  OPEN -->|model calls| GW
  GW --> OR
  API --> ENG
  MCP --> ENG
  ENG --> PG
  ENG <-->|"shakedown-part/1<br/>JSON-RPC over stdio"| world
```

The world is made of **integrations**: isolated programs that each put one kind of system in the world and
speak the `shakedown-part/1` protocol with shaked ([docs/protocol.md](docs/protocol.md)). Each one starts
instances, runs tool calls, saves an instance's state, starts from a save, shows the state and compares two
states. A **benchmark** names the world it runs in (its **parts**) and brings the tasks, the tools, the data and
the gold actions. When a branch ends, shaked diffs its world with the task's gold world; it passes when no key
differs and at least one part could be checked. The core (shaked, the protocol, the kits) depends on no UI: the
web UI is one client of shaked's HTTP API, and the Python SDK and CopperTide's Shakedown control workspace are others.

Read [docs/concepts.md](docs/concepts.md) for the full model: runs, branches, steps, saves, the restore table,
teams and handoffs, forking rules and cost labels.

## Quick start

This uses the included retail example benchmark (`benchmarks/retail`, customer service from tau2-bench retail,
on one `postgres` part), the default benchmark. You need Docker, Rust and Node; the `claude` and/or `codex`
CLIs on your PATH for CLI agents; for API agents, a key (`OPENROUTER_API_KEY`, `OPENAI_API_KEY` or
`ANTHROPIC_API_KEY`) or a local OpenAI-compatible server named in `SHAKEDOWN_LOCAL_MODELS` (for example
`ollama=http://127.0.0.1:11434/v1`); `uv` only for the Python integrations.

```bash
git clone https://github.com/CopperTide/Shakedown.git && cd Shakedown
docker compose -f deploy/compose.yml up -d     # Postgres on 127.0.0.1:5544 (loopback only)
cargo build --release                          # the Rust integrations and benchmarks, into core/target/release
(cd core && cargo run -p shaked)               # API on http://127.0.0.1:4510
(cd ui && npm install && npm run dev)          # UI on http://127.0.0.1:4520 (proxies /api to 4510)
```

shaked keeps its records in the database `shakedown` on that Postgres (`arena` on a server made before 2026-11-19). It starts the integration and benchmark
programs itself, from `SHAKEDOWN_BIN_DIR` (default `core/target/release`), and refuses to start if the benchmark
can't be described (package missing or not built). It listens on 127.0.0.1 only. `GET /api/health` shows the
benchmark, which CLIs were found, each model provider and whether API mode is available, and the disk use;
`GET /api/providers` also asks the local servers and lists the models each provider offers.

The UI is optional: everything works from code ([docs/sdk.md](docs/sdk.md)). A UI served from another origin
can call shaked when that origin is in `SHAKEDOWN_CORS_ORIGIN`. Or build the UI and let shaked serve it:

```bash
(cd ui && npm install && npm run build)
(cd core && SHAKEDOWN_UI_DIST=../ui/dist cargo run -p shaked)    # UI and API on :4510
```

Then:

1. **New experiment:** pick the scenario (a task and any disruptions), add as many agent setups as you compare (team, model,
   rules), choose runs per setup, and press Run experiment.
2. **Read the results:** one column per setup: pass rate, a square per run, checks, safety, cost and time. Click
   a square to open that run.
3. **Inspect a run:** its timeline; click a step to see its arguments, its result, and the state it read or
   wrote, as it was at that step. A run's ⋯ menu has its world view, process view, comparison with another run,
   and, under Advanced, rewind (fork from a step with a change and rerun).

Settings (port, benchmark, package folders, disk limit, clock speed, model providers) are environment variables;
see [docs/reference.md](docs/reference.md#configuration).

## Integrations

Columns: **Exact** is `restore.exact`, **Approx.** is `restore.approximate`, **Clock** is `uses_clock`, all
from each program's `describe`. **Verified** says what has run, and on what. A "live" test is `#[ignore]`d with
the prefix `live_` and needs Docker or a real server. Each folder's README has the details.

| Name | What it puts in the world | Exact | Approx. | Clock | Verified |
|---|---|---|---|---|---|
| `postgres` | A Postgres database per instance | save | no | no | Tests against the `postgres:17-alpine` server of `deploy/compose.yml`, with the `shakedown-part-check` run (`integrations/postgres/tests/part.rs`); the retail example runs on it through shaked |
| `folder` | A folder of files per instance | save | no | no | Tests on local disk with the `shakedown-part-check` run (`integrations/folder/tests/part.rs`); no service involved |
| `service` | Any Docker Compose project per instance | save | no | no | `live_service_roundtrip` against a real Redis 6.2 project in Docker. No `shakedown-part-check` run |
| `live-service` | A Docker Compose platform on the world clock (clock file, deploys as a world tool, probes and watches) | save | no | yes | `live_platform_deploys_a_new_image` against the software factory's real platform: deploy, world-time telemetry, save and restore |
| `mariadb` | A MariaDB database per instance (saves are binary-log positions) | save | no | no | Seven `live_mariadb_*` tests against MariaDB 11.8 in Docker, including `shakedown-part-check` |
| `erpnext` | An ERPNext site per instance (saves are binary-log positions plus the site's files) | save | no | no | `live_erpnext_roundtrip` against ERPNext v16.53.0 in Docker, including `shakedown-part-check` |
| `twenty-crm` | A Twenty CRM server per instance (saves are copies of its Postgres database) | save | no | no | `live_twenty_roundtrip` against Twenty v2.51.0 in Docker, including `shakedown-part-check` |
| `sculptured` | A CopperTide server per instance, for approvals | save | no | no | `live_sculptured_roundtrip` against a real `sculptured` build; `shakedown-part-check` passed once by hand |
| `ridgeline-hub` | A factory's simulated suppliers, carriers, insurer and customers | save | no | yes | Python tests against the real hub `market.py`, with `shakedown-part-check`. No services, no shaked run |
| `isaac-sim` | A factory plant, on Isaac Sim physics or a stand-in | replay | yes | yes | **Stand-in only.** Python tests with `shakedown-part-check`. The `isaac` backend has never run on Isaac Sim |
| `isaac-sim-mirror` | A read-only view of a running plant | none | no | yes | One Python test against a stub HTTP server. A world containing it can't be rerun |
| `utility` | A simulated electric utility: the site meter fed from the plant's load, tariff, demand-response events | save | no | yes | Python tests with `shakedown-part-check`. No shaked run |
| `ot-network` | A simulated plant OT network: zones, conduits, the IT/OT gateway's sessions, the incident playbook | save | no | no | Python tests with `shakedown-part-check`. No shaked run |
| `shift-roster` | A simulated shift roster: stations, operators, certifications, call-outs and call-ins | save | no | no | Python tests with `shakedown-part-check`. No shaked run |
| `line-sim` | A plant's discrete-event simulation model for an optimisation study: simulate configs, a budget, hard limits, a verified recommendation | save | no | no | Python tests (model determinism and common random numbers, limits, budget, verification, saves); real Claude Code runs through shaked with `benchmarks/line2-oee` |
| `counter` (Rust), `counter-py` (Python) | Kit examples: one integer per instance | save (flavours: replay, approximate) | no (flavour) | yes | Kit and core tests (`kits/*/tests`, `core/crates/parts/tests`, shaked tests) |

Only `postgres` (through the retail example) has been used in a shaked run with agents. The other
integrations are checked on their own, not inside a team's run. Three of them need code that is not public yet:
`sculptured` runs the CopperTide program, and `ridgeline-hub` and `isaac-sim` (with its mirror) wrap the Ridgeline
factory example. Without those, their tests are skipped.

## Extend Shakedown

- **Write an integration:** a folder with an `shakedown-package.toml` and a program that speaks the protocol. Start
  from the counter in [integrations/README.md](integrations/README.md), then check it with `shakedown-part-check`.
- **Write a benchmark:** a package with `kind = "benchmark"` that answers `describe`, `seed`, `call_tool` and
  `gold_actions`. `benchmarks/retail` is the example; see [docs/packages.md](docs/packages.md#write-a-benchmark).
- **Install a package:** put its folder under one of the `SHAKEDOWN_PACKAGES` roots. `command` runs are for
  programs you trust; `image` runs are boxed in a container ([docs/packages.md](docs/packages.md)).

## Documentation

| Document | What it covers |
|---|---|
| [docs/sdk.md](docs/sdk.md) | Using Shakedown from code: the Python SDK, external agents, the step-loop env, export, reproducibility, Rust, citing |
| [docs/agents.md](docs/agents.md) | Bringing your own deployed agent: the `shakedown-agent/1` protocol, Bedrock agents, secrets, what is verified |
| [docs/concepts.md](docs/concepts.md) | Glossary, restore modes, teams and runners, external designs, overhead and estimates, CLI isolation, forking, cost labels |
| [docs/protocol.md](docs/protocol.md) | `shakedown-part/1`: transport, manifest, every call, errors, clock and determinism contracts |
| [integrations/README.md](integrations/README.md) | The integrations, a step-by-step counter integration, `shakedown-part-check` |
| [docs/packages.md](docs/packages.md) | Writing and installing integrations and benchmarks |
| [docs/reference.md](docs/reference.md) | HTTP API, configuration, disk use and cleanup, tests |
| [docs/measurements.md](docs/measurements.md) | Measurements made with the retail example benchmark |

## Project status

Shakedown is early. Only `postgres` has been used in a shaked run with agents, through the retail example; every
other integration is checked on its own, and API mode has not been measured.

<details>
<summary><b>Limits and not built yet</b></summary>

- Only `postgres` has been used in a shaked run with agents; every other integration is checked on its own.
- Two benchmarks are included: the retail example and `benchmarks/factory`, which needs a factory example that is
  not public yet. shaked runs one benchmark at a time.
- shaked's own runners run a team one after another or all at once; any other shape needs the design's own code
  to drive the run through the external API (`docs/concepts.md`, "External designs"), which is tested with
  Claude Code as the external system's agent (`core/crates/shaked/tests/external.rs`, `live_`). Any agent may
  write unless its team marks it `read_only`.
- The production estimate (measured time minus Shakedown's overhead) has been checked by tests only, not against a
  production deployment. Costs not reported by a provider are estimated only for the models in
  `docs/prices.toml`; every other cost shows as unknown.
- API mode has not been measured with any provider. Its providers' request and answer translation is unit-tested
  on data; its `live_` tests call a real provider only when its key is set, and none has run yet (only a rejected
  key, against OpenAI and the Claude API).
- Isaac Sim on hardware: the `isaac` backend of `isaac-sim` has never run on Isaac Sim 6.2 (needs a 16 GB GPU).
- Cross-part calls: parts only reach each other through a benchmark's `links`, one tool call per event
  (`docs/protocol.md`); tested with toy parts. `proposal_id` on the `ridgeline-hub` and `isaac-sim` tools binds
  only in a world whose benchmark puts a gate on them.
- A world with real agents made of several integrations has not been run. `benchmarks/factory` builds one (ERPNext,
  the hub, CopperTide and the plant) and reads a factory example that is not public yet; its live test now runs the
  benchmark's own team on a real model and has not been run yet (earlier runs used a scripted team, since removed).
- Comparing a world made only of replay parts: there is no final save to compare, so compare answers 409 and
  the branch fails with "no part of this world can be checked against the task's expected result".
- Community publishing of integrations and benchmarks (design only). Packages install from a local folder.
- Process checkpointing (CRIU), fault injection, fault search and design search.
- SGLang or vLLM hosting, serving-state capture and chain-of-thought capture.
- Dashboards.

</details>

## Old names

These were renamed on 2026-11-19. The old names still work, so code, packages and scripts written before keep running:

| Old name | New name | What still works under the old name |
|---|---|---|
| `arenad`, the service (`core/crates/arenad`) | `shaked` (`core/crates/shaked`) | `cargo run -p arenad`, `cargo build -p arenad` and the program `core/target/<profile>/arenad`: the package `core/crates/arenad-alias` builds the same program under the old name |
| the Python SDK `arena` (distribution `arena-sdk`, class `Arena`) | `shakedown` (distribution `shakedown-sdk`, class `Shakedown`) | `import arena` and `from arena.<module> import ...` give the same objects and warn once; `Arena`, `LocalArena`, `ArenaError`, `ArenaParallelEnv`; `sdk/python/compat/arena-sdk` installs `shakedown-sdk` under the old distribution name; `Shakedown.local(arenad=...)`, and a build named `arenad` is found; `branch.arena` (and a suite's, a driver run's, an env's) is `.shakedown` |
| settings `ARENA_<NAME>` (environment variables, `VITE_ARENA_*` for the UI build) | `SHAKEDOWN_<NAME>`, `VITE_SHAKEDOWN_*` | every old name is read when the new one is not set, with one warning per name; every program shaked or a part starts (part programs, drivers, docker compose projects) gets both names |
| the protocols `arena-part/1` and `arena-agent/1` | `shakedown-part/1` and `shakedown-agent/1` | a part's `protocol` may be `1`, `"shakedown-part/1"` or `"arena-part/1"` (the kits now announce the new name); an HTTP agent that refuses `shakedown-agent/1` (a 4xx answer) is asked again under `arena-agent/1` |
| `arena-package.toml`, `${ARENA_BIN_DIR}`, `${ARENA_DIR}` | `shakedown-package.toml`, `${SHAKEDOWN_BIN_DIR}`, `${SHAKEDOWN_DIR}` | a folder with only `arena-package.toml` is read (the new file wins when both are there), and both placeholders expand |
| the programs `arena-<package>` (`arena-retail`, `arena-postgres`, ..., `arena-part-check`) | `shakedown-<package>`, `shakedown-part-check` | the old programs are built too |
| the Cargo packages `arena-<package>` (crates `arena_<package>`) and the Rust kit `arena-part` (`arena_part`) | `shakedown-<package>` (`shakedown_<package>`), `shakedown-part` (`shakedown_part`) | `cargo build -p arena-factory`, `-p arena-erpnext` and `-p arena-twenty-crm`, which the example stacks run: the alias packages `<package>/arena-alias` build the same programs; an outside Rust package depends on `shakedown-part` |
| the Python kit `arena_part` (distribution `arena-part`) | `shakedown_part` (distribution `shakedown-part`) | `import arena_part` and its modules give the same objects and warn once; `kits/python/compat/arena-part` installs `shakedown-part` under the old name |
| the proxy's request headers `X-Arena-Agent`, `X-Arena-Readonly`, `X-Arena-Effect`, and its user `arena` | `X-Shakedown-*`, user `shakedown` | the proxy sends the agent and read-only headers under both names, takes a part's effect under either, drops both from what a client sends, and takes proxy URLs with the user `arena` |
| the MCP server name `arena` (tools `mcp__arena__*`, for the SDK's Claude Code and an empty benchmark) | `shakedown` (`mcp__shakedown__*`) | nothing: prompts that name `mcp__arena__` tools must say `mcp__shakedown__` |
| the dataset format `arena-dataset/1` | `shakedown-dataset/1` | the same layout; `shakedown.export.is_dataset_format` takes both |
| docker resources: `arena-mariadb` / `arena_mariadb_data`, `arena-erpnext-db` / `arena_erpnext_db` / `arena_erpnext`, `arena_twenty`, label `arena.part`, `arena_<instance>` | `shakedown-…`, `shakedown_…`, label `shakedown.part` | a long-lived server (MariaDB, ERPNext's, Twenty's) keeps its old name while it is the only one, so its saves stay; a container labelled `arena.part` still counts as the helper's; new servers and instances get the new names |
| browser storage `arena.*` | `shakedown.*` | the UI moves each old key once, on load |
| Postgres user and password `arena`, and the database `arena` of shaked's records | `shakedown` (new deployments: `deploy/compose.yml`) | with no URL set, shaked and the packages connect as `shakedown`, else as `arena`; a server `deploy/compose.yml` makes has both logins; it keeps using a database `arena` while there is no `shakedown`; the postgres integration reads `?arena_ns=` on the URL as `?shakedown_ns=`. To rename by hand (shaked stopped): `docker exec deploy-pg-1 psql -U arena -d postgres -c "CREATE ROLE shakedown LOGIN SUPERUSER PASSWORD 'shakedown'" && docker exec deploy-pg-1 psql -U shakedown -d postgres -c "ALTER DATABASE arena RENAME TO shakedown"` |
| `SHAKEDOWN_PORT` (2026-11-20) | `SHAKEDOWN_BIND` (`127.0.0.1:4510`) | `SHAKEDOWN_PORT=<port>` still binds `127.0.0.1:<port>` |
| `SHAKEDOWN_AGENT_BASE` (2026-11-20) | `SHAKEDOWN_PUBLIC_URL` (the base of every URL shaked hands out) | still read, and it wins for HTTP agents |
| `SHAKEDOWN_LOCAL_MODELS` for your own model servers (2026-11-20) | custom servers in Settings → Models (`custom:<name>`, kept in the database with their keys sealed) | still read; a custom server with the same name takes over from it |
| shaked's tables in schema `public` (2026-11-20) | schema `shakedown` | moved there on the first start, runs and saves kept |
| `deploy/runpod`: `arenad_provider` in `pod.toml`, `gpupod --arenad`, `--through-arenad` | `shaked_provider`, `--shaked`, `--through-shaked` | the old key and flags are read |

Kept as they are, because they are data inside existing servers and saves: the MariaDB helper's database `arena_meta`, ERPNext's base site `arena-base.localhost`, the database servers' root password `arena` and Twenty's own user `arena`, and the Sculptured team and proposer `arena`.

## Contributing

Contributions are welcome: integrations, benchmarks, fixes and docs. Read [CONTRIBUTING.md](CONTRIBUTING.md)
for setup, the checks to run and the code rules, and follow the [Code of Conduct](CODE_OF_CONDUCT.md). Report
security problems privately as described in [SECURITY.md](SECURITY.md).

## License

Shakedown is licensed under the [Apache License 2.0](LICENSE).

The retail data in `data/retail/` (`db.json`, `tasks.json`, `policy.md`) comes from
[tau2-bench](https://github.com/sierra-research/tau2-bench), MIT License, Copyright (c) 2025 Sierra Research;
see [data/retail/LICENSE](data/retail/LICENSE) and [NOTICE](NOTICE).

## Examples and demos

Worlds, demos and showcases built on Shakedown (the factory showcase, the local pharma stack) live in the separate
`Shakedown-Examples` repo, next to this one, so this repo stays the core.
