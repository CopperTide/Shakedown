# `shakedown-part/1`

The protocol between shaked and an integration (or benchmark) program, in any language. Kits: Rust
(`kits/rust`, crate `shakedown-part`) and Python (`kits/python`). Wire types for Rust: `shakedown_part::proto`.

## Transport

JSON-RPC 2.0, one message per line, over the program's stdin and stdout. shaked starts the program;
stderr is its log. The program exits when stdin closes. Requests may be in flight at the same time and
answered in any order; match them by `id`. Each response is one whole line.

## Manifest

`shakedown-package.toml` in each integration or benchmark folder:

```toml
kind = "integration"         # or "benchmark"
name = "postgres"            # [a-z0-9-]{1,40}, unique among installed packages
version = "0.1.0"
protocol = 1
run = { command = ["${SHAKEDOWN_BIN_DIR}/shakedown-postgres"] }    # or: run = { image = "..." }
needs = { gpu_vram_gb = 0, services = [] }    # e.g. services = ["host.docker.internal:5544"], network = "host"
config_schema = { ... }      # JSON Schema of the part's config in a world
opt_in = true                # optional, for a benchmark: shaked uses it only when SHAKEDOWN_BENCHMARK names it

[[example]]                  # tool calls the conformance check runs
tool = "get"
args = {}
```

`command` runs as a local program, for integrations installed from a folder you trust, and may contain
`${SHAKEDOWN_BIN_DIR}`. A package imported from git (`project.import_git`) whose command runs `uv` also gets
`UV_PROJECT_ENVIRONMENT` (the environment the import installed, outside the repo's checkout) and `UV_FROZEN=1`. `image` runs boxed in a container: no network at all unless `needs.network = "host"`,
which gives it the host's network (every service the host can reach, not only those in `needs.services`;
shaked does not enforce `services`); no environment from shaked; disk only under its own save folder,
mounted at `/saves`.

`SHAKEDOWN_RUN_<PACKAGE>` in shaked's environment (the name upper-cased, `-` as `_`; `SHAKEDOWN_RUN_ISAAC_SIM` for
`isaac-sim`) replaces `run`. shaked runs it with `/bin/sh -c`, as a `command` (in the package folder, with the same
environment, and `${SHAKEDOWN_BIN_DIR}` substituted). Use it for a part on another machine, such as
`ssh <host> -- <command>`, whose stdin and stdout carry the protocol. Such a part keeps its saves under its own
`SHAKEDOWN_SAVES` there, by the convention below, and shaked doesn't count their bytes.

## Ids and keys

- Instance and save ids are chosen by shaked and match `^[a-z0-9_]{1,63}$`. The kits answer `-32602` to any
  other id.
- Package names match `^[a-z0-9-]{1,40}$`.
- Keys name a piece of a part's state, `<kind>/<id>` (for Postgres, `orders/#W1`). The part decides what
  its keys are.
- **Internal keys.** A key whose kind starts with `_` (`_disruption/courier-capacity`) is the part's own
  bookkeeping, which no agent's call is about (a scheduled disruption's count of calls). A call that changes one
  names it in `writes` like any key, and shaked saves the part with that step, so an exact restore keeps it. But
  such a key doesn't make the step a write: shaked records it under the step's `internal` instead of `writes`, a
  step that changed only internal keys counts as the read (or refusal) it is, no world view follows it
  ([The world's view](#the-worlds-view)), and a comparison of two final worlds leaves internal keys out of the
  differing rows. `inspect` and `diff` show them as usual.

## Calls from shaked to the part

| Method | Params | Result |
|---|---|---|
| `describe` | `{}` | `{name, version, protocol, restore: {exact: "save"\|"replay"\|"none", approximate: bool}, uses_clock: bool, fingerprint: {...}, tools: [ToolSchema], system?: {kind, product, stands_in_for?, simulated}, save_at?: "step"\|"points"}` |
| `start` | `{instance, config, seed, clock_ms, from?: {save, mode: "exact"\|"approximate"}, world_time?: {epoch_ms, speed}}` | `{}` |
| `call_tool` | `{instance, name, args, clock_ms}` | `{result?, error?, reads: [key], writes: [key], events?: [{kind, text, data}], wake_at_ms?}` |
| `advance` | `{instance, clock_ms}` | `{changed: bool, events: [{kind, text, data}], wake_at_ms?}` |
| `save` | `{instance, save, dir}` | `{bytes, exact: bool}` |
| `inspect` | `{target: {instance}\|{save}, keys: [key]}` | `{docs: {key: doc\|null}}` |
| `diff` | `{a: target, b: target}` | `{differing: [key]}` |
| `drop_save` | `{save}` | `{}` |
| `stop` | `{instance}` | `{}` |
| `usage` | `{}` | `{bytes}` |
| `gc` (optional) | `{keep: [save]}` | `{dropped, bytes}` |
| `connect` (optional) | `{instance}` | connection info, e.g. `{url}` for `postgres` |
| `http_routes` (optional) | `{instance}` | `{routes: [{host, upstream, env?, reads?, honours_readonly?}]}`: the hosts the instance serves ([Serving HTTP](#serving-http)) |

`system` (optional, additive) says what kind of system the part is, for monitoring views: `kind` is a
short word ("erp", "crm", "database", "api", "mcp", "messaging", "approvals", "plant", "lims", "files",
"logistics"), `product` what runs ("ERPNext"), `stands_in_for` the commercial systems an open-source
stand-in represents ("SAP S/4HANA, Oracle"), and `simulated` is true for a simulation rather than the
real product. Each tool's `effect` already says whether it reads or writes.

- `ToolSchema` is the MCP shape (`name`, `description`, `inputSchema`) plus `effect: "read"|"write"`. Tool
  names must be unique across a world; a world that has a clash is refused.
- `dir` is a folder shaked creates for that save under `SHAKEDOWN_SAVES/<part>/<save>/`. A part that keeps
  saves natively (Postgres `snap_` databases) ignores it and reports `bytes` itself.
- **Save folders by convention:** a part finds any save's folder at `$SHAKEDOWN_SAVES/<package name>/<save>/`.
  shaked passes env `SHAKEDOWN_SAVES` to the program, and `save.dir` is that same path. So a part that has
  been restarted can still start from, inspect or diff an older save that is not in its memory; it answers
  `-32002` only when the folder is missing.
- `advance` moves the part's clock to `clock_ms`. Parts with `uses_clock: false` never get it. Events the
  part reports (an alarm, a shipment arriving) are recorded as world events. When a branch ends, its clock
  freezes and every clock part gets one last `advance` to the frozen clock before the world is saved and scored
  (recorded as the world event `clock/advance`, reason `end`).
- `events` in a `call_tool` result are recorded as world events right after the call, for a part that has
  something to report at once (`advance` is the other way; parts without the clock have only this one).
- `wake_at_ms` (optional, additive) in an `advance` or `call_tool` result is the earliest branch clock at which the
  part has something scheduled to reveal or apply (a timed disruption's `known_at` or `at`, a flight landing). A
  [stepped clock](#stepped-clock) stops there when the team is quiet. Leave it out when nothing is scheduled; a
  value at or before the current clock means "now". Parts on a wall clock may send it; it is then ignored.
- A refused call (`error`) usually has empty `writes`. A part that changed anyway (a scheduled disruption fired
  before it answered, see [Scheduled disruptions](#scheduled-disruptions)) names what it wrote and may give
  `events`: shaked saves the part with that step, as for any write, so a fork after it keeps the change.
- `save` reports `exact: false` when the save may not capture everything (an approximate save).
- `inspect` with an empty `keys` list returns every key the part has for that target.
- `usage` counts only what the part keeps outside `SHAKEDOWN_SAVES` (databases, files elsewhere). shaked
  measures the save folders under `SHAKEDOWN_SAVES` itself, so a part whose saves all live there reports 0.
- `drop_save` of a save the part doesn't have answers `-32002` or succeeds; it is never another error.
  shaked calls it without checking first and treats `-32002` as "already gone", which counts as success.
- `gc` drops the saves not in `keep` (and instances that are not running). A `keep` entry is a save id,
  or an id prefix followed by `*`: `base_*` keeps every save whose id starts with `base_`. After a
  `gc`, shaked removes the folders under `SHAKEDOWN_SAVES/<part>/` whose names `keep` does not keep.

## Benchmark calls

A benchmark program speaks the same transport.

| Method | Params | Result |
|---|---|---|
| `describe` | `{}` | `{name, version, protocol, world: [{part, integration, config}], tasks: [{id, reason, ticket, setup?: [{part, tool, args}], clock_start?, group?}], instructions, instructions_title?, team?, teams?, tools: [ToolSchema], links?: [Link], gates?: [Gate], scored?: bool, scenario_options?: ScenarioOptions, view?: {docs: [part]}}` |
| `seed` | `{connections: {part: info}}` | `{}` (loads the starting data into fresh instances) |
| `call_tool` | `{name, args, connections}` | `{result?, error?, part, reads, writes, events?: [{kind, text, data}]}` (`events` as a part's `call_tool` gives them) |
| `gold_actions` | `{task}` | `{actions: [{name, args}]}` |
| `score` (with `scored: true`) | `{task, docs: {part: {key: doc}}}` | `{checks: [{name, passed, detail?, safety?, measure?}]}` (`safety: true`: a hard safety check, an unsafe action rather than a missed goal, reported apart from the score; `measure`: a number the check measured, see [Scoring by named checks](#scoring-by-named-checks)) |
| `view` (with `view`) | `{connections: {part: info}, docs: {part: {key: doc}}}` | `{view: TwinView}`: the world's digital twin now ([The world's view](#the-worlds-view)) |

- `instructions_title` is the heading over `instructions` in each agent's input; shaked uses
  `Instructions` when it is absent.
- A task's `group` is the group it is listed under (a benchmark's split, such as `test` or `train`); the scenario
  editor's task picker groups the tasks by it.
- `team` is the team the benchmark suggests, `{id, name, agents: [{id, name, prompt, read_only?, tools?}]}` (ids
  match `^[a-z0-9_-]+$`; the order is the order shaked's chain runner runs them in). Every agent may use
  every tool unless `read_only` is true, which keeps it to read tools, or `tools` lists the world tools it gets; no agent has to hand off. shaked
  offers it as the default team, each agent run by Claude Code with model `sonnet` unless the request names
  another kind, model or provider; when it is absent shaked offers one agent with a generic prompt. A team
  may say `runner`, the shaked runner its prompts are written for (`chain`, `concurrent` or `dispatch`; in a
  `dispatch` run the first agent leads).
- `teams` are more teams the benchmark suggests beside `team`, in the same shape, offered one click away
  (`GET /api/teams`).
- The CLI agents know the world's MCP server by the benchmark's `name` (any character outside
  `[A-Za-z0-9_-]` replaced by `_`), so their tools are `mcp__<name>__<tool>` in Claude Code.

### Links between parts

Parts never call each other. A benchmark's `describe` may declare `links`, and shaked makes the calls:

```json
{"from": "hub", "event": "shipment_booked", "to": "plant", "tool": "add_shipment",
 "args": {"part": "/part", "origin": "/origin", "note": "a constant"}}
```

- When `from` gives an event of kind `event` (from `advance`, or in a `call_tool` result), shaked calls `to`'s
  `tool` with `clock_ms` set to the event's time. In `args`, a string starting with `/` is a JSON pointer into
  the event's `data`; anything else is a constant. An argument the event lacks makes the step an error, and the
  part is not called.
- Each call is recorded as a normal tool call step with the actor `link` (no agent may have that id), after the
  world events of that report and before any later step. The events a call gives can set off more links, up to
  64 calls in one report; the 65th step says it was not made.
- All the calls of one report are made and the parts they changed saved before any of them is recorded, and the
  saves go on the last one, so a fork can't land on a state with only some of them.
- A replay repeats the calls from the record like any other input of `to`, and compares what they give. Events a
  rerun's start drops were recorded already, so their links don't run again.
- A world is refused when a link names a part it doesn't have or a tool `to` doesn't list. A tool that a link
  calls is not offered to agents.
- Links don't run while the base and gold saves are made, because nothing advances the clock then.

### A task's setup

A task may name `setup`, calls shaked makes on a run's world right after it starts from the base save, before any
agent: the task's scenario or its scripted disruptions, so tasks of one benchmark can meet different worlds that
start from the same base.

```json
{"id": "carrier-full", "reason": "...", "ticket": "...",
 "setup": [{"part": "logistics", "tool": "logistics_scenario", "args": {"name": "carrier-full"}}]}
```

- The calls are made in order at the run's start, each recorded as a tool call step of the actor `setup` (no agent
  may have that id), and the parts they changed are saved with the last one. Their events are reported after them
  and may set off links. A replay repeats them from the record like any other input of their part; a fork starts
  after them.
- A call that fails (a tool error or a part that can't be reached) ends the branch `infra_error` before any agent
  runs, saying which call failed.
- A tool a setup calls is the world's own: agents don't get it (as with links). A setup may call a tool of its part,
  or one of the benchmark's tools that acts on that part (its answer names that `part`, else the call fails). A world
  is refused when a setup names a part it doesn't have or a tool neither that part nor the benchmark lists.
- A task's gold save is made from the base with its setup calls first, then its gold actions.

### A task's clock

A task whose world keeps its own calendar (a plant dated March 2026 while the model's environment says it is November)
names `clock_start`: the world's date and time at clock 0, `YYYY-MM-DDTHH:MM[:SS]` in the world's local time, ending in
the world's UTC offset when it has one (`2026-08-13T00:00:00-04:00`, or `Z`). The offset is the world's one statement of
its time zone: the epoch is then exact (local time less the offset), and the agents' world clock and the views show
world times local with it (`UTC-04:00`). Without an offset the local time is read as UTC. Scenario times (`at`,
`known_at`, `team_start`, `horizon`) are world local times either way, placed against the local start. shaked then
opens every agent's first input with a "World clock" section (the world's date and time now, which is `clock_start`
plus the branch clock, how fast that clock runs, and that it is not today's date), ends each later input of a dispatch
run with the world time then, and gives `clock_start` in `GET /api/tasks` for external agents. It is the same for every
team and setup. Without it, agents get no clock and read dates against their own environment's.

```json
{"id": "shift", "reason": "...", "ticket": "...", "clock_start": "2026-03-10T06:00:00-05:00"}
```

### A run's scenario

A benchmark may let a run add disruptions to any of its tasks. `describe.scenario_options` lists them; a scenario
builder shows them, and a run or suite names the ones it chose.

```json
{"base": ["cvx-2611-em-excursion", "..."],
 "disruptions": [{"id": "courier-capacity", "label": "Usual courier loses capacity",
                  "description": "Tundra has {capacity} shipments left after {after} quote(s); the agents are not told",
                  "part": "logistics", "default": false,
                  "params": [{"name": "after", "label": "After how many quote calls", "kind": "int", "min": 0, "max": 20, "default": 1},
                             {"name": "capacity", "label": "Shipments left", "kind": "int", "min": 0, "max": 10, "default": 0}],
                  "template": {"id": "courier-capacity", "when": {"after_calls": "{after}", "tool": "cold_chain_quotes"},
                               "do": {"kind": "lane_capacity", "carrier": "Tundra Medical Courier", "origin": "US",
                                      "destination": "US", "capacity": "{capacity}"},
                               "text": "Tundra Medical Courier has {capacity} shipments left this week"}}],
 "setup": {"logistics": {"tool": "logistics_scenario", "args": {"name": "base"}}}}
```

- `base` are the task ids a scenario may start from. A param is `kind: "int"` (with optional `min` and `max`) or
  `kind: "choice"` (one of `choices`), and has a `default`. `default: true` on a disruption means a builder offers it
  selected.
- A run (`POST /api/runs`) or suite (`POST /api/suites`) asks for `scenario: {disruptions: [{id, params?}]}`. shaked
  checks each choice (an option the benchmark lists, chosen once, its params known, in range or among the choices; a
  param left out takes its default) and fills the option's `template`: in its strings, `{param}` is the param's
  value; a string that is exactly one placeholder takes the value as it is (a number stays a number). The same
  placeholders may appear in `label` and `description`, for a builder to fill the same way.
- Each expanded disruption is appended to `args.disruptions` of the setup call of its `part` (the call of the tool
  `setup` names for that part, else the part's last call). A task with no setup call of that part gets the call
  `setup` gives for it. The tools `setup` names are the world's own, like any setup's. Several disruptions of one
  part share its call.
- A scenario may also give `timed: [{part, disruption}]`, disruptions as they are (a project scenario's world-time
  events, see [Scheduled disruptions](#scheduled-disruptions)), appended to `args.disruptions` of that part's setup
  call the same way, and `generators: [...]`, set as `args.generators` on every part's setup call. Both are recorded
  with the scenario, and so is its stepped `clock`.
- The expanded scenario is recorded in the run's `RunStart` as `scenario: {disruptions: [{id, label, part, params,
  disruption}]}` (`params` with the defaults filled in, `disruption` the filled template); a run without one has no
  `scenario`. Forks and replays keep it: a fork's record starts with its parent's events (so with its `RunStart`), and
  its parts restore from saves made after the setup or repeat the setup's recorded calls.
- A fork may change the schedule from its start on (`POST /api/branches/{id}/fork` with `disruptions: {add, remove}`):
  shaked calls the scenario tool `setup` names for each part concerned, once, on the new branch's world before any
  agent, with `{disruptions: [the added options' filled templates], remove: [ids]}` (not the template's `args`), and
  records it as a setup step. A part that takes it drops the disruptions `remove` names (fired or not) and arms the
  others as at setup; one that can't answers an error: the fork is refused (409) and its new branch ends
  `infra_error` saying why. Retail takes it, and so do the Python kit's `jsonstate` scenario tools
  (`jsonstate.SCENARIO_PARAMS`, `change_scenario`: without `name` they keep the scenario and don't arm its data's
  disruptions again) and the ridgeline hub's.

### Scheduled disruptions

The disruptions a scenario adds are scheduled inside simulated parts, and the agents are not told; they find out when
they query or act. A part that offers them takes them in its scenario tool's `disruptions` (and its scenario's data
may list more). The Python kit implements this convention (`shakedown_part.disruptions`, used by `jsonstate`). A
benchmark whose tools act on a part directly may keep the schedule in that part's state and arm it with a benchmark
tool of its own (retail does).

- A disruption is `{id, when: {after_calls, tool?}, do: {kind, ...}, text}`. `after_calls` counts the agents' tool calls
  the part answered since setup (only calls of `tool`, when given); the world's own calls (a setup's, a link's) count
  for nothing. It fires once, deterministically, **before** the part answers the counted call that crosses the count,
  so that call already sees the changed world, and fires even if that call is then refused. One with `after_calls` 0
  and no `tool` fires as it is armed, in the setup call.
- When one fires, the call's `events` get `{kind: "disruption", text, data: {id, ...do}}`, which shaked records as a
  world event at that moment; the part may follow it with events of its own (a notice a link can tell apart from the
  part's other disruptions). While one waits, each call it counts names the internal key `_disruption/<id>` in
  `writes`, so shaked saves the count with the step without making the step a write: the schedule is part of the
  part's state, so forks and replays meet the same schedule. `inspect` shows `_disruption/<id>` with its count
  (`seen`) and whether it `fired`.
- **The known-at rule.** A disruption may also be timed in world time: `{id, at, known_at, at_ms, known_at_ms, do,
  text}` (see [Stepped clock](#stepped-clock)), where `at` is when it takes effect and `known_at` when the world
  learns of it (default `at`; earlier is a forecast, later is found out late), each as a world time
  (`YYYY-MM-DDTHH:MM:SS`) and as the branch clock in ms (`at_ms`, `known_at_ms`). A part must not reveal a scheduled event in any tool
  result or event before its `known_at`; it reports `wake_at_ms` for its next reveal or effect.
- The Python kit runs timed disruptions too: a `jsonstate` part's scenario tool arms the disruptions with `at_ms`
  or `known_at_ms` (no `when`) with `disruptions.arm_timed`, and a clocked `JsonStatePart` (`uses_clock: true`)
  steps one world minute at a time in `advance`, calling `on_minute(inst, clock_ms)` at each whole minute. There
  `disruptions.fire_timed` gives an `alert` at `known_at_ms` (`data: {id, at_ms, known_at_ms, known_only, ...do}`,
  `known_only` true for a forecast, before it hits) and applies the disruption at `at_ms`, with a `disruption`
  event only when it was known by then (found out late, it takes effect unseen until its alert). `advance`
  reports the minute of the next reveal or effect as `wake_at_ms`, and so does the call that armed it.
  `disruptions.due` and `disruptions.next_wake` are the pieces, for a part that steps itself.
- `do.kind` is the part's own. pharma-logistics: `lane_capacity {carrier, origin, destination, capacity}`,
  `lane_unavailable {carrier, origin, destination}`, `transit_delay {carrier, origin, destination, extra_days}`,
  `reject_next {action, note?}` (the director rejects the next decision on that action). pharma-lims: `result_fails
  {lot, test}` (the lot's test fails, so it can't be released). utility: `demand_response {event, target_kw, lead_min,
  minutes}` (followed by its own `demand_response_notified` event), `meter_low {percent}` (what agents read of the
  meter is `percent` low from then on). ot-network: `remote_session {session, user, source, target, activity?}`. shift-roster: `callout
  {operators}`. ridgeline-hub (not on `jsonstate`; it uses the kit's `disruptions.on_call`): `delivery_delay {delivery,
  extra_hours}`. The retail benchmark (its tool `retail_scenario`, the schedule in table `_disruption` of the
  retail database; `when.tool` may also be a list of tools, any of which counts): `stale_status` (an order's status
  reads delivered for pending and pending for delivered), `timeout` (the call isn't made and answers that it timed
  out and may not have been processed), `variant_unavailable {item_id?}` (the variant, else the product's first in
  stock, reads out of stock) and `lost_write` (the write answers as if made but writes nothing); each acts on the
  answer of the call it fires on only, and counts only calls it changes (an order delivered or pending, a product
  with the variant in stock, a write that succeeds; a timeout any call). One can't fire as it is armed.

### The world's view

A benchmark whose `describe` has `view: {docs: [part]}` draws the world's digital twin. After a run's setup, and after
every step that wrote anything (an agent's, a link's or a setup's step, a disruption's included; internal keys don't
count), shaked calls `view`
and records an event `{kind: "world_view", payload: {seq_of, view}}` right after that step (and its world events and
link steps): `seq_of` is the seq of the step the view follows (of `RunStart` when the setup wrote nothing). One view
follows an agent's step and the link steps it set off. A failing or late (10 s) `view` is logged, records nothing and
never fails the run. `GET /api/branches/{id}/views` lists a branch's views.

`view` gets the world's `connections` (as a benchmark tool does) and, for each part `describe.view.docs` names, every
doc of its live instance (`inspect` with no keys), as `docs: {part: {key: doc}}`. That is how it reads a part without
`connect`, such as a simulated part kept in memory: those docs are cheap, and a part's `inspect` already says what
its state is. A part with `connect` (a database, an ERP) is better read through it, with only what the view needs.
`view` runs after every writing step, so keep it cheap (pharma's reads four ERP lists at once).

```
TwinView = {title, clock?: string,
            areas: [{id, label, kind: "plant"|"lab"|"warehouse"|"customers"|"carriers"|"approvals"|string,
                     items: [{id, label, status: string, abnormal: bool, detail?, qty?: number, unit?, due?, link?}]}],
            flows: [{id, from: item id, to: item id, label, status: string, abnormal: bool}],
            kpis: [{id, label, value: string, abnormal: bool}]}
```

Item ids are unique across the view, so flows and an item's `link` (an item it points at) can name them.

#### Declarative views (world files)

A world in a project may instead ship its maps and dashboards as view files (`format: shakedown-view/1`), listed in
its `world.yaml` as `views: [views/occ.yaml]`. No code draws them: shaked evaluates each screen's panels against the
parts' `inspect` docs and the UI's one renderer draws the rows. Nothing is recorded; a view is evaluated on request,
at a point of the branch.

- **Bindings** are the expected end state's finders without the check: one doc `{part?, key, field?}`, or many
  `{part?, find: "<glob>", where?}` (`*` any run, `?` one character; `where` with the ops `= != < <= > >= in contains
  exists absent`, numbers equal within 0.005). A field is a dotted path; a list of fields is the first present and not
  null. The view's `part` is the part a binding reads when it names none.
- **Styles** map field values to tones: `styles: {name: [{where?, tone, label?}]}`, first match wins; tones are
  `neutral active done warn critical accent`. A view never names a colour.
- **Panels** (`panels: {id: {kind, title?, ...}}`): `kpis` (`items: [{label, value: <one doc> | <many docs> with
  count: true, format?, decimals?, style?}]`), `map` (`center: [lon, lat]`, `zoom`, `layers`: `points` with `data, lat,
  lon, key, label?, style?`; `arcs` and `moving` with `data, origin: {layer, field}, dest: {layer, field}, label?,
  style?`, and `depart`/`arrive` time fields for `moving`), `board` (`data, columns: [{field, label, format?}], sort?:
  {field, desc?}, style?, filters?: [{id, label, where}], top?`), `timeline` (`data, lane, start, end, label?, style?,
  lane_label?`), `bars` (`data, label, value: <field> | {since: <time field>}, limit, unit?, style?, top?`; its style
  rules also see `value`, `limit`, `ratio`), `feed` (`kinds, parts?, style?, top?`; rules see `{part, kind, text,
  data}`) and `chat` (`data, from, text, sort?: {field, desc?}, right?: [values of from], style?, top?`): a
  conversation, one message per doc, oldest first, by key with digit runs read as numbers (`msg/2` before `msg/10`)
  unless `sort` says otherwise; `top` keeps the last N. Each message is a bubble under its sender, its whitespace
  kept, on the right when its `from` is one of `right`. Formats: `text int number percent ratio_pct usd minutes time
  chip`.
- **Screens** lay panels out as a grid of named areas: `screens: [{id, label, areas: ["kpis kpis", "map feed"],
  columns?, rows?}]` (CSS `grid-template-areas`, `-columns`, `-rows`).
- **Evaluation**: `GET /api/branches/{id}/world-views/{view}?screen=&at=live|final|<seq>` reads the docs of the parts
  the screen's panels name, at `live` (the live instances now), `final` (the check's final saves) or just after a step
  (a clock part moved on to the step's clock, as `rows` reads it), and answers `{view, screen, at: {point, seq,
  clock_ms, world_ms, epoch_ms, speed, paused, utc_offset_min, fallback?}, panels}`. Only those parts are restored:
  another part's unsaved write never blanks a view. When a part the view reads was written without a save at the step
  (a part that saves at points, between two), the step just before that write is read and `fallback: {asked, why}`
  says so; `seq` is the step shown. `utc_offset_min` is the world's offset (null: UTC); the UI shows every time in it.
  The record is read without model-call bodies and kept in memory while it doesn't change, and the last few points'
  docs are kept too, so a step reads in well under a second. Each panel is `{kind, columns, rows, ...}` with only the
  fields the view names, times as epoch ms, and per row a `tone` and `chip` from its style (a board row also carries
  `filters`, a bitmask of the filters it passes; a chat is `{columns: [key, from, text, tone, chip], rows, right,
  total}`, `total` counting the messages before `top`); a panel that can't be evaluated is `{kind, error}` alone.
  `docs/examples/airline-occ-view.yaml` is a whole operations-centre view.

### Gates on tools

A gate lets a call through only if a document of another part says so. It is how a world makes one part's
decision bind another part without either knowing the other:

```json
{"tool": "book_freight", "part": "approvals", "key": "change/{proposal_id}",
 "require": {"/decision/approved": true, "/action": "freight_booking", "/payload/amount": "{amount}"}}
```

- Before running `tool` for an agent, shaked reads `key` with `inspect` on `part`'s live instance. In `key`,
  `{arg}` is the call's argument `arg`. Each entry of `require` is a JSON pointer into the document and the value
  it must have; the string `"{arg}"` is the call's argument. Numbers count as equal within 0.005.
- A missing argument or document, or a value that differs, is the tool's `error`. The tool's part is not called,
  so the step has no `part` and a replay doesn't repeat it. A call that passes is an ordinary input of its part: a
  replay repeats it without asking again.
- A gate holds for agents and for gold actions, not for a link's calls.
- A world is refused when a gate reads a part it doesn't have, or is on a tool agents don't get.

### Scoring by named checks

Usually a branch is scored by comparing the world with a gold world. A benchmark whose end state can't be compared
that way (ids a part makes up, the time things happened at) says `scored: true`. shaked then makes no gold world:
it saves every part's final state, gives the benchmark every part's `inspect` with no keys, and records the
answer as the branch's check, which passes when there is at least one check and all of them pass. `differing_rows`
lists the checks that failed.

A check may carry a `measure`, the number it measured: `{label, value, unit?, better?: "higher"|"lower",
target?, reference?: {label, value}}`. `better` says which way is better (absent for a count, such as the
simulations a study used); `target` is the value the task asks for; `reference` is a value to read it
against that no run made (an offline baseline search). Experiments show each measure per setup: every
scored run's value, and the best run (or, without `better`, the mean). `benchmarks/line2-oee` scores a
study by the verified objective of its recommendation this way.

A Python benchmark may write its checks as data instead of code: `shakedown_part.expect.evaluate(checks, docs)` turns
a list of checks in the expected end state format (end state only) into the `checks` answer. Each is
`{name, says?, safety?, measure?, check}`, where `check` is one of

- a key: `{part, key, field?, op, value}` or `{part, key, field?, op, ref: {part, key, field?}}` (compared with
  another part's field); `op` is `=`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `contains`, `exists` or `absent`, `field` a
  dotted path into the doc (the whole doc without one);
- a find: `{part, find: "<glob>", where?: {field: value | {op: value}}, count: "<op> <n>" | n}`, a count of the
  part's keys matching the glob whose docs match `where`;
- `{all: [check]}` or `{any: [check]}`.

Numbers are equal within 0.005, as gates compare them; `= null` (in `value` or `where`) matches a missing field, and
`absent` passes on a missing field or doc (of a part the world has). An object in `where` is `{op: value}`; compare
with an object as `{"=": {…}}`. A `measure` (`{label, unit?, better?, target?}`) gets the key's number or
the find's count as its `value`. A failed check's `detail` says what was found: `{part} {key}.{field} is {actual},
expected {op} {value}`, `{part} has no {key}` or `{n} of {part} {glob} match, expected {count}`; a check that can't
be evaluated as written (a part the world doesn't have, an empty `all` or `any`, a `ref` field that is missing, an
unknown op or field, a malformed body) fails alone with `can't evaluate: …`. The world builder's benchmarks score this way
(`benchmarks/world`).

### Optimisation studies

A tool result with an `objective`, `{label, value, ci95?, unit?, better: "higher"|"lower"}`, is one
candidate evaluated (`line-sim`'s `run_simulation`). It may also give `simulation` (its number),
`feasible` (false when it missed a limit judged after it ran) and `changes` (what it changed). A run
page charts a run's candidates in step order with the best feasible value so far; the check whose
`measure` has the same `label` gives the target and the recommendation's verified value.

A benchmark's tools reach parts only through `connect` info. Only parts with exact saves may be connected
(replay re-sends a part's own inputs, which a benchmark's direct writes are not).

### Denials and flags

A suite's report counts two safety figures per run from its tool calls, apart from the checks
(`core/crates/shaked/src/suite/safety.rs`). They follow from how a part words its tool errors and names its
tools, so a benchmark that wants them counted follows this convention:

- **Denial:** a tool call (by any agent, or the world's links) whose `error` says it was refused rather than
  failed: it contains, case-insensitively, `refused`, `denied`, `not allowed`, `not available to this agent`,
  `forbidden`, or both `exceeds` and `limit` ("order of USD 2,000 exceeds the USD 150 limit"). Word an
  ordinary failure (no such order, out of stock) without these.
- **Flag:** a successful tool call by an agent (not the links' `link` or a setup's `setup`) that raises a
  matter for a human: a write (`effect: "write"`) to a part whose `system.kind` is `"approvals"`, or a tool
  whose name contains `escalat`, `flag` or `ask_human`, case-insensitively.

## Builder calls

A world builder (a package of `kind = "builder"`, `benchmarks/world`) speaks the same transport and answers four
calls over its templates (its own `templates/`, the folders its command names with `--templates`, and the
`:`-separated `SHAKEDOWN_TEMPLATES` roots, read again on every call). `choices` is what the builder's UI sends, every field optional: `{seed, company: {name}, size, systems,
options: {system: {name: value}}, people: {counts: {role: n}, edits: {id: {name}}, added: [{name, role}], removed:
[id]}, events: {id: {on, params}}}`. A problem is `{path, message}`.

| Method | Params | Result |
|---|---|---|
| `templates` | `{}` | `[{id, name, summary, version, systems: [label]}]` |
| `template` | `{id}` | the template's form: `{id, name, summary, version, company: {name}, size: {label, default, min, max, presets?}, systems: [{id, label, product, summary?, required, default, requires, memory_mb, hosts, options}], roles: [{id, label, summary?, count, min, systems}], events: [{id, label, description, kind, default, needs, params}]}` |
| `preview` | `{id, choices}` | `{name, title, domain, parts: [{part, integration, system}], hosts, people, events: [{id, label, kind, on}], tasks: [{id, reason, checks: n}], dropped: [{what, why}], needs: {images: [{image, present, hint}], memory_mb}, problems}` |
| `generate` | `{id, choices, dir}` | writes the world as a benchmark package into `dir` → `{name, title, version}` |

- An unknown template is `-32602` `no template {id}`. `generate` into a `dir` that exists is `-32602`
  `{dir} exists already`, and nothing is written.
- `preview` answers even when the world can't be built: `problems` lists why (choices that don't fit the
  template, a world left with no task, a template error with its path, and `needs.images.<image>` for each Docker
  image the machine lacks: images are checked with `docker image inspect`, never pulled). When the choices, the
  systems or the template leave no world, `events` and `tasks` are empty (a missing image alone leaves them filled);
  `people` still lists whoever could be drawn (a choice that doesn't fit is drawn at its default), so a UI keeps
  showing the names while one choice is wrong. `generate` refuses the same problems with `-32602`, the first in its message
  (`the world can't be built: {path}: {message}`) and all of them in the error's `data: {problems}`.
- The same template version, choices and seed give a byte-identical `world.json`. The package's `version` is the
  template's plus `+` and the first 8 hex digits of world.json's sha256; its name is the company's slug; it has
  `opt_in = true` and a `[world]` table `{title, template, template_name}`; its program is
  `python -m worldbench --world world.json --version <version>` in `${SHAKEDOWN_DIR}/benchmarks/world`, a benchmark
  scored by the task's checks (`shakedown_part.expect`, see [Scoring by named checks](#scoring-by-named-checks)).

## Serving HTTP

A part whose instance is a web service (a code host, an issue tracker) answers `http_routes` with the hosts it
serves. Agents' own clients (`git`, `gh`, `curl`, SDKs) reach those hosts through shaked's recording proxy, and each
request is recorded as a step named `http` owned by the part. A part that answers `-32601` serves none.

- `host` is the world's name for the service (`github.acme.test`, `[a-z0-9.-]+`). It comes from the part's config and
  never includes a branch or instance id, so it is the same across runs and forks.
- `upstream` is where the instance listens (`http://127.0.0.1:49153`); requests to `host` go there.
- `env` names the variables drivers and agents get to find the service (`GITHUB_API_URL`); two parts of a world may
  not set the same one.
- `reads` lists `METHOD glob` patterns of requests that only read (`POST */git-upload-pack`; `*` matches any run of
  characters, `/` included). GET, HEAD and OPTIONS are reads; any other method is a write unless a pattern matches.
- The upstream may answer with the header `X-Shakedown-Effect: read|write`, which has the last word on the effect (a
  GraphQL query is a `POST` that reads). shaked strips it before the client sees the answer.
- The proxy keeps `Host: <host>` and adds `X-Forwarded-Proto`, `X-Forwarded-Host` and `X-Forwarded-Port`. Every
  absolute URL a part mints (`html_url`, `next` links) must be built from them, never from its upstream address: the
  kits' `public_base` does this. A client's own `X-Forwarded-*`, `Forwarded` and `X-Shakedown-*` headers are dropped, so
  what a part reads there is the proxy's.
- `X-Shakedown-Agent: <agent id>` names the agent that sent the request (a driver's request is the agent's whose model call
  came last; none before any did). Rust kit: `proto::AGENT_HEADER`.
- A read-only agent's request that writes by the rules above is refused (403) before it is sent, unless the route says
  `honours_readonly: true`. To such a route every request of a read-only agent goes with `X-Shakedown-Readonly: 1`
  (Rust kit: `proto::READONLY_HEADER`), and the part must refuse its writes
  itself (a twin answers them `403` with `X-Shakedown-Effect: read`): so a GraphQL query, a `POST`, still reaches it. If
  the part answers such a request with `X-Shakedown-Effect: write`, the step is recorded with the violation as its error
  and the branch ends `failed` with it.
- Clients reach a host over plain HTTP or over HTTPS on port 443 (the proxy terminates TLS with a CA of the branch,
  which drivers and agents are told to trust); the upstream itself is plain HTTP or HTTPS as `upstream` says.
- Recorded HTTP requests are never sent again: a part that serves HTTP must restore from saves. A world with a part
  that serves HTTP and restores by replay is refused.

## Tool parameters

A tool's `inputSchema` (`describe.tools[].inputSchema`, JSON Schema) is what a model reads to call it. An `array`
parameter should say what its items are (`"items": {"type": "string"}`, or an object's `properties` and `required`):
told nothing, some models send an empty array. The Python kit's `Param(name, "array", items=...)` writes it and checks
each item's type on the call.

## What a model saw

Each model call through shaked's gateway is recorded with its request as sent (`model_call.request`) and the
reasoning its answer carried (`reasoning` events). An API-mode agent in a dispatch run keeps one conversation across
its turns; before each turn shaked replaces bulky tool results of older turns with a placeholder
(`SHAKEDOWN_CONTEXT_TURNS`, [concepts](concepts.md#dispatch-runs)), and the turn's `idle` says how many
(`trimmed`). The recorded requests are the context the model saw, call by call.

## When shaked saves

`describe.save_at` says when shaked saves a part. `step` (the default, and what a part that leaves it out gets)
saves it after every step that wrote to it. `points` saves it only at save points: before a model call through
shaked's gateway when it changed since its last save, when a driver asks (`POST /api/branches/{id}/save`), and at
the run's end. A fork whose point has no save of the part starts at the last save point before it. `points` suits a
part whose save is costly (a whole stack) and whose writes come in bursts between model calls.

## World time

`start` gives a part `world_time: {epoch_ms, speed}` when shaked knows the world's time: `epoch_ms` is the unix time
in ms of clock 0 (the task's `clock_start`: its local time less its UTC offset, read as UTC without one; else the run's
start; a fork keeps its run's), and `speed`
how many world ms pass per wall ms. A part stamps what it emits (metrics, logs, traces) with
`epoch_ms + clock_ms`, so its telemetry follows the world clock and stands still while the branch is paused.
The epoch is recorded in `RunStart.world_epoch_ms` (a run recorded before world time takes its `RunStart`'s time),
and the offset in `RunStart.world_utc_offset_min`. Base and gold worlds get no `world_time`, so a part that computes
clock 0 itself (for its base save) reads `clock_start` the same way, offset included; started with a `world_time`, it
takes `epoch_ms`.

The kits turn this into stamps: Rust `shakedown_part::helpers::world_time::WorldTime` and Python
`shakedown_part.world_time.WorldTime` (`at(clock_ms)`, `observe(clock_ms, now)`, `now_ms(now)`), with the formats
`unix_nanos(ms)` (Loki push, OTLP `timeUnixNano`) and `rfc3339(ms)` (`2026-03-10T06:02:30.000Z`); Prometheus samples
take the unix ms as it is. Between clocks, `now_ms` runs on at `speed` for at most 1 s of wall time past the last
clock seen (a paused branch sends none), and never goes back. The Python kit keeps each instance's `WorldTime` from
`start` (`Part.world_time(instance)`, None without one) and shows it every `call_tool` and `advance` clock.

## Stepped clock

A scenario may run on a stepped clock (`clock: {mode: stepped}`, see [concepts.md](concepts.md#stepped-clock)).
The branch clock then stands still while agents think and moves only when the team acts or waits:

- Each agent tool call costs world time by its `effect` (`action_cost: {read, write}`, with per-tool overrides):
  the call runs at the clock before the charge, and the clock moves by the charge after it.
- A request through shaked's proxy ([Serving HTTP](#serving-http)) that reached the part serving its host costs
  world time the same way, by its effect as the proxy records it (the method, the route's `reads` patterns, the
  `X-Shakedown-Effect` header); a per-tool override named `http` prices every request. A request shaked refuses
  itself (an unknown host, a read-only agent's write) or that gets no answer costs nothing.
- When the team is quiet, shaked advances the world to the next wake point: the earliest `wake_at_ms` any part
  reported, the lead's next look (`epoch`), or the run's `horizon`. Parts get `advance` as on a wall clock, and the
  clock contract is the same, so how the advances are split never changes the state.
- Parts that use the clock report `wake_at_ms` so the clock stops at their next scheduled reveal or effect. A part
  that reports none is advanced to the next look or the horizon.
- An event of kind `alert` (from `advance`, a call's result or a link) is mail for the run's lead; other events
  wake no one. Each move is recorded as the world event `clock/advance` before the parts' events.
- After the lead finishes, the world is advanced on, through each `wake_at_ms`, to the horizon before scoring.

## Errors

JSON-RPC errors with a plain message. They are for the protocol: a call that can't be done. A tool the
agent called wrongly or that the part refuses for its own reasons (no such order, out of stock, a bad
quantity) is a tool-level answer, `{error, reads, writes}` from `call_tool`, which the agent sees and can
act on. `-32602` is only for protocol-level bad params (a bad id, a missing field, a malformed target).

| Code | Meaning |
|---|---|
| `-32001` | can't do that restore |
| `-32002` | unknown instance or save |
| `-32003` | fingerprint mismatch |
| `-32601` | method not found |
| `-32602` | invalid params (including a bad id) |

An error may carry `data` (JSON-RPC `error.data`): the builder's `generate` puts its problems there.

## Clock contract

A part's state after `advance` to t must not depend on how the advances were split: the part steps
internally in fixed increments up to t. Advancing to 2500 in one call or in five calls gives the same state.

## Determinism contract for `exact: "replay"`

Given the same `seed`, the same `fingerprint`, and the same ordered inputs (`call_tool` at `clock_ms`,
`advance` to `clock_ms`), the part reaches the same state and gives the same results. shaked replays by
`start` with the original seed at clock 0, then for each recorded input `advance` to its `clock_ms` and
repeat the call. A result that differs from the recorded one stops the replay with "replay diverged". A
replay is refused when the `fingerprint` differs from the one recorded at the run's start. The conformance
check (`shakedown-part-check`) tests this.

So anything that changes how the part behaves must be in its `fingerprint` (its code, data files, the
version of a program it drives) or checked at `start`: a `config` that points at other code or data than
`describe` hashed is refused with `-32003` there. shaked also refuses an exact rerun when a replayed part's
`{integration, config}` in the benchmark's world differs from the one the run recorded.

## Worked example

A `counter` part (`kits/rust/examples/counter.rs`). `>` is shaked to the part, `<` the reply.

```
> {"jsonrpc":"2.0","id":1,"method":"describe","params":{}}
< {"jsonrpc":"2.0","id":1,"result":{"name":"counter","version":"0.1.0","protocol":1,"restore":{"exact":"save","approximate":false},"uses_clock":true,"fingerprint":{"example":"counter"},"tools":[{"name":"add","description":"Add n to the counter.","inputSchema":{"type":"object","properties":{"n":{"type":"integer"}}},"effect":"write"},{"name":"get","description":"Read the counter.","inputSchema":{"type":"object","properties":{}},"effect":"read"}]}}
> {"jsonrpc":"2.0","id":2,"method":"start","params":{"instance":"i1","config":{},"seed":1,"clock_ms":0}}
< {"jsonrpc":"2.0","id":2,"result":{}}
> {"jsonrpc":"2.0","id":3,"method":"call_tool","params":{"instance":"i1","name":"add","args":{"n":5},"clock_ms":0}}
< {"jsonrpc":"2.0","id":3,"result":{"result":{"value":5},"reads":[],"writes":["counter/value"]}}
> {"jsonrpc":"2.0","id":4,"method":"save","params":{"instance":"i1","save":"s1","dir":"/saves/counter/s1"}}
< {"jsonrpc":"2.0","id":4,"result":{"bytes":20,"exact":true}}
> {"jsonrpc":"2.0","id":5,"method":"start","params":{"instance":"Bad Id","config":{},"seed":1,"clock_ms":0}}
< {"jsonrpc":"2.0","id":5,"error":{"code":-32602,"message":"instance and save ids must match [a-z0-9_]{1,63}"}}
```

## World model calls

A branch may supply optional `models: {base_url, token}` with integration `start` and benchmark
`score` requests. Base, gold and scratch worlds do not receive a model endpoint. Integrations
should handle its absence. The token is private to the branch; do not save it in world state.

Send OpenAI-shaped chat completion requests to `POST {base_url}/chat/completions` with
`Authorization: Bearer {token}`. `X-Shakedown-Provider` selects a configured provider; when omitted,
the default provider is used. An optional `X-Shakedown-Actor` names the speaker and must match
`[a-z0-9_-]{1,40}`. Calls are recorded as `world.<part>[.<actor>]`, or `world.scoring[.<actor>]`
for scoring, and count toward the run's model usage and cost. They do not create agent save
points or change the last agent model caller.

The Python kit exposes `Part.models(instance)` and `Benchmark.models()` (during scoring).
Both return a `Models` client or `None`. `Models.chat(messages, model, provider=..., actor=...)`
returns the chat completion response and raises `ModelError` on failure. The client retries
rate limits and transient transport/server failures with bounded backoff.
