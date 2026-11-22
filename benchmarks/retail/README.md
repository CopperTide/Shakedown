# retail

The τ²-bench retail tools over a Postgres world part (`retail`, integration `postgres`): users, orders and
products as rows, the tools ported from `cs2680-gap-tests/harness/tool.py` (`src/tools/`), the data and policy in
`data/retail`. A run passes when its final database matches the task's gold database.

## Scenario options

`describe.scenario_options` (`src/options.rs`) offers four faults any task's run may plant, faults X1 to X4 of the
cs2680 evaluation. None is offered selected, and the agents are not told. Each is a scheduled disruption
(`docs/protocol.md`, "Scheduled disruptions") that the world's own tool `retail_scenario` arms in the retail database
(table `_disruption`, whose rows are internal keys: saved with each step they change, left out when the final world is
compared with the gold one). Each fires on the agents' call after the first `after` (default 0: the first) of its
tools, and acts on that call's answer only:

| Option | Counts calls of | Disruption | What the call answers |
|---|---|---|---|
| `stale-order-status` | `get_order_details` | `stale_status` | the order's status as delivered for pending and pending for delivered |
| `order-read-timeout` | `get_order_details` | `timeout` | the call isn't made: "The request timed out; it may not have been processed." |
| `variant-out-of-stock` | `get_product_details` | `variant_unavailable` | the product's first variant in stock (by id) as out of stock |
| `lost-write` | every write tool | `lost_write` | the write's usual answer, but nothing is written and the step names no written key |

When one fires, the call records a `disruption` world event (`data: {id, kind}`).

## Tests

`cargo test -p shakedown-retail` (needs the Postgres of `deploy/compose.yml`, or `SHAKEDOWN_PG_URL`): the gold changes
of all tasks, reads and writes, the schedule (arming, refusals, counting, firing once), and describe over stdio.
`core/crates/shaked/tests/retail_disruptions.rs` runs each fault through shaked: it fires at the scheduled call and
only then, and after `lost-write` the step's save and the read after it keep the old row. Verified on Linux,
2026-11-21.
