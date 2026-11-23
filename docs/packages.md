# Packages: integrations and benchmarks

Everything system-specific in Shakedown is a package: a folder with an `shakedown-package.toml` and a program that
speaks [`shakedown-part/1`](protocol.md). shaked finds packages under the `SHAKEDOWN_PACKAGES` roots (default
`integrations/` and `benchmarks/`) and starts their programs itself. The core never depends on a package, and a
package never depends on the core: it uses only the kits.

## Write an integration

A folder with an `shakedown-package.toml` (`kind = "integration"`, `name`, `version`, `protocol`, `run`, `needs`,
`config_schema`, and `[[example]]` tool calls) and a program that speaks the protocol. The Rust kit
(`kits/rust`, crate `shakedown-part`) and the Python kit (`kits/python`, package `shakedown_part`) read and write the
lines and check the ids; you fill in `describe`, `start`, `call_tool`, `advance`, `save`, `inspect`, `diff`,
`drop_save`, `stop` and `usage` (`gc` and `connect` are optional). `integrations/README.md` walks through the
counter example (`kits/rust/examples/counter.rs`, `kits/python/examples/counter.py`). Then check it, from the
repository root:

```bash
cargo build                                                  # your integration (debug build)
(cd core && cargo build -p parts --bin shakedown-part-check)     # the checker
SHAKEDOWN_BIN_DIR=core/target/debug core/target/debug/shakedown-part-check integrations/<name> [--config file.json]
```

`shakedown-part-check` runs the program and checks describe, start, the examples, save and restore, diff, replay
and `drop_save`. It prints a JSON report and exits 0 if all passed, 1 if not, 2 if it could not run.

Install a package by putting its folder under one of the `SHAKEDOWN_PACKAGES` roots. A `command` run is for
programs you trust; it runs in the package folder with only `PATH`, `HOME`, `SHAKEDOWN_PG_URL`, `UV_CACHE_DIR`,
`XDG_CACHE_HOME`, `RUST_LOG`, `SHAKEDOWN_SAVES` and, when it is set, `SHAKEDOWN_TEMPLATES` in its environment. An `image` run is boxed in a container: no
network unless the manifest asks for host networking (`needs.network = "host"`), no environment from shaked,
and only its own saves folder, mounted at `/saves`.

## Write a benchmark

A benchmark is a package with `kind = "benchmark"` in its `shakedown-package.toml` and a program that speaks the same
transport (`docs/protocol.md`, "Benchmark calls"). It answers:

- `describe`: its name and version, the world (`[{part, integration, config}]`), the tasks (`[{id, reason,
  ticket}]`), the instructions every agent gets, and its tools (MCP shape plus `effect: "read"|"write"`).
  Optionally `instructions_title`, the heading over the instructions in each agent's input (default
  `Instructions`), and `team`, the team it suggests (`{id, name, agents: [{id, name, prompt}]}`).
  The CLIs know the world's MCP server by the benchmark's name.
- `seed`: load the starting data into fresh instances. shaked saves the result once as the base world that
  every run starts from.
- `call_tool`: run one of its tools and say which part it touched and which keys it read and wrote.
- `gold_actions`: the tool calls that solve a task. shaked applies them to the base world once per task to make
  the gold world that branches are scored against.

A benchmark's tools reach parts only through the parts' `connect` info (for `postgres`, `{url}`), so only parts
with exact saves may be connected. In Rust, implement `shakedown_part::Benchmark` and end `main` with
`serve_benchmark(..)`. In Python, the kit's `Benchmark` (`kits/python/shakedown_part/benchmark.py`) does the same;
`benchmarks/factory-01`, `plant-floor` and `line2-oee` use it.

`benchmarks/retail` is the included example: a Rust program (`src/main.rs`, `src/lib.rs`) whose world is one
`postgres` part named `retail`; `src/load.rs` seeds it from `data/retail/db.json`, `src/tools/` holds the tools,
and `src/tasks.rs` reads the tasks and gold actions from `data/retail/tasks.json` (the tasks offered are the
ids in `data/retail/demo_tasks.json`); `src/team.rs` holds the team it suggests and the `Store policy` heading
over its policy. Its tests are in `benchmarks/retail/tests`. Each run names the benchmark it runs on (`POST /api/runs`'s
`benchmark`, a project world's `package`), else the current one: `SHAKEDOWN_BENCHMARK`, or the one chosen with
`POST /api/worlds/{name}/use`.

`benchmarks/factory` is a benchmark over eight integrations (`erpnext`, `twenty-crm`, `ridgeline-hub`, the stand-in
plant, `sculptured`, and three simulations: `utility`, `ot-network`, `shift-roster`) with two tasks, a press bearing
failure and a shift whose scenario opens up to four fronts. It shows what a world of several parts adds: `links` between
parts, `gates` on tools (approvals, and interlocks between the plant, the OT network and the roster), scoring by named
checks with `scored` and safety checks, scenario options and a twin `view` (`docs/protocol.md`). It reads a factory example that is not public yet, so its
world can't start without it; see `benchmarks/factory/README.md`.

`benchmarks/pharma` is a benchmark of a production incident at a fictional sterile-injectables maker over five parts:
`erpnext`, `twenty-crm`, `sculptured` and two simulations, `pharma-lims` (a LIMS with a simulated QA head) and
`pharma-logistics` (cold-chain carriers and a customer desk with a simulated supply chain director). Its five tasks are
scenarios of one incident, each set by a task's `setup` (`docs/protocol.md`, "A task's setup"); links hand every
proposed change to the simulated deciders and their decisions back to the approvals part; its named checks include hard
safety checks (`safety: true`). See `benchmarks/pharma/README.md`.


## Templates and built worlds

`benchmarks/world` is the world builder: a Python package that reads templates (YAML checked by
`worldbench/schema/template.schema.json`) and generates a world from a template, the user's choices and a seed
(`docs/protocol.md`, "Builder calls"). A built world is itself a benchmark package: `shakedown-package.toml`
(`opt_in = true`, a `[world]` table naming its title and template), `world.json` and the folders the template
copies under `files/`. Its program is the same Python package run as `python -m worldbench --world world.json`,
which serves the world and scores each task by its checks (`shakedown_part.expect`).

- `kind = "builder"`: the package shaked asks for templates, previews and builds.
- Templates: the ones shipped with the builder are in `benchmarks/world/library/` (its manifest passes
  `--templates library`; `small-manufacturer`, see its README), beside its own `templates/` (`software-factory`:
  a GitHub twin with Actions CI, Jira, three services in Docker Compose on `live-service`, an observability stack,
  Slack, PagerDuty and simulated engineers).
- `SHAKEDOWN_TEMPLATES`: `:`-separated folders of more templates (each `<id>/template.yaml`).
- `SHAKEDOWN_WORLDS`: where built worlds are written (default `<repo>/core/worlds`, git-ignored), scanned at start like
  `SHAKEDOWN_PACKAGES`. A built world's command names `${SHAKEDOWN_DIR}`, the repository folder.

shaked asks the first builder installed (by name) for templates and previews, writes a built world into
`SHAKEDOWN_WORLDS`, installs it while it runs and prepares its base in the background (`docs/reference.md`,
`/api/templates` and `/api/worlds`).
