# Retail customer service (`retail`)

Measures whether a team of agents resolves a retail customer's request the way the store policy says: cancel,
modify, exchange or return orders, and update addresses and payment methods. The tasks, data and policy come from
the retail domain of [τ²-bench](https://github.com/sierra-research/tau2-bench) by Sierra Research. Shakedown runs
them over a Postgres world, scores the final database against each task's gold database, and adds four faults a
run may plant.

| | |
|---|---|
| Program | `shakedown-retail` (Rust, crate `shakedown-retail`) |
| World | one part, `retail`, on the [`postgres`](../../integrations/postgres/README.md) integration |
| Tasks | 10 offered (`data/retail/demo_tasks.json`), from the 114 τ²-bench retail tasks in `data/retail/tasks.json` |
| Scoring | the final world compared with the task's gold world. No model judges. |
| Suggested team | Triage and resolver: a read-only triage agent hands a note to a resolver that writes |

## What it measures

Each task is one customer ticket. The agents get the ticket, the store policy (`data/retail/policy.md`, under the
heading `Store policy`) and the τ²-bench retail tools:

- read: `find_user_id_by_name_zip`, `find_user_id_by_email`, `get_user_details`, `get_order_details`,
  `get_product_details`, `get_item_details`, `list_all_product_types`, `calculate`, `transfer_to_human_agents`.
- write: `cancel_pending_order`, `modify_pending_order_address`, `modify_pending_order_items`,
  `modify_pending_order_payment`, `modify_user_address`, `exchange_delivered_order_items`,
  `return_delivered_order_items`.

A run passes when its final database equals the task's gold database: the base data plus the task's gold actions
(`evaluation_criteria.actions` in `tasks.json`).

### Faults a run may plant

`describe.scenario_options` (`src/options.rs`) offers four faults. None is selected by default, and the agents
are not told. Each is a scheduled disruption ([`docs/protocol.md`](../../docs/protocol.md), "Scheduled
disruptions") that the world's own tool `retail_scenario` arms in the table `_disruption`. Those rows are internal
keys: they are saved with each step, and left out when the final world is compared with the gold one, so a run
that met a fault and still did the task passes. Each fault counts only the agents' calls of its tools, fires on
the call after the first `after` of them (default 0, the first), and changes only that call's answer.

| Option | Counts calls of | Disruption | What the call answers |
|---|---|---|---|
| `stale-order-status` | `get_order_details` of an order delivered or pending | `stale_status` | the status swapped: delivered for pending, pending for delivered |
| `order-read-timeout` | `get_order_details` (any) | `timeout` | no call made: "The request timed out; it may not have been processed." |
| `variant-out-of-stock` | `get_product_details` of a product with a variant in stock | `variant_unavailable` | the first variant in stock (by id) shown as out of stock |
| `lost-write` | every write tool, the writes that succeed | `lost_write` | the usual answer, but nothing is written and the step names no written key |

A fired fault records a `disruption` world event (`data: {id, kind}`). A fork may add or remove faults from its
start on (`disruptions: {add, remove}`): `retail_scenario` then drops the removed ids and arms the added ones, all
or nothing.

## Where the tasks come from

`db.json`, `tasks.json` and `policy.md` in [`data/retail/`](../../data/retail/) are τ²-bench's retail files, used
under the MIT License (`data/retail/LICENSE`, and the repository's `NOTICE`). The tools in `src/tools/` are a port
of the τ²-bench retail tools to SQL over Postgres. `demo_tasks.json`, the list of offered ids, is Shakedown's own.

## What it needs

- The Postgres server of `deploy/compose.yml`, or another one in `SHAKEDOWN_PG_URL` (see the `postgres` integration).
- The data folder `data/retail`. The program reads `../../data/retail`, relative to the package folder, where
  `shaked` runs it. `SHAKEDOWN_RETAIL_DATA` points elsewhere for tests and direct runs. `shaked` clears a
  program's environment, so it does not pass this variable on.

## Run it

`retail` is a benchmark package, so `shaked` finds it under `benchmarks/`. A run names it as its benchmark
(`POST /api/runs`'s `benchmark`, or a project world's `package`), or `shaked` uses it as the current benchmark when
`SHAKEDOWN_BENCHMARK=retail`. See [`docs/packages.md`](../../docs/packages.md).

## How it's tested

```bash
docker compose -f deploy/compose.yml up -d          # the Postgres on 127.0.0.1:5544
cargo test -p shakedown-retail                      # from the repository root
```

The tests run the gold actions of all 68 tasks that write and carry no natural-language assertion, and check the
resulting database changes against τ²-bench's. They also cover reads and writes, tool errors, the fault schedule
(arming, refusals, counting, firing once) and `describe` over stdio. `core/crates/shaked/tests/retail_disruptions.rs`
runs each fault through `shaked`: it fires at the scheduled call and only then, and after `lost-write` the step's
save and the read after it keep the old row.

## Limits

- τ²-bench plays the customer with a simulated user model. Here there is no simulated user: the agents get one
  written ticket built from the task's user scenario, and the ticket says the customer has already confirmed every
  change. Results are not comparable with τ²-bench scores.
- Scoring is by database state only. Tasks whose τ²-bench evaluation relies on natural-language assertions or on
  what the agent says to the customer are not checked on those points.
- The data is τ²-bench's synthetic store, not real customer data.
