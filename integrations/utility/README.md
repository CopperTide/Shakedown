# Electric utility (`utility`)

Puts a simulated electric utility in a Shakedown world: the site's main meter, its two-rate tariff, and the utility's
demand-response events, in which the site is asked to hold its load at or below a target during a window of peak
prices. The meter does not measure anything itself: a benchmark feeds it the plant's load with a link to
`meter_load`. It is a simulation written by CopperTide, not a model of any real utility's program. The
[`factory`](../../benchmarks/factory/README.md) benchmark uses it, as the part `utility`, for the demand-response
front of its shift task and for its `meter-low` fault.

| | |
|---|---|
| Program | `python -m utility_part` (Python, run with `uv`) |
| Stands in for | a utility's demand-response program and a revenue meter (`describe.system.stands_in_for`) |
| Restore | exact, from a save (one `state.json`). No approximate restore. |
| World clock | used: the meter integrates the load over the clock, and windows open and close on it |
| Agent tools | `utility_meter`, `demand_response_events`, `respond_to_event`. The world tools `meter_load` (for a link) and `utility_scenario` (for a task's setup) are not for agents. |

## What it needs

- `uv` and Python 3.10 or later. The package depends only on the Python kit (`kits/python`, package
  `shakedown_part`), which `uv` installs from the repository.
- No Docker, no services, no GPU.

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | `saves`, relative to the working folder | Where saves are kept, as `<SHAKEDOWN_SAVES>/utility/<save>/state.json`. `shaked` always passes its own. |

## Use it

A benchmark or world names it as a part. The factory benchmark uses
`{part: "utility", integration: "utility", config: ...}`, with the config from the `utility` entry of
`benchmarks/factory/data/shift.json` plus `plant_start`. It links the plant's `power` events to `meter_load`
(`kw` from the event's `detail.kw`).

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `meter` | string | `Site main meter` | the meter's name, shown in its document |
| `site` | string | `Plant` | the site's name, shown in the meter's document |
| `plant_start` | string | `2026-03-10T06:00:00` | the plant's date and time at clock 0, for the times the tools show |
| `initial_kw` | number | `1500` | the load until the first `meter_load` |
| `tariff` | object | `{standard_per_kwh: 0.11, peak_per_kwh: 0.42}` | USD per kWh. The peak price applies while a window is active. |
| `scenarios` | object | `{}` | named scenarios, each with `disruptions` |

A negative `initial_kw`, a tariff without both prices or a `plant_start` that is not an ISO date and time answers
`-32602`. Other keys are ignored (the factory's data also gives `utility`, the utility's name, which the part does
not use).

| Tool | Effect | What it does |
|---|---|---|
| `utility_meter {}` | read | the load now, the peak so far, the energy and its cost since the start, the tariff period and price |
| `demand_response_events {}` | read | each event: target, window in plant time, status (`notified`, `active`, `ended`), the site's response, and inside the window so far the minutes, the average and highest kW and the minutes over the target |
| `respond_to_event {event, committed_kw, plan}` | write | the site's answer to an event. Refused for an unknown event, one that has ended, or a negative `committed_kw`. |
| `meter_load {kw, cause?}` | write | the world tool a link calls: the plant's load from now on. Refused for a negative `kw`. |
| `utility_scenario {name?, disruptions?, remove?}` | write | the world tool: sets the scenario and arms disruptions. Without `name` it is a fork's change: it drops the ids in `remove` and arms `disruptions`. |

How it behaves:

- **Keys.** `scenario/current`, `meter/site`, `dr/<event>` and `_disruption/<id>` (internal).
- **The clock.** `advance`, and every `call_tool` first, integrates the load up to the clock. The load is kept in
  whole watts and the energy in whole watt-milliseconds, cut at every window's start and end, so the state after an
  advance does not depend on how the advances were split ([`docs/protocol.md`](../../docs/protocol.md), "Clock
  contract").
- **Events.** A window's notice is the world event `demand_response_notified`, given with the disruption that creates
  it. Its start and end are `demand_response_active` and `demand_response_ended`, given by `advance`. One found
  during a tool call waits for the next `advance`. The factory benchmark links all three to the plant's status boards.
- **Scheduled disruptions.** They follow the kit's convention ([`docs/protocol.md`](../../docs/protocol.md),
  "Scheduled disruptions"). `meter_load` and `utility_scenario` count for none.
  - `demand_response {event, target_kw, lead_min, minutes, program?}`: the utility notifies an event. The window
    opens `lead_min` minutes of plant time after the notice and lasts `minutes`. Arming refuses a negative
    `target_kw` or `lead_min`, and `minutes` below 1.
  - `meter_low {percent}` (1 to 99): from the call it fires on, `utility_meter` reads `percent` low (`kw`, `peak_kw`,
    `kwh_this_shift`, `energy_cost_usd`). The `meter/site` document and the demand-response measures keep the true
    load, so a benchmark scores what happened.
- **Saves and restores.** The kit's `shakedown_part.jsonstate` with its clock (`ClockedJsonStatePart`): each tool runs
  on a copy of the state, kept only when the tool did not refuse. A save writes the config and the state (the meter,
  the events, the queued window events, the disruption schedule) to one `state.json`. A restore reads the state back
  and takes the config from the start call. `usage` reports 0, since every save lives under `SHAKEDOWN_SAVES`.

## How it's tested

```bash
cd integrations/utility
uv run --with pytest pytest -q
```

The tests run the part in process. They cover the meter on the clock, split advances giving the same state, a window
opening, closing and measured, a window event found in a call waiting for the next `advance`, a notice after its
count, responses and refusals, save and restore, `meter_low` (from its count on, kept by a save, its percent checked),
bad disruptions, and a save written by an older version of the part (`tests/fixtures/old-save`) that still restores.
`test_conformance` runs `shakedown-part-check` on the package. It is skipped when the checker is not built in
`SHAKEDOWN_BIN_DIR` (default `core/target/debug`) or `uv` is missing. Build it with
`(cd core && cargo build -p parts --bin shakedown-part-check)`. No test is marked `live`.

`core/crates/shaked/tests/meter_fault_fork.rs` runs this part under `shaked`, beside a toy part, with the factory's
`meter-low` option: a fork that removes the fault reads the true load.

On 2026-11-07, on Linux, the tests then in the package passed, the conformance check included (its replay step
skipped, since the part restores from saves). On 2026-11-21 the `meter_low` tests were added and passed, with the
conformance check skipped because no checker was built.

## Limits

- A simulation: one meter, a load that stays constant between readings, and a two-rate tariff. No demand charges,
  power factor, time-of-use bands or billing.
- The load is only what a link sends. Without a link to `meter_load`, the meter stays at `initial_kw`.
- No approximate restore.
