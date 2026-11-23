# Plant line simulation study (`line-sim`)

Puts a simulation study of a plant in a Shakedown world, as one part. An agent proposes configurations
(batch sizes, line speed, crew, buffers, planned maintenance), simulates them on a discrete-event model, reads
the KPIs and recommends one. The part enforces the study's hard limits and its simulation budget, records every
simulation, and checks the recommendation by simulating it again on replications the study never saw. The plant
comes in the part's config. The [`line2-oee`](../../benchmarks/line2-oee/README.md) benchmark gives it a two-line
valve and pump plant and uses it to test agents that run such a study.

| | |
|---|---|
| Program | `python -m line_sim` (Python, run with `uv`) |
| Stands in for | a commercial discrete-event simulation package. Only the open-source model here is built. |
| Restore | exact, from a save (one `state.json`). No approximate restore. |
| World clock | not used |
| Agent tools | `describe_model`, `run_simulation`, `get_history`, `submit_recommendation` |

## What it needs

- `uv` and Python 3.10 or later. The part depends only on the Python kit (`kits/python`, package
  `shakedown-part`).
- No Docker, no services and no GPU. The model runs in the part's own process.
- No environment variables of its own. It writes saves under `SHAKEDOWN_SAVES`, which `shaked` passes.

## Use it

A benchmark or world names it as a part with the whole plant as its config, for example
`{part: "sim", integration: "line-sim", config: <plant>}` (as `line2-oee` does). The `line2-oee` benchmark's plant is
`benchmarks/line2-oee/data/plant.json`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `seed` | integer | required | The plant's seed for every random stream. Every run of a benchmark simulates the same weeks. |
| `lines` | array | required | The lines, their stations, buffers, crews, variants, MTBF and MTTR, scrap and changeovers |
| `decisions` | array | required | What the agent may change: each decision's name, today's value, bounds and unit |
| `safety` | object | required | Safety minimums (operators per station, technicians per shift, PM hours per line) |
| `study` | object | required | `objective` (a KPI name), `max_simulations`, `replications` (`{default, max}`), `verify_replications`, `verify_from` |
| `week` | object | needed by the model | `shifts`, `shift_hours`, `warmup_hours` |
| `backend` | string | `des` | The simulator. Only `des` exists. Any other name is refused at `start`. |
| `company` | string | needed by `describe_model` | The plant's name |

The plant also carries `headcount_per_shift`, `capex` (`budget_usd`, `buffer_slot_usd`), `maintenance.techs`,
`labour_usd_per_h` and `holding_usd_per_unit_h`. `line2-oee` sets 30 simulations, 5 replications by default and
at most 10, and verifies on 20 replications from number 1000.

### Tools

| Tool | Effect | What it does |
|---|---|---|
| `describe_model` | read | Lines and stations, every decision (today's value, bounds, unit), the hard limits, the limits judged after a simulation, the KPIs, the budget |
| `run_simulation(config?, replications?)` | write | Simulates `config` (decision name to value, where anything left out keeps today's value) on replications 0 to n-1 (n from `study.replications`). Answers the KPIs, station states, the after-simulation limits and `objective` (`{name, label, value, ci95, unit, better}`) |
| `get_history` | read | Every simulation: changes, objective, feasibility, key KPIs. Also the best feasible one and the budget left |
| `submit_recommendation(simulation, summary)` | write | Recommends one simulated config. Final. The part re-checks the hard limits and simulates it again on `verify_replications` replications from `verify_from`, which no `run_simulation` uses |

### inspect, diff and events

- Keys: `simulation/<n>` (each simulation's config and KPIs), `recommendation/final`, `study/budget` and
  `_audit/denials`.
- Each simulation records a `simulation` world event, for example "Simulation 7: Line 2 OEE 81.3% ± 0.6
  (5 replications)". The recommendation records a `recommendation` event. A denial records a `denial` event.

### The model (`line_sim/des.py`)

A compact discrete-event simulation of serial flow lines that share one maintenance crew, written for this part
(no SimPy). Per line:

- stations with finite buffers between them, blocking after service.
- processing times that depend on the crew at the station, the line speed and the variant, with lognormal
  variation.
- machines that fail on busy time (MTBF, shorter at higher speed on speed-sensitive stations) and wait for a free
  technician to repair them (MTTR).
- a changeover whenever the variant changes, whose length depends on the crew.
- campaigns of each variant, sized by the batch size and the mix.
- planned maintenance (PM) at the start of each shift, which stops the line until a technician has done it and
  lengthens MTBF.
- test scrap that rises above 100% speed.

A simulated week is `week.shifts` shifts after a warm-up (10 eight-hour shifts after 4 hours in `line2-oee`).

**Common random numbers.** Every draw comes from its own stream, seeded by `(seed, replication, line, station,
purpose)` (Python's `random.Random` with a string seed, hashed with SHA-512, stable across processes). So
replication r of two configs meets the same failures, repair times and variation wherever they agree, and the
difference between two configs is mostly the configs', not noise.

### KPIs (`line_sim/kpis.py`)

Per replication, then the mean and 95% confidence half-width (Student's t) over the replications: `line2.oee`,
`line2.quality`, `line2.good_per_week`, `line2.wip`, `line2.lead_time_h`, `line1.oee`, `line1.good_per_week` and
`cost_per_week_usd`. Also each station's share of time working, in changeover, starved, blocked, down and in PM,
and its failures per week.

- **OEE** is good units times the line's ideal cycle time, over all staffed time. PM, changeovers, failures,
  waiting for a technician, blocking and starving all count as losses.
- **Lead time** is the mean flow time plus half the campaign cycle: an order waits on average half a cycle for
  its variant's next campaign.

### Hard limits and denials

Checked before anything is simulated, from the plant's `decisions` and `safety`: the bounds of each decision,
headcount per shift (every operator on both lines plus the technicians), the capex budget (each buffer slot above
today's), and the safety minimums.

- A config that breaks any of them is refused with a denial that names every rule it broke:
  `{"denied": true, "code": "CONFIG_REJECTED", "violations": [{rule, param, value, limit, message}]}`. It is
  recorded in `_audit/denials`, reported as a `denial` event, and costs no simulation.
- A simulation past the budget is denied with `SIMULATION_BUDGET_EXHAUSTED`.
- A malformed call (an unknown decision, a wrong type, replications out of range) is an ordinary refusal, not a
  denial.
- Limits that only a simulation can judge (Line 1's weekly commitment, Line 2's lead-time promise) are reported
  with each result (`constraints`, `feasible`), never denied.

### Backends (`line_sim/backend.py`)

The part reaches a simulator only through `SimulationBackend`: `replicate(plant, seed, reps)` answers one raw
result per replication number, and `fingerprint()` says what decides the results (for `des`, a hash of
`des.py`). Validation, the budget, KPIs, intervals and the record belong to the part, whatever the backend.

A commercial simulator on a licensed Windows machine is meant to plug in as a worker over HTTP, behind a backend
named for it. That adapter is not written. This is the contract it and the worker would have to meet:

- `POST /simulate` with `{"model": "<model id and version>", "plant": {...}, "seed": 2026, "replications": [0, 1, 2]}`.
  `plant` is the plant with the config applied, in this part's shape (`lines`, `stations`, `buffers`, `crew`,
  `batch_size`, `speed_pct`, `pm_hours`, `maintenance.techs`, `week`). The worker maps it onto its model's
  controls.
- It answers `{"replications": [<raw result>, ...]}` in the order asked, each in the shape `replicate` documents:
  `measured_s`, and per line `good`, `scrap`, `wip_mean`, `flow_mean_s`, `campaign_cycle_s`, `pm_s`, and per
  station `failures` and the `share` of time in `working`, `changeover`, `starved`, `blocked`, `down` and `pm`.
- Replication r with the same seed gives the same result, as common random numbers need.
- `GET /model` answers the model id, its version and a hash of the model file, for `fingerprint()`.
- A config the model can't take is a 422 with a reason. The part reports it as an ordinary refusal.
- One replication of a week should answer in seconds. The adapter would wait at most 60 s per call.

## How it's tested

```bash
cd integrations/line-sim
uv run --with pytest pytest -q
```

The tests need no services. They read the plant from `benchmarks/line2-oee/data/plant.json`. They cover the model
(a replication is the same in another process, common random numbers, changeovers, failures, PM time) and the
part: KPIs with confidence intervals, denials that name every broken limit and cost nothing, the budget, malformed
calls, the history, a verified and final recommendation, exact save and restore, and `describe` over the
protocol. No test is marked `live`.

## Limits

- Only the built-in `des` model exists. The commercial-simulator backend is a written contract, not code.
- The model is a compact serial-line model written for this part, not a validated model of a real plant. Its
  results are as good as the plant data in the config.
- The KPI names are fixed (`line_sim/kpis.py`): only lines with ids `line1` and `line2` get line KPIs.
- `start` checks only `seed`, `lines`, `decisions`, `safety` and `study`. A plant without `week`, `company`,
  `capex` or the other plant keys fails later, on the first tool call that needs them.
- No world clock: a simulation takes no world time.
