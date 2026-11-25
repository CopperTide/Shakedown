# Ridgeline outside companies (`ridgeline-hub`)

Puts the outside companies of Ridgeline, a fictional manufacturer, in a Shakedown world: suppliers that quote
parts, carriers that quote and book freight, an insurer's claim intake, and customers' agents that answer
delivery notices. It wraps the `Market` of the Ridgeline factory example's hub and changes only where time comes
from, so replies arrive at the same world-clock times in every run and replay. It also adds raw-material trucks on
their way to the plant, with a delay a task can schedule. The [`factory`](../../benchmarks/factory/README.md)
benchmark uses it as its part `hub`.

| | |
|---|---|
| Program | `python -m ridgeline_hub_part` (Python, run with `uv`) |
| Restore | exact, from a save (one `state.json`). No approximate restore. |
| World clock | used: the market ticks every 500 ms of world time, and trucks arrive on the plant clock |
| Agent tools | `request_quotes`, `get_quotes`, `award_order`, `freight_quotes`, `get_freight_quotes`, `book_freight`, `submit_claim`, `answer_claim`, `notify_customer`, `customer_replies`, `inbound_deliveries`. The world tool `hub_scenario` is for a task's setup, not for agents. |

## What it needs

- `uv` and Python 3.10 or later. The part depends only on the Python kit (`kits/python`).
- **The Ridgeline factory example, which is not public.** The part imports `ridgeline_hub.market` from it and
  reads its supplier data. By default it looks in `CopperTide-Examples/factory/hub`, in the folder that holds
  the repository folder (`../CopperTide-Examples` from the repository root). That repository is private. Without
  it the part can't start, every test is skipped, and the `factory` benchmark's world can't start. Outside
  contributors can read and change this code, but can't run it.
- No Docker, no services and no GPU. No environment variables of its own. It writes saves under
  `SHAKEDOWN_SAVES`, which `shaked` passes.

## Use it

A benchmark or world names it as a part. The `factory` benchmark names it
`{part: "hub", integration: "ridgeline-hub", config: {inbound: [...]}}`, with the trucks from
`benchmarks/factory/data/shift.json`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `hub_path` | string | `../CopperTide-Examples/factory/hub` from the repository root | The hub folder that holds `ridgeline_hub/market.py` |
| `data_path` | string | `<hub_path>/../data/ridgeline.json` | The data file whose `suppliers` list the market uses |
| `plant_start` | string | `2026-03-10T06:00:00` | The plant clock at world time 0 |
| `inbound` | array | none | Raw-material trucks on their way: `[{id, material, supplier, carrier, eta_hours, line?, line_stock_hours?, ...}]` |

**Loading the example.** Only `ridgeline_hub.market` is imported, with `hub_path` first on `sys.path`. The
hub's app is not imported, because it pulls in FastAPI and httpx. The folder is read only: no `__pycache__` is
written there. `hub_path` is process-global (`sys.path`, `sys.modules`), so one part process serves one hub.

**Fingerprint.** `describe` declares the sha256 of `market.py` and `ridgeline.json` at the default paths.
`start` refuses configured files that are missing (`-32602`, naming the path) or differ from those (`-32003`),
before it imports anything. So `hub_path` and `data_path` may point at another copy only if it is byte for
byte the same.

### Tools

The market tools have the names and arguments of the factory example's `ridgeline_mcp/market.py`. They answer at
once. `wait_s` on `get_quotes`, `get_freight_quotes` and `customer_replies` is accepted and ignored: blocking
inside a call would stall the world, so agents poll while the world clock runs.

| Tool | Effect | What it does |
|---|---|---|
| `request_quotes {part, qty?, need_by?}` | write | Asks the suppliers that stock a part for quotes. Quotes arrive over about a minute of world time. |
| `get_quotes {rfq}` | read | The quotes received and who has not answered |
| `award_order {rfq, supplier, po}` | write | Tells the winning supplier its quote is accepted |
| `freight_quotes {origin, weight_kg, need_by?}` | write | Asks carriers to quote a load to the plant's receiving dock |
| `get_freight_quotes {request}` | read | The carriers' quotes |
| `book_freight {proposal_id, quote_id, carrier, amount, part?}` | write | Books a quote. The carrier and amount must match it. Makes a shipment. |
| `submit_claim {policy, machine, amount, downtime_hours, evidence}` | write | Sends an equipment-breakdown claim. Answers the insurer's follow-up questions. |
| `answer_claim {claim, answers}` | write | Answers those questions, in order |
| `notify_customer {proposal_id, customer, order, new_date, message}` | write | Sends a customer's agent a new delivery date |
| `customer_replies {}` | read | The customers' replies so far |
| `inbound_deliveries {}` | read | Each truck: material, supplier, carrier, line, `eta`, `status` (`in transit` or `arrived`), `line_runs_out` and `line_short_hours` |
| `hub_scenario {name?, disruptions?, remove?}` | write | World tool: sets the scenario (only `base`) and arms delivery delays |

- `proposal_id` on `book_freight` and `notify_customer` is stored on the record and not checked here. A benchmark
  can gate these tools on an approvals part ([`docs/protocol.md`](../../docs/protocol.md), "Gates on tools").
- Unknown ids, an unknown supplier or quote, a carrier or amount that doesn't match the quote (numbers within
  0.005, text compared without case) and bad arguments come back as a tool-level `error` and change nothing. An
  unknown tool name is `-32602`.

### Keys and events

- `inspect` and `diff` keys: `rfq/<id>`, `freight/<id>`, `claim/<id>`, `notice/<id>`, `shipment/<id>`,
  `inbound/<id>`, `scenario/inbound`, and the internal `_disruption/<id>`.
- `call_tool` first advances the market to its `clock_ms`, then runs the tool. Events from the ticks (a quote
  arrived, a customer replied) stay queued, and the next `advance` returns them.
- A booking creates `shipment/SHP-001`, `SHP-002` and so on, with its arrival on the plant clock, and the next
  `advance` gives a `shipment_booked` event whose `data` is the shipment. A benchmark's link can hand it to the
  plant ([`docs/protocol.md`](../../docs/protocol.md), "Links between parts"). The part itself knows nothing of
  the plant.

### Trucks and their delay (`ridgeline_hub_part/inbound.py`)

The example's `Market` has no raw-material trucks. This part adds them beside it, for the `factory` benchmark's
shift task. A truck's `eta` is `plant_start` plus `eta_hours` plus any delay, and its `status` follows the world
clock. With `line_stock_hours`, `line_runs_out` is `plant_start` plus that, and `line_short_hours` is how long
the line would wait.

The scheduled disruption `delivery_delay {delivery, extra_hours}` makes a truck arrive `extra_hours` later
([`docs/protocol.md`](../../docs/protocol.md), "Scheduled disruptions"). `hub_scenario` arms it, all or nothing.
It fires before the agent's call (of any hub tool, or of `when.tool`) that crosses its count, and stays when that
call is refused. `hub_scenario` itself counts for nothing. Without `name`, `hub_scenario` is a fork's change: it
drops the delays `remove` names and arms the ones given.

### Saves

One `state.json` in the save folder holds the market's requests, quotes, claims and notices, the shipments,
the queued events, the trucks, the schedule and the clock position. The market has no randomness, so `seed` is
accepted and unused, and the replay contract holds for any seed (the fingerprint says so). `usage` is 0, because
every save lives under `SHAKEDOWN_SAVES`. `gc` removes the save folders not kept.

## How it's tested

```bash
cd integrations/ridgeline-hub
uv run --with pytest pytest -q
```

Every test imports the real `market.py` from the factory example, so without it every test is skipped, with the
reason. The trucks' tests read `benchmarks/factory/data/shift.json`. The tests cover quotes arriving on the world
clock, bookings and their `shipment_booked` events, refusals that change nothing, save and restore with an equal
future, replay equal to a save, a call at an earlier clock, the fingerprint (a changed `market.py` is refused and
nothing is imported), `gc`, delivery delays that fire on the counted call and survive a save, and a
`shakedown-part-check` conformance run. That last test is skipped unless the checker is built
(`(cd core && cargo build -p parts --bin shakedown-part-check)`, found in `SHAKEDOWN_BIN_DIR` or
`core/target/debug` or `release`) and `uv` is on `PATH`. No test is marked `live`.

All 23 tests, the conformance run included, passed on Linux on 2026-11-07, next to a private checkout of the
example. `core/crates/shaked/tests/factory.rs` has a live test that runs the whole `factory` world, this part
included, under `shaked`. No result of it is recorded here.

## Limits

- It can't run without the private Ridgeline factory example, and only with files identical to the default
  copy.
- The suppliers, carriers, insurer and customers are scripted by the example's `market.py`. Ridgeline, its
  suppliers and its customers are fictional.
- `proposal_id` is not verified by this part.
- Nothing is linked from the trucks to the plant: the plant doesn't model material, so a late truck doesn't stop
  a line by itself. `line_runs_out` says when it would.
