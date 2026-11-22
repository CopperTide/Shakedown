# Airline World Driver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a stepped world clock and world-time scenario events in Shakedown core, plus a deterministic
simulated airline (driver part, legality package, hub-day world, scoring) in a new `airline-world` repo. "Thunderstorm
at the hub" must run end to end, repeatably and forkably.

**Architecture:**
- **Core changes are generic.**
  - A `stepped` mode of the branch clock that moves only on agent actions and on quiet-team advances.
  - A `wake_at_ms` hint from parts.
  - Deploy forwards timed scenario events into parts' setup.
- **The airline lives in a separate repo** as a Python `shakedown_part` benchmark package. It holds the driver part
  (the physical airline), an MIT legality package, the world folder (data, rules, costs) and scenarios.

**Tech Stack:** Rust (shaked, `parts` crate, `shakedown-project` crate), Python 3.12 with `uv`, the
`shakedown_part` kit, PyYAML, numpy only if needed (avoid otherwise). The data sources are BTS On-Time, OurAirports
and the FAA registry, already downloaded at `~/MLSys/CopperTide/work-local/airline-data/`.

**Spec:** `docs/superpowers/specs/2026-11-22-airline-world-driver-design.md` (this worktree). Read it before any task.
Research numbers are in `~/MLSys/CopperTide/research_notes/Airline operations simulation build/` (crew rules:
`occ_operations_and_rules.md`; costs: `disruption_case_studies.md`).

## Global Constraints

- **No domain data in code or tests.** Tail numbers, hubs, stations, rule numbers and costs live in YAML or JSON
  files in the world folder. Tests load fixtures from files.
- **No scripted or fake agents in anything client-facing.** Fake models are fine in unit tests only.
- **Parts meet the clock contract.** A part's state after `advance` to t does not depend on how the advances were
  split. Internal steps are fixed at 1 world-minute (60_000 ms).
- **Same seed and same inputs give the same state.** Randomness is a counter-based hash of
  `(seed, stream, entity, day)` (blake2b or splitmix64). Never use global RNG state.
- **No read tool or event reveals a scheduled event before its `known_at`.**
- **Shared machine limits:**
  - `CARGO_BUILD_JOBS=2`, and no debug info for test builds.
  - Rust tests take `SHAKEDOWN_PG_URL=postgres://...:15433/...` in the same command (copy it from repo `docs/` or
    the existing test scripts).
  - pytest runs with one worker.
- **Pin bumps update both** the root and `crates/` `Cargo.lock`, and builds pass `--locked`.
- **The airline repo is created locally** at `~/MLSys/CopperTide/airline-world` (git init, MIT licence). It is not
  pushed to GitHub without the owner's go.
- **Docs move with code.** `docs/protocol.md` and the Shakedown docs describe every user-visible core change in the
  same task.

## Review Focus

1. **A fork taken mid-epoch, between advances,** must restore the exact same driver state and the same pending
   truth-tape events (Task 12 test `fork_mid_epoch_matches_parent`).
2. **A scenario event whose `known_at` is after `at`** (found out late) must apply its effect at `at` and still not
   appear in reads until `known_at` (Task 9 test `late_discovery_hidden_until_known`).
3. **A command for a flight that has already departed, or a tail at another station,** is refused with a reason and
   changes no state (Task 10 test `refuses_departed_and_wrong_station`).
4. **A lead that finishes at 15:00 before a 15:10 storm** gets scored after the world runs to the horizon, so the
   costs count (Task 4 test `finish_early_runs_to_horizon`).
5. **A quiet stepped run with no alerts** advances to the horizon without waking anyone and without tripping
   `max_idle_wakes` (Task 4 test `quiet_world_runs_to_horizon_without_wakes`).

---

## Part A: Shakedown core (worktree `.worktrees/airline-world`, branch `airline-world`)

### Task 1: Scenario schema gains clock, timed-event and generator fields

**Files:**
- Modify: `core/crates/project/src/format/scenario.rs`
- Modify: the open-source subset schema, i.e. the JSON Schema or validator that mirrors `ScenarioFile`. Find it with
  `grep -rn "shakedown-scenario" core docs`.
- Test: `core/crates/project/tests/` (follow the existing scenario format tests)

**Interfaces:**
- Produces:
  - `ScenarioFile.clock: Option<ClockSpec>`
  - `ClockSpec { mode: ClockMode /* Wall | Stepped */, action_cost: ActionCost, epoch: Option<String> /* "60m" | "none" */, horizon: Option<String> }`
  - `ActionCost { read: String, write: String, tools: BTreeMap<String,String> }`. Durations are strings like `"2m"`
    and are parsed by `parse_duration_ms(&str) -> Result<i64>`. Defaults: read `0m`, write `2m`, epoch `60m`.
  - `Event` gains `known_at: Option<String>`, `part: Option<String>`, `do_: Option<Value>` (serde rename `do`).
  - `ScenarioFile.generators: Vec<Value>`

- [ ] Write the failing tests:
  - `parses_stepped_clock_and_timed_events`: YAML with the spec §5.8 scenario round-trips and gives
    `epoch == Some("60m")`.
  - `rejects_bad_duration`: `write: "2 parsecs"` fails with a message naming the field.
  - `wall_is_default`: no `clock` gives `None`.
- [ ] Run `cargo test -p shakedown-project` and confirm the new tests fail.
- [ ] Implement the fields with `#[serde(default)]`, keeping `deny_unknown_fields`. Mirror them in the subset schema.
- [ ] Run the tests and confirm they pass.
- [ ] Commit: `feat(project): scenario clock block, known_at and generators`

### Task 2: Protocol: `wake_at_ms` and the known-at rule

**Files:**
- Modify: `parts` crate proto (where `Advanced` and the `call_tool` result types are; `grep -rn "pub struct Advanced" core`)
- Modify: `docs/protocol.md` (the `advance`/`call_tool` table, "Scheduled disruptions", plus a new "Stepped clock"
  section)
- Modify: the Rust kit helper if it builds `Advanced`.
- Test: the proto serde tests in the `parts` crate.

**Interfaces:**
- Produces: `Advanced.wake_at_ms: Option<i64>` and the call result's `wake_at_ms: Option<i64>`. Both are optional,
  additive, and `skip_serializing_if = "Option::is_none"`. Meaning: the earliest world clock at which this part has
  something scheduled to reveal or apply.

- [ ] Write the failing test `advanced_carries_wake_at`: JSON `{"changed":false,"events":[],"wake_at_ms":5400000}`
  parses, and old JSON without the field still parses.
- [ ] Implement the change. In docs, state the rule: "A part must not reveal a scheduled event in any tool result or
  event before its `known_at`; it reports `wake_at_ms` for its next reveal or effect."
- [ ] Run `cargo test -p parts` and confirm it passes.
- [ ] Commit: `feat(protocol): parts report wake_at_ms; known_at rule`

### Task 3: Stepped branch clock

**Files:**
- Modify: `core/crates/shaked/src/clock.rs`, `core/crates/shaked/src/ticker.rs`, `core/crates/shaked/src/exec.rs`
  (`execute`)
- Modify: `core/crates/shaked/src/world_clock.rs` (brief wording)
- Test: unit tests in `clock.rs`; an exec-level test using the existing test kit (`testkit.rs`)

**Interfaces:**
- Produces:
  - `BranchClock::stepped(base_ms: i64, cost: StepCost) -> BranchClock`
  - `BranchClock::is_stepped(&self) -> bool`
  - `BranchClock::charge(&self, effect: Effect, tool: &str)`, which adds the cost in ms
  - `BranchClock::step_to(&self, ms: i64)`, which never moves backwards
  - `StepCost { read_ms: i64, write_ms: i64, tools: HashMap<String,i64> }`
  - `now_ms()` in stepped mode returns the counted ms only, with no wall time.
- Consumes: Task 1 `ClockSpec`.

- [ ] Write the failing tests:
  - `stepped_clock_ignores_wall_time`: `now_ms` is unchanged after a 50 ms sleep.
  - `charge_by_effect_and_tool_override`: read adds 0, write adds 120_000, `plan_recovery` override adds 300_000.
  - `step_to_never_goes_back`.
- [ ] Write the failing exec test `tool_call_charges_world_minutes`: a write tool call in a stepped branch moves the
  clock by `write_ms` after the call, and the call itself runs at the pre-charge clock.
- [ ] Implement:
  - The ticker is not started for stepped branches (`Ticker::start` returns None).
  - `execute` charges after a successful or refused call.
  - The World clock brief says the clock "moves when the team acts or waits" in stepped mode.
- [ ] Run `CARGO_BUILD_JOBS=2 SHAKEDOWN_PG_URL=… cargo test -p shaked clock` and the exec test, and confirm they pass.
- [ ] Commit: `feat(shaked): stepped branch clock`

### Task 4: Dispatch runner advances a quiet stepped world

**Files:**
- Modify: `core/crates/shaked/src/dispatch/mod.rs`, `core/crates/shaked/src/dispatch/board.rs`,
  `core/crates/shaked/src/world/clocked.rs` (return each part's `wake_at_ms`)
- Test: dispatch tests (follow `dispatch/resume.rs` test style, with fake models allowed)

**Interfaces:**
- Consumes: Task 2 `wake_at_ms`; Task 3 `step_to`, `is_stepped`; the scenario `epoch` and `horizon` as ms (Task 5
  passes them into the run request; until then, test via the request fields).
- Produces:
  - `World::advance_all` returns `(Vec<PartEvent>, Option<i64> /* min wake */)`.
  - Dispatch quiet handling: when every agent is idle and the branch is stepped, compute
    `next = min(min_wake, last_lead_look + epoch, horizon)`, then `step_to(next)` and `advance_all(next)`.
    - Any events of kind `alert` become mail to the lead, who is woken.
    - With none, it loops without waking.
    - At `next == horizon` the run ends.
  - After the lead calls `finish` in a stepped branch, keep advancing (no agents) through each wake to the horizon,
    then end.
  - `max_idle_wakes` counts only wakes that delivered no alert and no mail.

- [ ] Write the failing tests:
  - `quiet_world_runs_to_horizon_without_wakes`
  - `alert_wakes_lead_with_mail`
  - `finish_early_runs_to_horizon` (the clock ends at the horizon and the advances are recorded)
  - `epoch_gives_lead_a_look` (no alerts, epoch 60m gives one wake per 60 world-minutes)
- [ ] Implement.
- [ ] Run the dispatch tests (same env as Task 3) and confirm they pass.
- [ ] Commit: `feat(dispatch): quiet stepped worlds advance to the next wake`

### Task 5: Deploy forwards timed events and picks dispatch for stepped scenarios

**Files:**
- Modify: `core/crates/shaked/src/ops/deploy.rs`; run request types (`NewExperimentReq`/`SetupReq`, which carry the
  clock spec into the branch)
- Modify: the Chain, Concurrent and External runners, which refuse a stepped clock
- Test: deploy tests beside `deploy.rs` with the `larkspur` fixture style

**Interfaces:**
- Consumes: Task 1 fields; Task 3 `BranchClock::stepped`.
- Produces:
  - Every scenario event with `part` and (`at` or `known_at`) is appended to that part's setup call
    `args.disruptions` as `{id: slug(name), at: iso, known_at: iso, do: <do_>, text: name}`.
  - `generators` go to every part's setup as `args.generators`.
  - `HH:MM` resolves against the task's `clock_start` day.
  - The `clock` spec reaches branch creation, so a stepped branch gets `BranchClock::stepped`.
  - `Runner::Dispatch` is chosen when stepped.
  - Other runners given a stepped clock fail with: "stepped clock needs a team with a lead (dispatch runner)".

- [ ] Write the failing tests:
  - `timed_events_reach_part_setup`
  - `hhmm_resolves_on_clock_start_day`
  - `stepped_picks_dispatch`
  - `chain_refuses_stepped`
- [ ] Implement.
- [ ] Run the tests and confirm they pass.
- [ ] Commit: `feat(deploy): timed scenario events and stepped clock reach the run`

### Task 6: Python kit: timed disruptions with known_at and wake hints

**Files:**
- Modify: `kits/python/shakedown_part/disruptions.py`, `kits/python/shakedown_part/jsonstate.py` (an `advance`
  hook that returns `wake_at_ms`)
- Test: `kits/python/tests/` (the existing kit tests)

**Interfaces:**
- Produces:
  - `disruptions.arm_timed(config, state, kinds, given: list[dict]) -> None`, which stores pending timed
    disruptions with `at_ms` and `known_ms`.
  - `disruptions.due(state, clock_ms) -> tuple[list[reveal], list[effect]]`
  - `disruptions.next_wake(state) -> int | None`
  - `JsonStatePart.advance` calls `on_minute(inst, clock_ms)` per 60_000 ms step and returns `wake_at_ms`.
  - A reveal emits event kind `alert` (with `data.known_only: true` when `known_at < at`, a forecast). An effect
    emits kind `disruption`.

- [ ] Write the failing tests:
  - `forecast_reveals_before_effect`
  - `late_discovery_reveals_after_effect`
  - `split_advances_same_state` (advancing to 90 min in one call equals three 30-minute calls)
- [ ] Implement.
- [ ] Run `uv run pytest -p no:xdist kits/python/tests -q` and confirm it passes.
- [ ] Commit: `feat(kit): timed disruptions with known_at and wake hints`

## Part B: Airline repo (`~/MLSys/CopperTide/airline-world`, new)

### Task 7: Repo scaffold and the legality package

**Files:**
- Create: `LICENSE` (MIT), `README.md`, `NOTICE` (data sources and licences), `pyproject.toml` (uv workspace with
  members `packages/legality`, `parts/driver`, `data-pipeline`)
- Create: `packages/legality/legality/{__init__,rules,kinds,check}.py`
- Create: `worlds/hub-day/rules/{far117.yaml,fa121.yaml,maintenance.yaml,mel.yaml}`
- Test: `packages/legality/tests/test_far117.py`, `test_maintenance.py`

**Interfaces:**
- Produces:
  - `load_rules(paths: list[Path]) -> RuleSet`
  - `check(history: list[Duty], proposed: Duty, rules: RuleSet) -> list[Violation]`
  - `maintenance_due(tail: TailCounters, rules: RuleSet, now_ms: int) -> list[Violation]`
  - `Duty{start_ms, end_ms, report_local_hhmm, segments, flight_min, acclimated: bool}`
  - `Violation{rule_id, severity /* regulatory|contract|company */, limit, actual, text}`
  - `TailCounters{fh, cycles, days_since_check, mel: list[MelItem]}`
- The six rule kinds are `table`, `rolling_sum`, `min_gap`, `extension_counter`, `interval`, `mel_expiry`.

- [ ] Write the failing tests. Every value comes from the YAML; each test reads the expected limit from the loaded
  file and builds the duty around it. No numbers are hardcoded:
  - `fdp_limit_from_table_b`: a duty at the Table B limit for its report time and segment count passes, and one
    minute over gives a `regulatory` violation.
  - `rolling_60_in_168`
  - `rest_10h_min_gap`
  - `extension_over_30m_once_per_30h_rest` (117.19)
  - `a_check_interval_by_fh_cycles_or_days`
  - `mel_category_b_expires_3_days`
- [ ] Fill the YAML from `occ_operations_and_rules.md`. Each table cites its source URL in a comment. Part 117
  Table C, split duty and EASA standby are left out with a `# not in v1` note.
- [ ] Implement.
- [ ] Run `uv run pytest -p no:xdist packages/legality -q` and confirm it passes.
- [ ] Commit (repo initial commit): `legality: FAR 117, 121.467, maintenance and MEL rules as data`

### Task 8: Data pipeline builds the hub-day world

**Files:**
- Create: `data-pipeline/airline_data/{bts,stations,fleet,pairings,groups,build}.py`, porting from
  `~/MLSys/CopperTide/Shakedown/.worktrees/airline-engines/benchmarks/airline-engines/data` where it fits
- Create: `worlds/hub-day/build.yaml` (carrier, date, hub, size cap 300 flights, raw data dir), and outputs
  `worlds/hub-day/data/{stations,tails,flights,pairings,groups,delays_by_cause}.json` with a `fingerprint.json`
- Create: `tests/fixtures/mini-world/` (about 20 flights and 6 tails, generated by the pipeline from a tiny raw slice,
  committed)
- Test: `data-pipeline/tests/test_build.py`

**Interfaces:**
- Produces:
  - `build(config: Path) -> Path`, which is deterministic: the same inputs give byte-identical outputs.
  - JSON shapes:
    - `flights[]{id, number, date, origin, dest, std, sta, tail, family}`
    - `tails[]{tail, family, base, fh, cycles, days_since_check}`
    - `stations[]{code, lat, lon, tz, curfew?, min_turn{family:min}, arr_rate_per_h}`
    - `pairings[]{id, base, legs[flight ids], synthetic: true}`
    - `groups[]{id, itinerary[flight ids], pax, tier}`
    - `delays_by_cause{station:{hour:{cause:[quantiles]}}}`
- Station minimum turns and arrival rates come from a data table in `build.yaml`, not code. Groups are synthetic
  from BTS T-100 loads plus hub connection banks, labelled synthetic.

- [ ] Write the failing tests:
  - `deterministic_build` (two builds give equal hashes)
  - `pairings_pass_legality` (every pairing passes Task 7 `check`)
  - `rotations_are_continuous` (each tail's legs chain by station)
- [ ] Implement.
- [ ] Build `worlds/hub-day`.
- [ ] Run the tests and confirm they pass.
- [ ] Commit: `data: hub-day world from BTS, OurAirports and FAA registry`

### Task 9: Driver part: the simulated airline

**Files:**
- Create: `parts/driver/driver/{part.py,state.py,sim.py,rand.py,delays.py,events.py,generators.py}`, plus
  `shakedown-package.toml`
- Test: `parts/driver/tests/test_sim.py`, `test_known_at.py`

**Interfaces:**
- Consumes:
  - Task 6 kit (`arm_timed`, `due`, `next_wake`, `on_minute`)
  - Task 7 `check`, `maintenance_due`
  - Task 8 JSON shapes
- Produces:
  - `DriverPart(JsonStatePart)` with `uses_clock: true`, `restore.exact: "save"`, and a fingerprint over the code and
    the world data files.
  - Config is `{world_dir}`.
  - Setup tool `driver_scenario {disruptions, generators}`.
  - Event kinds: `movement`, `crew_report`, `defect`, `capacity_program`, `alert`, each with
    `data{flight|tail|crew_unit|station, …}`.
  - `rand.draw(seed, stream, entity, day) -> float` in [0,1).
- The propagation rule, from spec §5.4:
  ```
  dep = max(std + ground_delay, tail_ready + min_turn, crew_ready + min_connect, slot)
  ```
  Block time is sampled. Knock-on delay is never sampled.
- Disruption kinds (`do.kind`): `capacity_program {station, arrival_rate, until}`,
  `aog {tail, station, fix_minutes}`, `sick_call {crew_unit}`, `mel_defect {tail, category, restriction}`.

- [ ] Write the failing tests:
  - `same_seed_same_day`
  - `delays_propagate_through_tail` (a delayed inbound delays the outbound by the turn shortfall)
  - `capacity_program_meters_arrivals`
  - `late_discovery_hidden_until_known` (Review Focus 2)
  - `forecast_alert_at_known_at`
  - `split_advance_invariant`
  - `no_estimate_uses_truth_tape` (a flight's `etd` before the delay is known equals std plus known delays only)
- [ ] Implement.
- [ ] Run `uv run pytest -p no:xdist parts/driver -q` and confirm it passes.
- [ ] Commit: `driver: simulated airline with crew, known_at and generators`

### Task 10: Driver commands, feasibility and read tools

**Files:**
- Create: `parts/driver/driver/{commands.py,feasibility.py,reads.py}`
- Test: `parts/driver/tests/test_commands.py`

**Interfaces:**
- Produces:
  - World-own write tools: `apply_schedule_change {kind: swap|delay|cancel|ferry, …}`,
    `assign_crew {flight, crew_unit|"reserve"}`, `extend_duty {crew_unit, minutes}`, `release_aircraft {tail}`,
    `rebook_group {group, flights}`.
  - Read tools: `summary`, `flight {id}`, `tail {tail}`, `crew_unit {id}`, `group {id}`, `kpis`.
  - Refusals use the kit `Denied`. A flag is an `alert` event plus `data.flags[]` in the result.

- [ ] Write the failing tests:
  - `refuses_departed_and_wrong_station` (Review Focus 3)
  - `swap_respects_family_and_mel_restriction`
  - `curfew_refusal`
  - `extend_duty_flags_over_117_19`
  - `illegal_assign_accepted_but_flagged_regulatory`
  - `reads_hide_unknown_events`
- [ ] Implement.
- [ ] Run the tests and confirm they pass.
- [ ] Commit: `driver: operational commands with feasibility and legality flags`

### Task 11: Scoring and the do-nothing baseline

**Files:**
- Create: `parts/driver/driver/{score.py,baseline.py}`, `worlds/hub-day/costs.yaml`
- Test: `parts/driver/tests/test_score.py`

**Interfaces:**
- Produces:
  - `score(state, costs) -> Kpis{total_cost_usd, otp_a15_pct, d0_pct, completion_pct, misconnect_pax, legality_regulatory, mx_overflown, curfew_breaches, cost_vs_baseline}`
  - `baseline.run(world, seed, tape) -> Kpis`, computed once at start and stored in state.
  - `costs.yaml` holds:
    - `block_minute_usd: 100.76` (A4A 2024)
    - `pax_hour_usd: 47`
    - `cancel_flight_usd`, labelled estimate
    - `refund_trigger_min {domestic: 180, international: 360}`
    - `eu261 {short: 250, medium: 400, long: 600}`, applying only when `eu261_scope`
    - `crew_overtime_hour_usd`, labelled estimate
    - `out_of_position_usd {tail, crew_unit}`, labelled estimate
  - Each value cites its source or says `estimate`.

- [ ] Write the failing tests:
  - `do_nothing_cost_vs_baseline_is_one`
  - `cancel_costs_and_refunds`
  - `hard_failures_counted_with_alert`
  - `out_of_position_penalty_at_horizon`
- [ ] Implement.
- [ ] Run the tests and confirm they pass.
- [ ] Commit: `driver: dollar scoring and in-part do-nothing baseline`

### Task 12: Benchmark package, world folder, thunderstorm scenario, acceptance

**Files:**
- Create: `benchmark/` (the package `describe`: task `recover-the-bank` with `clock_start` from the world data,
  setup calling `driver_scenario`, and `scored` checks)
- Create: `worlds/hub-day/world.yaml` (`shakedown-world/3`, `package: airline-world`, connectors labelled by
  category), `worlds/hub-day/checks/recovery.yaml`
- Create: `scenarios/thunderstorm-at-the-hub.yaml` (spec §5.8, with `{hub}` resolved from world data by the package)
- Create: `README.md` sections: "Run it locally", "What's open source here", "Data sources and licences"
- Test: `tests/test_acceptance.py`, which drives a local `shaked` through its HTTP API using the repo's existing
  test-launch helper. It is skipped with a clear reason if shaked isn't built.

**Interfaces:**
- Consumes: everything above.

- [ ] Write the failing tests:
  - `baseline_is_repeatable` (two do-nothing runs with the same seed give equal final driver state hashes)
  - `fork_at_1600_differs_only_downstream` (one branch cancels 15% of remaining hub departures through
    `apply_schedule_change`; the states are equal before the fork, the cost and OTP differ after)
  - `fork_mid_epoch_matches_parent` (Review Focus 1)
  - `replay_reproduces_both_branches`
- [ ] Implement the package and files.
- [ ] Run `uv run pytest -p no:xdist tests/test_acceptance.py -q` with shaked built from the airline-world worktree,
  and confirm it passes.
- [ ] Commit: `airline-world: hub-day world, thunderstorm scenario, acceptance tests`
