# Integrations

An integration is a program that puts one kind of system in a Shakedown world: a database, a folder, an ERP, a code
host, a simulated plant. It works like a device driver. `shaked` knows nothing about the system. It speaks the
protocol `shakedown-part/1` to the program, and the program does the rest: start an instance, run tool calls, save
the state, restore it, show it and compare it. The protocol is [`docs/protocol.md`](../docs/protocol.md), and
[`docs/packages.md`](../docs/packages.md) covers writing and installing packages.

Each integration is a folder here with a `shakedown-package.toml`. It uses only the kits (`kits/rust`,
`kits/python`), never a `core/` crate, and the core never depends on it. `core/crates/parts/tests/isolation.rs`
checks this. Benchmarks, which bring tasks, tools and checks, are separate packages under
[`benchmarks/`](../benchmarks/README.md).

## The integrations

Restore is what the integration declares in `describe` (see [Restore modes](#restore-modes)). Clock says whether it
runs on the world clock. Each folder's README says what it needs, how to use it, how it is tested and its limits.

| Integration | What it puts in the world | Restore | Clock |
|---|---|---|---|
| [`postgres`](postgres/README.md) | A Postgres database per instance | exact (save) | no |
| [`folder`](folder/README.md) | A folder of files per instance | exact (save) | no |
| [`service`](service/README.md) | Any Docker Compose project per instance, with the volumes you list | exact (save) | no |
| [`live-service`](live-service/README.md) | A Docker Compose platform on the world clock: a clock file, deploys as a world tool, probes and watches | exact (save) | yes |
| [`mariadb`](mariadb/README.md) | A MariaDB database per instance. Saves are binary-log positions. | exact (save) | no |
| [`erpnext`](erpnext/README.md) | An ERPNext site per instance. Saves are binary-log positions plus the site's files. | exact (save) | no |
| [`twenty-crm`](twenty-crm/README.md) | A Twenty CRM server per instance, on its own database | exact (save) | no |
| [`sculptured`](sculptured/README.md) | A CopperTide server per instance, for a team's proposed changes and approvals | exact (save) | no |
| [`github-twin`](github-twin/README.md) | A GitHub Enterprise Server-compatible code host over real git repositories | exact (save) | yes |
| [`jira-twin`](jira-twin/README.md) | A Jira-compatible tracker: issues, JQL, boards and sprints | exact (save) | yes |
| [`slack-twin`](slack-twin/README.md) | A Slack-compatible workspace: the Web API chat methods | exact (save) | yes |
| [`pagerduty-twin`](pagerduty-twin/README.md) | A PagerDuty-compatible pager: incidents, notes, on-call and Events API v2 | exact (save) | yes |
| [`people`](people/README.md) | Simulated people who act on the world clock by rules written as data | exact (save) | yes |
| [`ridgeline-hub`](ridgeline-hub/README.md) | A fictional factory's outside companies: suppliers, carriers, insurer and customers | exact (save) | yes |
| [`isaac-sim`](isaac-sim/README.md) | A factory plant, on Isaac Sim or a stand-in without a GPU | exact by replay, approximate | yes |
| [`isaac-sim-standin`](isaac-sim-standin/README.md) | The `isaac-sim` plant on its stand-in backend, as a package of its own | exact by replay, approximate | yes |
| [`isaac-sim-mirror`](isaac-sim-mirror/README.md) | A read-only view of a plant that is already running | none | yes |
| [`plant-opcua`](plant-opcua/README.md) | A simulated plant floor served over OPC UA, with browse, read and write tools | exact (save) | no |
| [`line-sim`](line-sim/README.md) | A discrete-event model of production lines for an optimisation study, with a simulation budget and hard limits | exact (save) | no |
| [`utility`](utility/README.md) | A simulated electric utility: the site meter fed from the plant's load, a two-rate tariff, demand-response windows | exact (save) | yes |
| [`ot-network`](ot-network/README.md) | A simulated plant OT network: zones, conduits, an IT/OT gateway's remote sessions, an incident playbook | exact (save) | no |
| [`shift-roster`](shift-roster/README.md) | A simulated shift roster: stations, operator certifications, call-outs and call-ins | exact (save) | no |
| [`pharma-lims`](pharma-lims/README.md) | A simulated LIMS: environmental monitoring, batch release tests, lab documents, a rule-based QA head | exact (save) | no |
| [`pharma-logistics`](pharma-logistics/README.md) | Simulated cold-chain carriers and a customer desk, with a rule-based supply chain director | exact (save) | no |
| [`factory-01-erp`](factory-01-erp/README.md) | A small simulated ERP: stock and reservations, schedule, suppliers, purchasing with a hard limit, flags | exact (save) | no |

Kit examples to copy: [`kits/rust/examples/counter.rs`](../kits/rust/examples/counter.rs) (package `counter`) and
[`kits/python/examples/counter.py`](../kits/python/examples/counter.py) (package `counter-py`), plus the HTTP twin
example [`kits/python/examples/notes_twin.py`](../kits/python/examples/notes_twin.py).

Some integrations need code from outside this repository. `sculptured` runs a CopperTide server build that has
the route for outside proposers, which as of 2026-11-22 only the CopperTide repository's `platform` branch has.
`ridgeline-hub`, `isaac-sim` and `isaac-sim-standin` read the Ridgeline factory example, which is not public, and
`isaac-sim-mirror` follows a running plant from that example. Without them their tests are skipped. Each README says
so.

## Restore modes

Each integration says in `describe` how it restores:

- `exact: "save"`: it saves its state exactly and starts from that save.
- `exact: "replay"`: it can't save exactly. `shaked` starts it again with the same seed and repeats every recorded
  input. The part must be deterministic for this to work.
- `exact: "none"`: no exact restore.
- `approximate: true`: it also starts from a save that may miss some state. A rerun that uses this is labelled
  "approximate restore".

A rerun asks for `exact` (the default) or `approximate`. `core/crates/shaked/src/restore.rs` holds the table.

## Write one: the counter, step by step

The counter holds one integer per instance. Its Rust code is `kits/rust/examples/counter.rs`.

1. Make a folder with a `shakedown-package.toml` (see `kits/rust/examples/counter/shakedown-package.toml`): `kind`,
   `name`, `version`, `protocol = 1`, `run`, `needs`, `config_schema`, and one `[[example]]` per tool call the check
   may run.
2. Depend on the kit crate `shakedown-part`. Write `impl Part for Counter` and end `main` with
   `serve(Counter::new(..)).await`. The kit reads and writes the lines and checks the ids.
3. `describe` returns the name, `restore` (the default flavour: `{exact: Save, approximate: false}`), `uses_clock`, a
   `fingerprint` and the tools. `add` has `effect: Write`, `get` has `effect: Read`.
4. `start` makes an instance. If `from` is set, it reads that save instead. A part that can't restore in that mode
   answers `-32001`.
5. `call_tool` runs `add` or `get` and lists the keys it read and wrote (`counter/value`).
6. `advance` moves the clock.
7. `save` writes `state.json` into `dir` and returns `{bytes, exact: true}`. `inspect`, `diff`, `drop_save`, `stop`
   and `usage` are small. A part whose saves all sit in `SHAKEDOWN_SAVES` reports `usage` 0.
8. Build it, then check it (below). Put the folder where `shaked` looks (`SHAKEDOWN_PACKAGES`, default
   `integrations/` and `benchmarks/`) and start `shaked`.

Python is the same with `shakedown_part.Part` and `shakedown_part.serve(Counter())`. Its manifest is
`kits/python/examples/shakedown-package.toml`.

A command-mode program runs in its package folder with a cleared environment: it gets only `PATH`, `HOME`,
`SHAKEDOWN_PG_URL`, `UV_CACHE_DIR`, `XDG_CACHE_HOME`, `RUST_LOG`, `SHAKEDOWN_SAVES`, and `SHAKEDOWN_TEMPLATES` when
set. Settings an integration needs under `shaked` belong in its config. `SHAKEDOWN_RUN_<PACKAGE>` (upper case, `-`
as `_`) in `shaked`'s environment replaces a package's run command.

## Check it

From the repository root, after building the integration's own program:

```bash
(cd core && cargo build -p parts --bin shakedown-part-check)
SHAKEDOWN_BIN_DIR=core/target/debug core/target/debug/shakedown-part-check integrations/<name> [--config file.json]
```

`--config` takes a file that holds the config as JSON. The check starts the program and runs, in order: `describe`,
`start`, each `[[example]]` tool call, save and restore in each mode the part declares (exact must match, an
approximate difference is reported but does not fail), `diff` of a save against itself and against the original,
replay for a part that declares it, and `drop_save` lowering `usage`. It prints a JSON report and exits 0 if all
passed, 1 if not, and 2 if it could not run. A step it can't do is listed as skipped, with the reason.
`SHAKEDOWN_BIN_DIR` is where `${SHAKEDOWN_BIN_DIR}` in the manifest points (default: the release build). Without
`SHAKEDOWN_SAVES` it uses a temporary folder.

Some folders have a `check/` package: the same program with a config and examples made for the check (`postgres`,
`mariadb`, `erpnext`, `twenty-crm`, `folder`).

## Helpers in the kits

- `kits/rust` (crate `shakedown-part`) has helpers, most behind cargo features: `folder` (a folder copied as a save,
  with hard links), `compose` (Docker Compose projects and their volumes), `postgres`, `mariadb` (binary-log saves,
  used by `mariadb` and `erpnext`) and `http` (HTTP helpers, Frappe and Twenty clients). `sql_tools`,
  `process_group` and `world_time` need no feature.
- `kits/python` (package `shakedown_part`) has `JsonStatePart` and `ClockedJsonStatePart` (one JSON document of
  state, saved exactly), `DocsPart`, `twin` (HTTP twins reached through `shaked`'s proxy), `people` (simulated
  deciders), `disruptions` (scheduled disruptions), `expect` (end-state checks written as data), `world_time`,
  `FixedStep` (a fixed-step clock), `folder`, `saves` and `sources`.
