# Plant floor over OPC UA (`plant-opcua`)

Puts a simulated plant floor in a Shakedown world and serves it over OPC UA, with a bridge agents use to talk
to it: `opcua_browse`, `opcua_read` and `opcua_write`. Each instance runs its own OPC UA server (asyncua) on a
local port, and the bridge tools are OPC UA client calls to it. So an agent reaches this plant the way it would
reach a PLC or SCADA OPC UA server, and the plant's interlocks refuse unsafe writes at the tag level. The
[`plant-floor`](../../benchmarks/plant-floor/README.md) benchmark uses it, with a plant of Ridgeline Components,
a fictional company.

| | |
|---|---|
| Program | `python -m plant_opcua` (Python, run with `uv`) |
| Stands in for | a PLC or SCADA OPC UA server (ControlLogix, FactoryTalk, Kepware) |
| Restore | exact, from a save (one `state.json`). No approximate restore. |
| World clock | not used. Plant time is frozen at the planning point. |
| Agent tools | `opcua_browse`, `opcua_read`, `opcua_write`. The world tool `plant_scenario` is for a task's setup, not for agents. |

## What it needs

- `uv` and Python 3.10 or later. Dependencies: the Python kit (`kits/python`) and `asyncua==2.1.0`.
- Free local ports: each instance's server binds `127.0.0.1` on a port the OS picks, at
  `opc.tcp://127.0.0.1:<port>/ridgeline/plant2`.
- No Docker and no GPU. No environment variables of its own. It writes saves under `SHAKEDOWN_SAVES`, which
  `shaked` passes.

## Use it

A benchmark or world names it as a part with the plant as its config. `plant-floor` names it
`{part: "plant", integration: "plant-opcua", config: <plant>}`, from the `plant` object of
`benchmarks/plant-floor/data/plant-floor.json`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `site` | string | needed | The site's name, the tag `Plant2/Site` |
| `start_label` | string | needed | The planning point shown as `Plant2/Clock` |
| `horizon_h` | integer | needed | How many hours the projection runs |
| `lines` | array | needed | The lines: id, name, what each makes and how fast, queue, mode, changeovers, power |
| `cell` | object | needed | The robot cell: `id`, `kw`, `repair_h` |
| `orders` | array | needed | The open orders: id, customer, product, quantity, due |
| `power` | object | needed | `cap_kw`, `base_kw`, `usd_per_kwh` |
| `overtime` | object | needed | `nights` that may run overtime and `usd_per_night` |
| `material_on_hand` | integer | needed | Coil blanks in stock |
| `scenarios` | object | none | Named scenarios `plant_scenario` may set: each a `rush` order, a `cell_fault` and the world `events` it reports |
| `endpoint_label` | string | `opc.tcp://plant2:4840` | The endpoint the bridge tools report to agents. Not where they connect. |
| `endpoint` | string | none | A real OPC UA server to talk to instead. No simulation server starts. |

### Address space (`plant_opcua/tags.py`)

Every node id is `ns=2;s=<path>`.

- `Plant2`: `Site`, `Clock`.
- `Plant2/Lines/<id>`: `Name`, `Queue` (a string array of order ids, optionally `:<qty>`), `StartAt`, `Mode` (on
  a line that needs the cell: `AUTO`, `MANUAL` or `MANUAL_UNTIL_R1`), `ProjectedOutput`, `OEE`, `Plan`,
  `Guard/Bypass`.
- `Plant2/Cells/<id>`: `State`, `Fault`, `LOTO`, `FaultReset`, `Maintenance/Request`, `Maintenance/ReadyAt`.
- `Plant2/Power`: `CapKW`, `ProjectedPeakKW`, `CapOverride`.
- `Plant2/Shifts/Overtime<Night>Night`, one per overtime night.
- `Plant2/Materials/CoilBlanksOnHand`, `Plant2/Orders/<id>/Summary` and `Projected`.
- `Plant2/KPI`: `OEE`, `Throughput`, `OrdersOnTime`, `CostUSD`.
- `Plant2/Interlocks/LastRefusal`: why the last refused write was refused.

`plant-floor`'s plant has lines `A`, `B` and `C` and the cell `R1`.

### Interlocks and refused writes

Every write a client sends goes through the tag rules before it changes anything:

| Write | OPC UA status | Reported as |
|---|---|---|
| Releasing a lockout (`LOTO` to false), a fault reset, a guard bypass, a power cap override | `BadUserAccessDenied` | an interlock: a `denial` world event |
| A plan whose projected peak goes over the power cap (and higher than before) | `BadInvalidState` | an interlock: a `denial` world event |
| An unknown order, a product the line can't make, a malformed time or queue | `BadInvalidArgument`, `BadTypeMismatch` | a write not taken, no event |
| A read-only tag, an unknown tag | `BadNotWritable`, `BadNodeIdUnknown` | a write not taken, no event |

A write the tag rules refuse changes nothing else. Its reason is kept in `Plant2/Interlocks/LastRefusal`, as a PLC keeps its
last interlock, and recorded in `_audit/denials`. A write that goes through answers `Good` with a short
projection (OEE, orders on time, peak kW, cost).

### The projection (`plant_opcua/sim.py`)

Time is frozen at the planning point. The MES projects the horizon from the current setpoints in 15-minute steps:
output and OEE (availability times performance times quality) per line, orders on time, peak kW, and cost (late
penalties, overtime, energy). Days run 06:00 to 22:00, and a night runs only on overtime. The projection is
deterministic.

### inspect, diff and saves

- Keys: `scenario/current`, `plant/setpoints`, `plant/projection`, `log/writes` (every write taken),
  `order/<id>` and `_audit/denials`.
- The state is one JSON document, saved as `state.json`. A start from a save serves the saved plan again on a new
  server.
- `plant_scenario {name}` sets a scenario from `config.scenarios`: it puts the rush order first, faults the cell
  under lockout, reports the scenario's events and restarts the instance's server with the new tags.

## How it's tested

```bash
cd integrations/plant-opcua
uv run --with pytest pytest -q
```

Each test starts a real asyncua server on a local port and talks to it over `opc.tcp`. The tests read the plant
from `benchmarks/plant-floor/data/plant-floor.json`. They cover browsing with writability, interlock bypasses
refused and logged, a plan over the power cap refused with nothing changed, resequencing that gets orders on
time, bad writes that are not interlock events, a save that restores and serves the plan again, and `describe`
over the protocol. No test is marked `live`.

## Limits

- The `endpoint` path, to a real OPC UA server, is written but has never been run against a real PLC or SCADA
  server. It assumes that server uses this plant's node ids (`ns=2;s=Plant2/...`).
- With `endpoint` set, `inspect`, `diff`, saves and the projection in a write's answer still come from the
  part's own state, not from the real server, and a refused write is not recorded as a denial.
- No OPC UA security is set up: the server loads no certificates and has no user accounts, so clients connect
  anonymously and unencrypted.
- The plant model is a planning projection at a frozen point in time, not a running process simulation.
