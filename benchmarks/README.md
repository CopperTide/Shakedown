# Benchmarks

A benchmark is a package that names a world (a list of parts on [integrations](../integrations/README.md)) and
brings the tasks, the instructions every agent gets, its own tools, the data that seeds the world, and a way to
score each run: a gold world to compare against, or named checks on the end state. `shaked` finds benchmarks here,
next to the integrations, under `SHAKEDOWN_PACKAGES`. [`docs/packages.md`](../docs/packages.md) covers writing one,
and [`docs/protocol.md`](../docs/protocol.md), "Benchmark calls", has every call.

Each folder's README says what the benchmark measures, where its tasks come from, what it needs, how to run it, how
it is tested and its limits. No scoring here calls a model.

## The benchmarks

| Benchmark | What it measures | Scoring | Tasks from |
|---|---|---|---|
| [`retail`](retail/README.md) | Retail customer service under a store policy: cancel, modify, exchange and return orders, with four optional faults | final database against the gold database | [τ²-bench](https://github.com/sierra-research/tau2-bench) retail (MIT) |
| [`factory-01`](factory-01/README.md) | A supplier delay puts a key order at risk under a USD 150 purchase limit, in one simulated ERP | 6 named checks (3 safety) | written by CopperTide, fictional |
| [`plant-floor`](plant-floor/README.md) | A supplier fire, a rush order and a locked-out robot cell, re-planned through OPC UA tags and a simulated ERP | 5 named checks (2 safety) | written by CopperTide, fictional |
| [`line2-oee`](line2-oee/README.md) | An optimiser agent with a discrete-event plant model and a budget of 30 simulations reaching an OEE goal within limits | 5 named checks on the re-simulated recommendation (1 safety) | written by CopperTide, fictional |
| [`pharma`](pharma/README.md) | A sterile-fill environmental-monitoring excursion in five scenarios across ERPNext, Twenty, CopperTide approvals, a simulated LIMS and carriers | 14 named checks (5 safety) | written by CopperTide, fictional |
| [`factory`](factory/README.md) | A stamping-plant press failure and a disrupted shift across ERPNext, Twenty, CopperTide approvals, a plant and three simulations. Needs a factory example that is not public. | 12 and 9 named checks, safety included | written by CopperTide, fictional |

## Built worlds

[`world/`](world/README.md) is the world builder (package `world-builder`), not a benchmark with fixed tasks. It
fills a template from a user's choices and a seed and writes the result as a benchmark package that `shaked` runs.
Each built world's tasks are scored by named checks on the end state. [`software-factory/`](software-factory/README.md)
is that template built with its defaults and committed, for hosted Shakedown, which builds no world.

| Template | What a built world measures | Scoring |
|---|---|---|
| [`small-manufacturer`](world/library/small-manufacturer/README.md) | A pump plant's ERP and shift roster: keep a key account's order on track when a supplier slips, within a USD 150 purchase limit | 6 named checks (3 safety) |
| [`software-factory`](world/templates/software-factory/README.md) | A software company (code host, CI, tracker, three services in Docker, observability, chat, pager, simulated engineers): ship a sprint with traps while production stays healthy | `sprint` 8 and `contract-change` 5 named checks (2 safety each) |

## Run one

A run names its benchmark (`POST /api/runs`'s `benchmark`, or a project world's `package`). Otherwise `shaked` uses
the current one: `SHAKEDOWN_BENCHMARK`, or the one chosen with `POST /api/worlds/{name}/use`. Every benchmark here
except `retail` sets `opt_in = true` in its manifest: `shaked` never picks it as the only benchmark installed, and
does not list it for choosing unless it is the current one. Built worlds are always listed. The
[user guide](https://docs.coppertide.ai) covers worlds, scenarios and experiments in the app.
