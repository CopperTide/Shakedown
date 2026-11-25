# Line 2 OEE optimisation (`line2-oee`)

Measures whether an optimiser agent, or a team of them, can use a plant's simulation model to find a configuration
that reaches an engineer's goal within the plant's limits and a fixed budget of simulations, and back it with
evidence that holds when the plant re-simulates it. It also compares optimiser designs: how often each reaches the
goal, with how many simulations. The plant, the task and the data are written by CopperTide and are fictional. The
world is one simulated part, a discrete-event model of two production lines, so the benchmark needs no Docker.

| | |
|---|---|
| Program | `python -m line2_oee` (Python, run with `uv`) |
| World | one part, `sim`, on the [`line-sim`](../../integrations/line-sim/README.md) integration |
| Tasks | 1: `line2-oee` |
| Scoring | named checks over the re-simulated recommendation, one of them a safety check. No model judges. |
| Suggested team | Single optimiser (the default), Planner and analyst, Optimiser with a search strategy |

## What it measures

The plant is Ridgeline Pumps (`data/plant.json`): two lines that share two maintenance technicians, 13 people per
shift. Line 1 makes P-200 pumps on four stations and is committed to 2,650 good pumps a week. Line 2 makes V-50 and
V-80 valves on machining, assembly, test and pack, in campaigns with a changeover between variants, and customers
are promised a 4-hour lead time. Today's configuration runs Line 2 at 77.2% OEE, verified on 20 replications.

The plant manager's ticket: get Line 2 to 85% OEE with no extra headcount, within the USD 15,000 capex budget,
without breaking Line 1's commitment or Line 2's lead time, using at most 30 simulations. Recommend one simulated
configuration with its evidence.

The agents get the model's tools: `describe_model`, `run_simulation`, `get_history` and `submit_recommendation`.
They may change Line 2's three buffer sizes, campaign size, speed (85% to 115%), planned maintenance hours per week
and crew per station, Line 1's crew per station, and the technicians per shift. The part refuses a configuration
that breaks a hard limit before it simulates anything, as a `denial` event, and a refused call uses no simulation.
The hard limits are the bounds, the headcount, the capex budget and the safety minimums (an operator at every
station, a technician per shift, at least 2 h of planned maintenance per line). See the `line-sim` README for the
model.

### Checks

`describe` says `scored: true`. When the agents submit, the part re-simulates the recommendation on 20
replications the study never used. The checks read that verified result, never the agent's own numbers
(`line2_oee/checks.py`):

| Check | Kind | Passes when |
|---|---|---|
| `recommendation_submitted` | goal | a simulated configuration was recommended |
| `goal_reached` | goal | its verified Line 2 OEE is at least 85%. Its `measure` is the verified OEE, the run's score. |
| `evidence_holds` | goal | the verified OEE is no more than 1.5 points below the simulation given as evidence |
| `within_budget` | goal | at most 30 simulations were used. Its `measure` is the number used. |
| `constraints_respected` | safety | the configuration breaks no hard limit and, verified, keeps Line 1's commitment and Line 2's lead time |

### The three suggested teams

All three use the same tools. The benchmark offers the first as the default team and the others beside it.

- **Single optimiser** (one agent, `chain`): an industrial engineer improving a plant with its simulation model.
- **Planner and analyst** (`dispatch`, the planner leads): the planner decides what to try and submits the
  recommendation. The analyst runs the simulations it is given and reports the results and the bottleneck.
- **Optimiser with a search strategy** (one agent, `chain`): the single optimiser plus a written strategy. Simulate
  today's configuration first, find the bottleneck from the station states, change one lever at a time and then
  combine, look for idle capacity to move within the headcount, screen with 3 to 5 replications and confirm with 10.

### A reference number

`scripts/reference.py` runs a plain random search offline, through the same part and tools, with the same budget:
today's configuration first, then 29 drawn uniformly within the bounds (skipping any a hard limit would refuse), 5
replications each. It then submits the best feasible one, which the part verifies on 20 new replications.
`data/reference.json` holds 10 such searches, made on 2026-11-08: verified OEE 77.2% median and 80.3% best, and none
reached 85%. `goal_reached` carries the median as its `measure.reference`, a number to read the score against. It
is not a run.

### The world's view

`view` draws Line 2's stations, the best candidate so far, the recommendation and any denials, with KPIs for the
simulations used and the verified OEE (`line2_oee/twin.py`).

## Where the tasks come from

CopperTide wrote the plant, the task, the limits and the checks for this benchmark. Ridgeline Pumps and its numbers
are invented. The simulation is `line-sim`'s own discrete-event model (`integrations/line-sim/line_sim/des.py`),
not a commercial simulator. Nothing comes from an outside benchmark or dataset.

## What it needs

- `uv` and Python 3.10 or later. `shaked` runs the benchmark and the `line-sim` part with `uv run --project .`.
- No Docker, no services and no GPU.

## Run it

`line2-oee` is `opt_in`: `shaked` never picks it as the only benchmark installed, and does not list it unless it is
the current one. Name it: as a run's `benchmark` (`POST /api/runs`), or as the current benchmark with
`SHAKEDOWN_BENCHMARK`. See [`docs/packages.md`](../../docs/packages.md).

```bash
docker compose -f deploy/compose.yml up -d                  # shaked's Postgres on 127.0.0.1:5544
(cd core && SHAKEDOWN_BENCHMARK=line2-oee cargo run -p shaked)
```

To remake the reference number (it overwrites `data/reference.json`):

```bash
(cd benchmarks/line2-oee && uv run python scripts/reference.py 10)
```

## How it's tested

```bash
(cd benchmarks/line2-oee && uv run --with pytest pytest -q)
(cd integrations/line-sim && uv run --with pytest pytest -q)    # the simulation part
```

The benchmark's tests run against the real simulation part, in process. A study that reaches the target with
evidence passes every check, and its score is the verified OEE. No recommendation fails the three goals it should
and not safety. Recommending today's configuration misses only the goal. Configurations that break Line 1's
commitment or the lead time fail the safety check. Evidence far above the verified result fails `evidence_holds`.
Refused configurations use no budget. The tests also cover the reference number, the view, an unknown task, and
`describe` and `score` over the protocol. No test is marked `live`.

## Limits

- One task, one plant. The model is a compact simulation written for this benchmark, not a model of a real plant.
- `line-sim` runs only its own `des` backend. A commercial simulator is not connected.
- The reference is a random search, a floor to read scores against, not a strong baseline.
