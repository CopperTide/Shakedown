# Cold-chain logistics with a supply chain director (`pharma-logistics`)

Puts simulated cold-chain carriers and a customer desk in a Shakedown world: a register of carrier lanes with their
2-8 C qualification, quotes, bookings limited by each lane's weekly capacity, notices to customers and requests from
them. It also holds a simulated supply chain director, who decides proposed changes by fixed rules. It is a
simulation written by CopperTide for the [`pharma`](../../benchmarks/pharma/README.md) benchmark, where it is the part
`logistics` and its carriers and customers are fictional (`benchmarks/pharma/data/larchmont.json`).

| | |
|---|---|
| Program | `python -m pharma_logistics_part` (Python, run with `uv`) |
| Restore | exact, from a save (one `state.json`). No approximate restore. |
| World clock | not used |
| Agent tools | `qualified_lanes`, `cold_chain_quotes`, `book_shipment`, `shipments`, `send_customer_notice`, `customer_notices`, `customer_requests`. The world tools `logistics_scenario` (for a task's setup) and `supply_review` (for a link) are not for agents. |

`describe.system` names no product it stands in for: `{kind: "logistics", product: "Simulated cold-chain carriers
and customer desk", simulated: true}`.

## What it needs

- `uv` and Python 3.10 or later. The package depends only on the Python kit (`kits/python`, package
  `shakedown_part`), which `uv` installs from the repository.
- No Docker, no services, no GPU.

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | `saves`, relative to the working folder | Where saves are kept, as `<SHAKEDOWN_SAVES>/pharma-logistics/<save>/state.json`. `shaked` always passes its own. |

## Use it

A benchmark or world names it as a part. The pharma benchmark uses
`{part: "logistics", integration: "pharma-logistics", config: ...}` and builds the config from its data file
(`benchmarks/pharma/src/world.rs`).

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `logistics` | object | required | `origins` (`{key: name}`), `lanes` (`[{carrier, origin, destination, service, temperature, qualified, transit_days, price, capacity}]`), optional `restricted_batches` and `scenarios` |
| `customers` | array | required | `[{name, region}]`. A customer's region is the destination of its lanes. |
| `today` | string | required | the world's date, `YYYY-MM-DD`, the ship date of every quote |
| `defer` | array | `[]` | the change actions another desk decides. The pharma benchmark gives the QA head's: `batch_release`, `batch_hold`, `deviation`. |

A start without `logistics` or `customers`, a `logistics` without `origins` or `lanes`, or a `today` that is not a
date answers `-32602`.

| Tool | Effect | What it does |
|---|---|---|
| `qualified_lanes {}` | read | the origins and the register: each lane with its shipments left this week, and whether it is available or delayed |
| `cold_chain_quotes {origin, customer, qty}` | write | a quote (`Q-0001`, ...) from every available lane from `origin` to the customer's region, shipping today. `origin` is a key (`US`, `EU`), an origin's name, or the start of one. |
| `book_shipment {proposal_id, quote_id, carrier, order, batch, qty}` | write | books a quote once (`SHP-0001`, ...), with its arrival date |
| `shipments {}` | read | the bookings |
| `send_customer_notice {customer, order, new_date, message}` | write | a notice (`N-0001`, ...) |
| `customer_notices {}` | read | the notices |
| `customer_requests {}` | read | requests customers sent to the desk, such as an order escalated as urgent |
| `logistics_scenario {name?, disruptions?, remove?}` | write | the world tool: sets the scenario and arms disruptions. Without `name` it is a fork's change: it drops the ids in `remove` and arms `disruptions`. |
| `supply_review {id, action, payload?, summary?}` | write | the world tool a link calls: the director reviews a proposed change |

Refusals, each of which changes nothing:

- `cold_chain_quotes`: an unknown origin or customer, a `qty` of 0 or less, or no available lane.
- `book_shipment`: an unknown or already booked quote, another carrier or quantity than the quote's, an unavailable
  lane, or no capacity left. Capacity counts shipments per lane (carrier and origin), not units.
- `send_customer_notice`: an unknown customer, or a `new_date` that is not `YYYY-MM-DD`.

How it behaves:

- **Keys.** `scenario/current`, `register/lanes` (the register as it stands now), `quote/<id>`, `shipment/<id>`,
  `notice/<id>`, `request/<id>`, `review/<change id>` and `_disruption/<id>` (internal).
- **Bookings.** The part records a booking's order, batch and proposal as given. It does not know the ERP or check the
  proposal. The pharma benchmark gates `book_shipment` on an approved `cold_chain_shipment` change with the same
  carrier, quote, order, batch and quantity.
- **The director.** `supply_review` decides every action not in `defer`, records a review under `review/<change id>`
  and gives a `decided` event `{id, approved, note}`. The rules:
  - `stock_reallocation` needs `batch` and a numeric `qty`.
  - `cold_chain_shipment` needs `carrier`, `quote_id`, `order`, `batch` and a numeric `qty`, a quote of that carrier,
    a lane qualified for 2-8 C and available, and, for a restricted batch, enough quantity left in the reallocations
    it approved of that batch.
  - Any other action is rejected, with a note naming what each desk decides.

  The director does not check a lot's QA status or shelf life. The pharma benchmark's safety checks do.
- **Links.** The pharma benchmark links the approvals part's `change_proposed` event to `supply_review`, and this
  part's `decided` event back to the approvals part's `decide_change`.
- **Scenarios.** They come from `logistics.scenarios` in the config. A scenario may set a carrier's `capacity` and a
  `reject_first_reallocation` note. The pharma benchmark's data has two: `carrier-no-capacity` sets Tundra Medical
  Courier's capacity to 0, and `reallocation-rejected` rejects the first `stock_reallocation` with a note asking for
  the quantity and the orders it serves.
- **Scheduled disruptions.** They follow the kit's convention ([`docs/protocol.md`](../../docs/protocol.md),
  "Scheduled disruptions"): each fires once, before the agent's call that crosses its count, and
  `logistics_scenario` and `supply_review` count for nothing.

  | Kind | Fields | What it does |
  |---|---|---|
  | `lane_capacity` | `carrier, origin, destination, capacity` | the lane's weekly capacity becomes `capacity` (0 or more) |
  | `lane_unavailable` | `carrier, origin, destination` | no quotes on the lane, bookings refused, and the director rejects shipments on it |
  | `transit_delay` | `carrier, origin, destination, extra_days` | later quotes, and the shipments already booked on the lane, arrive `extra_days` (1 or more) later |
  | `reject_next` | `action, note?` | the director rejects the next decision on `stock_reallocation` or `cold_chain_shipment`, with `note` |
  | `customer_escalation` | `order, note?` | a customer's urgent request about `order` waits on the desk (`customer_requests`) |

  Arming refuses a lane the register does not have and values out of range.
- **Saves and restores.** The kit's `shakedown_part.jsonstate`: each tool runs on a copy of the state, kept only when
  the tool did not refuse. A save writes the config and the state (quotes, shipments, notices, requests, reviews, lane
  changes, the disruption schedule) to one `state.json`. A restore reads the state back and takes the config from
  the start call. `usage` reports 0, since every save lives under `SHAKEDOWN_SAVES`.

## How it's tested

```bash
cd integrations/pharma-logistics
uv run --with pytest pytest -q
```

The tests run the part in process on the pharma benchmark's config. They cover quotes, bookings and their refusals,
capacity, the director's rules, both scenarios, notices, save and restore, and each of the five disruption kinds
(when they fire, once, across a save, and a waiting count kept as an internal key). `test_conformance` runs
`shakedown-part-check` on the package with the same config. It is skipped when the checker is not built in
`SHAKEDOWN_BIN_DIR` (default `core/target/debug`) or `uv` is missing. Build it with
`(cd core && cargo build -p parts --bin shakedown-part-check)`. No test is marked `live`.

On 2026-11-07, on Linux, 14 tests passed, the conformance check included. The package has 15 now.

## Limits

- A simulation: no carrier APIs, rates, tracking, temperature logging or customs. Lanes are records from the config
  and capacity is a count of shipments per week, with no week rollover.
- The director's rules are the few above.
- No world clock: arrival dates are computed from `today`, and nothing arrives as time passes.
- No approximate restore.
