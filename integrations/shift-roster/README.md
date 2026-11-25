# Shift roster (`shift-roster`)

Puts a simulated shift roster in a Shakedown world: the stations of a shift (press lines, robot cells), the one
certification each needs, and the operators with their certifications, on shift at a station, floating, or on call.
It is a simulation written by CopperTide. The [`factory`](../../benchmarks/factory/README.md) benchmark uses it, as
the part `roster`, for the operator call-out front of its shift task, and the world builder's
[`small-manufacturer`](../../benchmarks/world/library/small-manufacturer/README.md) template uses it as its optional
roster.

| | |
|---|---|
| Program | `python -m shift_roster_part` (Python, run with `uv`) |
| Stands in for | time-and-attendance software such as UKG Workforce Central or SAP SuccessFactors (`describe.system.stands_in_for`) |
| Restore | exact, from a save (one `state.json`). No approximate restore. |
| World clock | not used |
| Agent tools | `shift_roster`, `assign_operator`, `call_in_operator`. The world tool `roster_scenario` is for a task's setup. |

Why a simulated part and not ERPNext: ERPNext's core has the Employee doctype but no skills, certifications or shift
assignments (those are in the separate HRMS app), and a call-out is a scheduled disruption, which needs a part that
keeps a schedule in its state ([`docs/protocol.md`](../../docs/protocol.md), "Scheduled disruptions").

## What it needs

- `uv` and Python 3.10 or later. The package depends only on the Python kit (`kits/python`, package
  `shakedown_part`), which `uv` installs from the repository.
- No Docker, no services, no GPU.

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | `saves`, relative to the working folder | Where saves are kept, as `<SHAKEDOWN_SAVES>/shift-roster/<save>/state.json`. `shaked` always passes its own. |

## Use it

A benchmark or world names it as a part. The factory benchmark uses
`{part: "roster", integration: "shift-roster", config: ...}`, with the config from the `roster` entry of
`benchmarks/factory/data/shift.json`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `stations` | array | required | `[{id, name, kind, needs}]`, `needs` the certification an operator must have |
| `operators` | array | required | `[{id, name, certifications, station, on_shift?, on_call?}]`. `on_shift` defaults to true, `station` may be null (floating). |
| `shift` | object | none | shown as it is in `shift_roster` |
| `overtime` | object | `{rate_per_hour: 0, hours: 0}` | what calling one operator in costs |
| `scenarios` | object | `{}` | named scenarios, each with `disruptions` |

A start with no stations, operators that are not a list, ids that are not unique across stations and operators, or
an operator at a station that does not exist answers `-32602`.

| Tool | Effect | What it does |
|---|---|---|
| `shift_roster {}` | read | each station with its operator and whether it is staffed (and why not), each operator, and the overtime so far |
| `assign_operator {operator, station}` | write | puts an on-shift operator (id or full name) at a station. Whoever was there floats. An operator not certified for the station is placed with a `warning`, and the station is not staffed. |
| `call_in_operator {operator}` | write | an on-call operator joins the shift, on overtime |
| `roster_scenario {name?, disruptions?, remove?}` | write | the world tool: sets the scenario and arms disruptions. Without `name` it is a fork's change: it drops the ids in `remove` and arms `disruptions`. |

`assign_operator` refuses an unknown operator or station and an operator who is not on shift. `call_in_operator`
refuses an operator who is on shift already or not on call. A refused call changes nothing.

How it behaves:

- **Keys.** `scenario/current`, `station/<id>`, `operator/<id>`, `overtime/shift`, `log/actions` and
  `_disruption/<id>` (internal). A station is `staffed` when its operator is on shift and certified for it. Its
  `why_not` says otherwise. `log/actions` numbers every assignment and call-in.
- **Gates.** The part has none of its own. The factory benchmark gates the plant's `start_cell` on `station/<cell>`
  being staffed (`benchmarks/factory/src/world.rs`).
- **Scheduled disruption.** `callout {operators}`: the operators (ids or names) call out. They are off shift, and the
  stations they staffed are not staffed. Arming refuses an empty list or an unknown operator. It follows the kit's
  convention: it fires once, before the agent's call that crosses its count, and `roster_scenario` counts for
  nothing.
- **Saves and restores.** The kit's `shakedown_part.jsonstate`: each tool runs on a copy of the state, kept only when
  the tool did not refuse. A save writes the config and the state, with the disruption schedule, to one `state.json`.
  A restore reads the state back and takes the config from the start call. `usage` reports 0, since every save lives
  under `SHAKEDOWN_SAVES`.

## How it's tested

```bash
cd integrations/shift-roster
uv run --with pytest pytest -q
```

The tests run the part in process on the factory benchmark's config. They cover the stations at the start, a call-out
covered by a floater and a call-in, a station taken over, and save and restore. `test_conformance` runs
`shakedown-part-check` on the package with the same config. It is skipped when the checker is not built in
`SHAKEDOWN_BIN_DIR` (default `core/target/debug`) or `uv` is missing. Build it with
`(cd core && cargo build -p parts --bin shakedown-part-check)`. No test is marked `live`.

On 2026-11-07, on Linux, all 5 tests passed, the conformance check included.

## Limits

- A simulation of one shift: no schedules over days, breaks, hours worked, labour rules or pay beyond a flat
  overtime cost per call-in.
- One certification per station, and no expiry dates on certifications.
- No world clock: a call-out happens on a count of calls, not at a time.
- No approximate restore.
