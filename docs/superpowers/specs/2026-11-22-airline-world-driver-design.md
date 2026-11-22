# Airline world driver: design (project 1 of 4)

Date: 2026-11-22. Status: draft for owner approval.
Background research: `~/MLSys/CopperTide/reports/Airline operations simulation build.md` (published at
https://claude.ai/artifact/Vc8La2oxNzHzdoceBvZM3h) and its notes in
`~/MLSys/CopperTide/research_notes/Airline operations simulation build/`.

## 1. Intent

The owner wants a few fun, realistic airline-operations demos. The demos should show, on the CopperTide cloud
platform, how to go from zero to a complete result:
- all the code we wrote;
- the open-source tools we integrated, and how;
- live runs and their results.

The UI should look good and not "AI-generated". It is our own design; nothing is copied from Skymerse or anyone
else.

The work is four projects, in the order the owner approved:
1. **World driver (this document).** The stepped clock and world-time disruptions in Shakedown's core. The simulated
   physical airline. Crew legality rules. Dollar scoring.
2. **Open airline stack in its own repo.** Movement control, crew tracking and maintenance control systems; ERPNext;
   a solver service; data loaders.
3. **View language and renderer in Shakedown core.** Map, flight board, aircraft timeline, duty bars, alert feed.
4. **Import from git, cloud runs, and Demo 1 end to end, recorded.**

## 2. Goals, non-goals, anti-goals

**Goals for project 1:**
- **G1. Stepped clock in core.** World time moves only when Shakedown moves it. An agent that thinks slowly does
  not change the outcome, and the same seed and the same decisions give the same run.
- **G2. World-time disruptions in core.** A scenario event fires at a world time (`at`). It can stay hidden until a
  later world time (`known_at`). Today, scenario `at` times are parsed but never run, and disruptions fire only
  after N agent calls.
- **G3. The airline driver part.** This is the simulated physical airline:
  - aircraft flying rotations;
  - primary delays sampled from BTS data, with knock-on delay emerging from aircraft, crew and connections;
  - crew duty clocks;
  - aircraft defects and aircraft grounded for parts;
  - weather capacity programs (ground delay programs);
  - passenger connections, modelled as groups.

  It emits real-world events (movements, crew check-ins and sick calls, defects) and accepts operational commands
  (swap, delay, cancel, ferry, assign crew, release an aircraft).
- **G4. Crew legality rules as a small open-source Python package.** Rule tables are data files: FAR 117 Tables A
  and B, cumulative limits, rest rules, flight-attendant §121.467, maintenance intervals, MEL categories. The
  driver uses it to flag violations, and later the crew-tracking system (project 2) uses it to refuse illegal
  assignments.
- **G5. Scoring in dollars plus hard safety failures.** Scoring is plain code driven by a `costs.yaml`, compared
  with a do-nothing run.
- **G6. One acceptance scenario.** "Thunderstorm at the hub" runs end to end in a test: a ground delay program at
  one hub, forkable at 16:00, with repeatable results across reruns.

**Non-goals for project 1:**
- Systems of record (project 2).
- Any UI or view work (project 3).
- Cloud and git import (project 4).
- Individual pilots and union rules. Crew is modelled per pairing (one crew unit per pairing), because real
  rosters are not public.
- ATC trajectories or BlueSky integration (optional later).
- Passenger booking records. Passengers are modelled as groups per itinerary.

**Anti-goals (a failure even if it works):**
- Scripted or fake agents anywhere client-facing (owner rule, 2026-11-21).
- Domain data in code or tests: tail numbers, hubs and rule numbers belong in data files (owner rule).
- Leaking the future to agents. The old branch's `est_dep` already included each flight's future sampled delay.
- Runs that don't repeat. A fork must see the same outside events as its parent.

## 3. Constraints

- **People inside the world (owner decision (a), 2026-11-22).**
  - Anyone an agent must persuade or wait on is a real LLM role: the duty manager, the VP who approves spending, a
    union rep. These are agent-setup roles and are wired in project 4.
  - Rates and queues are seeded statistical processes, labelled as such wherever they show: sick calls, crew phone
    queue throughput, passengers accepting a rebooking, mechanic throughput.
- **Agents call solvers rather than hand-optimizing** (owner rule, 2026-11-10). Solvers arrive in project 2. The
  driver's commands are the primitives the solvers' plans compile into.
- **Worlds, scenarios and experiments are YAML files** in a project folder (owner rule, 2026-11-20).
- **The part protocol's clock contract and determinism contract hold** (`docs/protocol.md`): fixed internal steps,
  and a state that does not depend on how advances were split.
- **Shared machine:** `CARGO_BUILD_JOBS=2`, one test worker, and the test DB env set in the same command
  (`SHAKEDOWN_PG_URL` to :15433).

## 4. Approaches considered

Project-level options, discussed with the owner on 2026-11-22:
1. **The airline lives in its own repo as Docker Compose services, imported by Shakedown.** This is the true
   customer story. It needs a bridge for messages between the services and the world, plus world time inside
   containers.
2. **Everything is a native Shakedown world**, as on the old `airline-engines` branch. This is the fastest and fully
   repeatable, but it shows no "import your system".
3. **Picked by the owner: native clock and disruptions, with the systems imported from a repo.**

**Refinement found during recon (my call; flag if wrong).** The `service` integration runs Compose projects but has
no tools, and parts never call each other: links connect them through tools. So in project 2 the airline's systems
of record will be written as Shakedown parts with the Python kit, living in the airline repo:
- Each part also serves a real REST API through `http_routes` and speaks MVT and ASM message formats, so agents and
  customers' code use it like a real service.
- The driver and the systems exchange events and commands through ordinary `links`.
- The generic Compose import path is still shown with ERPNext (maintenance parts and purchasing), which agents use
  directly.

This keeps exact forks and avoids building a message bridge now.

## 5. Design

### 5.1 Where the code lives

**Shakedown repo, branch `airline-world`:**
- the stepped clock (G1)
- world-time scenario events with `known_at` (G2)
- docs and tests for both

**New repo, `airline-world` (name is my call), local only until the owner says to publish:**
```
airline-world/
  parts/driver/          # the physical-airline part (Python, shakedown_part kit)
  packages/legality/     # rules engine, MIT licensed, usable outside Shakedown
  worlds/hub-day/        # the world folder: world.yaml, data/, rules/, costs.yaml, checks/
  scenarios/             # thunderstorm-at-the-hub.yaml
  data-pipeline/         # builds data/ from BTS, OurAirports, FAA registry (public domain sources)
  LICENSE (MIT), README.md, NOTICE (data sources and licences)
```
The driver, the world and the rules live with the airline, because they are the world's content. Shakedown owns
only what is generic.

### 5.2 Stepped clock (core)

A scenario opts in with `clock: {mode: stepped, …}` (5.3a). The default stays the current wall-time clock.

- **Clock movement.** World time stands still while agents think. Each agent tool call charges simulated time by
  its effect: `action_cost: {read: 0m, write: 2m}`, configurable, with a per-tool override such as solvers at 5m.
  So acting has a cost, but thinking speed does not.
- **When the team is quiet.** This means every agent in the dispatch runner is idle, using the runner's existing
  idle detection. shaked then advances the world to the next wake point:
  - the earliest wake a part asked for (a new optional field `wake_at_ms` in `advance` and `call_tool` results);
  - the next scheduled scenario event (5.3);
  - the epoch limit (see "What wakes agents" below).

  It then wakes the lead with the world events that happened as mail.
- **Runners (my call).** Project 1 supports the stepped clock in the dispatch runner only. Deploy picks the dispatch
  runner for a scenario with `clock.mode: stepped`. Chain, Concurrent and External runs refuse a stepped scenario
  with a clear message. External support comes later if a demo needs it.
- **What wakes agents (my call).**
  - Only events a part marks `alert` become mail to the lead. Movements and crew reports go to systems through
    links in project 2, not to agents.
  - When the team is quiet and nothing is pending, shaked advances straight to the next wake point without waking
    anyone. An advance that brings no alert wakes nobody.
  - `epoch` is a maximum gap that guarantees the lead a look at least that often, default 60m, and `epoch: none`
    turns it off.
  - In stepped mode, `max_idle_wakes` counts only wakes that brought no new alert and no new mail.
- **A run ends** at the scenario's `horizon`. If the lead finishes earlier, shaked keeps advancing the world, with no
  agents, to the horizon before scoring. Stopping early cannot hide costs that land later (my call).
- **Recording.** Every advance is recorded as an input. Replay and fork repeat them exactly. The clock contract
  makes the result independent of step size.
- **Agents' view of time.** Agents see world time in their inputs as today: the World clock section and the
  now-line. The text changes from "runs while you work" to "moves when the team acts or waits".

### 5.3 World-time scenario events (core plus kit)

The part schedules timed events; core passes them in and stops the clock at them. This is my call after the
builder check.

- **Delivery.** Scenario `events[]` may carry:
  - `at`: when the event happens in the world, as a world time `HH:MM` on the task's day or an ISO timestamp;
  - `known_at`: when the airline learns of it, which defaults to `at`.

  `known_at` may come before `at` (a forecast: the storm is forecast at 14:40 and arrives at 15:10). It may also
  come after (found out later: a defect is found at the next walk-around). Deploy forwards every such event, not
  only `fault` events, into the named part's setup call as `args.disruptions`, the same way scenario disruptions
  reach parts today. The event is then part of the part's saved state from the start.
- **Firing.** The part arms each event and fires it inside its own fixed-step `advance` (clock contract). It
  reports a world event at `known_at`: the forecast or the discovery, with only what is known. The effect takes hold
  at `at`. The part never reveals an event in a tool result or event before `known_at`; that rule goes into
  `docs/protocol.md`. The kit helper `shakedown_part.disruptions` gets an `at` and `known_at` variant beside
  `after_calls`, which keeps working.
- **Clock stops.** The part reports its next reveal or effect time as `wake_at_ms` (5.2), so the stepped clock stops
  there.
- **Recording.** Recording, replay and fork need nothing new, because the events are part state and the advances
  are recorded inputs.
- **Generators.** A scenario may give background generators (sick-call rate, AOG rate, weather cells). The driver
  expands them deterministically from the seed into concrete timed events at start. This is part logic.

### 5.3a How the files reach a run (my call)

- **The airline repo ships a benchmark package.** It has `shakedown-package.toml` plus `describe` with tasks, setup,
  links and `clock_start`. That is today's working path from files to runs (`ops/deploy.rs`, `world.package`). The
  package reads its world data, rules and costs from the world folder.
- **Core scenario schema changes are small.** `shakedown-scenario` gains:
  - an optional `clock` block (`mode`, `action_cost`, `epoch`, `horizon`);
  - `known_at`, `part` and `do` on events;
  - `generators` as an opaque list passed to the part.

  The open-source subset schema gets the same fields. `world` keeps `package:`.
- **`clock` lives in the scenario only.** A world does not set it, so one world can run under both clocks.

### 5.4 The driver part

**State, in a jsonstate-style save.** All input data is loaded from the world folder. Nothing is hardcoded.
- **Stations:** coordinates, curfew, minimum turn per aircraft family, capacity per hour.
- **Tails:** location, ready time, family, hours and cycles since check, open MEL items with expiry and
  restrictions.
- **Flights:** scheduled times, estimated times, actual OOOI times (out, off, on, in), tail, crew unit, status, and
  delay minutes by IATA delay code.
- **Crew units:** pairing, base, duty start, flight-duty-period (FDP) limit from the legality package, flight time,
  last rest, location, and status (on duty, resting, reserve, sick).
- **Passenger groups:** itinerary by connection, count, booking class tier (for compensation).
- **The truth tape:** every outside event with its `happens_at` and `known_at`.

**Propagation.** In one-minute internal steps (meeting the clock contract), a flight departs at the latest of:
- scheduled departure plus its sampled ground delay;
- the tail's ready time plus the minimum turn;
- the crew's ready time plus the minimum connection;
- its ATC slot under a ground delay program.

Block time is sampled per flight. Knock-on delay is never sampled directly; it emerges from the chain.

**Randomness.** Each random draw uses a counter-based hash of (seed, stream, entity, day), the way the old
`delays.py` does it. Every fork therefore sees the same outside world, and differences between forks come only from
decisions. Stream names live in the code; their parameters live in data.

**No future leak.** Read tools show only what is known at the current time. Estimates come from the airline's own
forecast, which is the schedule plus known delays, never from the truth tape.

**Events out.** These are what real systems would receive. In project 2 they become links into those systems.
- `movement`: MVT-like, with off-blocks, airborne, landed, in-blocks, estimates and delay codes.
- `crew_report`: a check-in, a sick call, or a crew unit about to time out.
- `defect`: a new MEL item or an aircraft grounded for parts.
- `capacity_program`: a ground delay program start or change, slots and rates.
- `alert`: a wake point for the stepped clock. It fires on a delay over the threshold, a projected crew legality
  breach, an aircraft grounded for parts, a curfew risk, or a misconnect group over size.

**Commands in.** These are the world's own tools. In project 1 they are called directly by tests, and in project 2
by links from the systems of record:
- `apply_schedule_change {swap|delay|cancel|ferry}`
- `assign_crew {flight, crew_unit | reserve}`
- `extend_duty {crew_unit, minutes}`, which follows FAR 117.19 limits
- `release_aircraft {tail}`, after a fix or deferral
- `rebook_group {group, flights}`

Each command is checked for feasibility (location continuity, curfew, MEL restrictions). It is refused with a reason
when impossible, and accepted but flagged when it creates a legality or cost problem. This is the "refusal-style
rules" pattern ported from the old part.

**Read tools,** for tests now and the systems later: a summary, flight, tail, crew unit, passenger group, and
`kpis`.

### 5.5 Legality package

- **Rules are YAML tables.** Example: `rules/far117.yaml` holds Table B indexed by acclimated report time and number
  of segments, plus the cumulative windows (60 hours of FDP per 168 hours, 190 per 672, and so on) and the rest
  rules.
- **Six rule kinds:** table lookup, rolling-window sum, minimum gap, extension counter, maintenance interval
  (hours, cycles, days), and MEL expiry by category.
- **Three severities:** `regulatory` (a hard failure), `contract` (a pay penalty), `company` (soft).
- **API:** `check(crew_unit_history, proposed_duty, rules) -> [Violation{rule_id, severity, limit, actual, text}]`,
  and `maintenance_due(tail, rules, now)`.
- **Sources.** Every number in a table cites its source in a comment (eCFR or Cornell LII, EASA). Values that are
  unverified are marked as such: Part 117 Table C, split duty, and EASA standby limits were not fetched in research
  and are left out of v1.

### 5.6 Scoring

`worlds/hub-day/costs.yaml` holds:
- A4A's cost per block minute ($100.76 for 2024);
- passenger time at $47 an hour;
- a cost per cancellation;
- the US DOT refund triggers (3 hours domestic, 6 international);
- EU261 tiers, which apply only to flights in scope;
- crew overtime;
- an end-of-horizon penalty for each tail and crew unit out of position.

**The scorer** is plain Python over the final driver state. It reports total cost, on-time performance (A15 and D0),
completion factor, misconnects and legality breaches.

**Hard failures are safety checks (my call).** They do not stop the run. They are:
- a flight that departs with a crew unit illegal under a `regulatory` rule;
- a tail flying past a maintenance or MEL limit;
- a curfew breach.

The driver records each one as an `alert` world event at the moment it happens, so the timeline shows exactly when.
It also counts them in `kpis`. A scenario safety check on those counts fails the run at scoring. No new mid-run
stop signal is needed.

**The do-nothing baseline lives inside the driver (my call).** At start, the driver simulates a shadow copy of the
world to the horizon:
- the same seed and the same truth tape;
- no commands.

It stores the baseline KPIs, so `cost_vs_baseline` is available to checks. This needs no core baseline feature.

### 5.7 Data

- **The default world is "hub day":** one airline's day at one hub bank structure. It is built from BTS On-Time
  Performance and T-100 for a chosen carrier and date, plus OurAirports stations and FAA registry tails. All three
  are public domain or the Unlicense, so they are safe for client-facing use.
- **Crew pairings are synthetic.** A deterministic generator builds pairings over the day's rotations, respecting
  the legality tables, and labels them synthetic.
- **The old pipeline** (`benchmarks/airline-engines/data` on the old branch) is ported where it fits. The carrier
  and date are config in the world folder.
- **Size target:** about 150 to 300 flights, so runs and forks stay fast on this machine. The full 21k-flight week
  stays possible but is not the default.

### 5.8 Acceptance scenario: "Thunderstorm at the hub"

```yaml
world: hub-day
task: recover-the-bank
clock: {mode: stepped, action_cost: {read: 0m, write: 2m}, epoch: 60m, horizon: "23:59"}
events:
  - name: Thunderstorm cells approach the hub
    at: "15:10"
    known_at: "14:40"          # forecast arrives 30 minutes ahead
    part: driver
    do: {kind: capacity_program, station: "{hub}", arrival_rate: 24, until: "19:30"}
generators:
  - {kind: sick_calls, rate_per_100_duties: 1.5}
checks:
  - {text: No crew flies illegal, kind: safety, query: {part: driver, key: kpis, field: legality_regulatory, op: eq, value: 0}}
  - {text: Cost below do-nothing, kind: goal, query: {part: driver, key: kpis, field: cost_vs_baseline, op: lt, value: 1.0}}
```
`{hub}` and the numbers come from the world's data and the scenario file, not code.

**Test:**
1. Run the do-nothing baseline twice with the same seed and confirm the states are identical.
2. Fork at 16:00, apply "cancel 15% of the remaining hub flights" on one branch, and confirm the branches differ
   only downstream of the decision.
3. A replay reproduces both branches.

## 6. What carries over from the old branch

| Old piece | Use |
|---|---|
| `integrations/airline-ops` event loop (`sim.py`), saves (`state.py`), seeded delays (`delays.py`) | Ported into `parts/driver`, with crew, passengers, `known_at` and MEL added |
| Feasibility rules (`rules.py`, `rules.json`) | Moved into data files plus the legality package |
| Recovery MILP (`optimize.py`, HiGHS) | Moves to the solver service in project 2 |
| Data pipeline (`benchmarks/airline-engines/data`) | Ported into `data-pipeline/` |
| Hand-coded `view.py`, `LiveMap.tsx`, `Timeline.tsx` | Reference only for project 3; not merged (owner's no-hand-coded-views rule) |
| `ops-desk` rule-based duty manager and VP | Dropped. These roles become real LLM agents under decision (a) |

## 7. Left to the builder

- Exact YAML field names beyond those shown, and internal module layout.
- How `wake_at_ms` is merged across several parts (earliest wins is expected).
- The delay-distribution fitting method from BTS, as long as it is per cause, per station and per hour, and
  documented.
- The pairing generator's heuristics, as long as its output passes the legality package.
- Test structure, following repo conventions.
