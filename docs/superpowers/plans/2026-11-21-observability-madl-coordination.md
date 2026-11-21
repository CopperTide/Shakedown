# Observability, MADL and Coordination Implementation Plan (6)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:**
- A world declares its observability and on-call: the tools, alert rules, simulated operators, and what counts as needing a human.
- A setup described in MADL is validated and drawn.
- A run shows its "Needs a human" moments and how its agents coordinated.

**Architecture:**
- The world file gains an `observability` section, edited in a new World section.
- An operation `setups.madl` (Read) loads `setups/<id>/team.madl.yaml` with the `madl` crate (a git dependency pinned to the owner's public repository). It answers MADL's diagnostics (code, JSON pointer, message) and a graph of agents and connections. The setup editor shows them for MADL teams.
- The run page gains two derived views, both pure functions over the run's recorded events:
  - "Needs a human": refusals that wait on approval, and messages to people;
  - "Coordination": the agents and the messages, handoffs and delegations between them.

**Tech Stack:** Rust (shakedown-project, shaked, `madl` at rev `3b5b7c91a4fcc5b7648b0c5b7b91dcb32a86b723`), React UI.

**Spec:** `docs/superpowers/specs/2026-11-20-primitives-and-editors-design.md` §14 (setups and MADL: visualize, coordinate). The observability and on-call design comes from the 2026-11-21 owner request recorded in the mockups (World → Observability & on-call; run "Needs a human").

## Global Constraints

- The code-quality rules of plans 1 and 2 hold.
- **MADL is optional:** any team works, and a MADL team gets the graph and checks. Use the `madl` crate; don't re-implement the spec.
- **Observability tools:** each is real (it receives the sandbox's telemetry, tagged with the run id) or a stand-in. Credentials are key-store names only.
- **Derived views** read recorded events only and invent nothing. When a run has nothing to show, they say so plainly.

## Review Focus

1. **A MADL setup whose file is missing.** The answer is 422 "team.madl.yaml is missing from the setup's folder", not a crash (Task 2 test `missing_madl_file`).
2. **A MADL file with errors.** The diagnostics are listed with code and pointer, and the graph is still drawn from what parsed (Task 2 test `invalid_madl_lists_codes`).
3. **A non-MADL setup asked for MADL.** 422 "this setup isn't described in MADL" (Task 2 test `not_madl`).
4. **A run with no messages between agents** shows "Agents didn't message each other in this run" (Task 3 test `single_agent_coordination`).
5. **An unknown operator behavior** in a world file is refused on parse with its pointer (Task 1 test `bad_operator_behavior`).

---

### Task 1: Observability and on-call in the world

**Files:**
- Modify: `core/crates/project/src/format/world.rs`, adding `observability: Option<Observability>`:
  - `Observability { tools: Vec<ObsTool>, alerts: Vec<Alert>, operators: Vec<Operator>, human: Vec<HumanRule> }`
  - `ObsTool { id, product, mode: real|stand-in, seeded_from?, auth? }`
  - `Alert { name, when, routes_to? }`
  - `Operator { id, label, role?, behavior }`, where `behavior` is one of `answers_after: {minutes}`, `approves`, `refuses` or `never_answers`
  - `HumanRule { moment, then }`, where `moment` is `paged | approval | gate_needs_person | agent_stuck | message_to_person` and `then` is `continue | pause | fail`
- Create: `ui/src/features/editors/world/ObservabilitySection.tsx`, and add the section to `WorldEditor`
- Modify: `ui/src/model/project.ts`
- Test: `core/crates/project/tests/formats.rs` (`observability_round_trips`, `bad_operator_behavior`), `ui/src/features/editors/world/world.test.tsx` (`add_operator_never_answers`, `human_rule_choices`)

- [ ] **Step 1:** Write the failing tests.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the project crate tests and the UI tests, plus lint and build. Expected: PASS.
- [ ] **Step 5: Commit:** `feat: observability and on-call in the world`

### Task 2: MADL setups validated and drawn

**Files:**
- Modify: `core/Cargo.toml` (workspace dependency `madl = { git = "https://github.com/CopperTide/Multi-Agent-Descriptor-Language", rev = "3b5b7c91a4fcc5b7648b0c5b7b91dcb32a86b723" }`), `core/crates/shaked/Cargo.toml`
- Create: `core/crates/shaked/src/ops/madl_ops.rs`, with `setups.madl` at `GET /api/project/setups/{id}/madl`, Read. It answers:
  - `{valid, diagnostics: [{code, severity, pointer, message}], graph: {agents: [{id, label, runtime?}], edges: [{from, to, kind}]}}`
- Create: `ui/src/features/editors/setup/MadlSection.tsx` (validation list plus an SVG graph laid out in columns: starting agents left, by reachability) and `setup/graph.ts` (`layout(graph)`, pure)
- Test: `core/crates/shaked/tests/madl.rs` (`valid_madl_graph`, `invalid_madl_lists_codes`, `missing_madl_file`, `not_madl`), `ui/src/features/editors/setup/setup.test.tsx` (`madl_section_lists_diagnostics`, `layout_columns`)

- [ ] **Step 1:** Write the failing tests. The fixtures are MADL files copied from the MADL repo's `examples/` (pipeline, and debate with an error added).
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests, clippy, `npm test`, lint and build. Expected: PASS.
- [ ] **Step 5: Commit:** `feat: MADL setups validated and drawn`

### Task 3: Needs a human, and coordination, on the run page

**Files:**
- Create: `ui/src/features/run/derived/humans.ts` (`needsAHuman(events): Moment[]`), `derived/coordination.ts` (`coordination(events): {agents, links: {from, to, messages, handoffs, delegations}[]}`), `derived/NeedsAHuman.tsx` and `derived/Coordination.tsx`
- Modify: `ui/src/features/run/RunPage.tsx` (a "Needs a human" card above the timeline when there are moments, and a Coordination tab)
- Test: `ui/src/features/run/derived/derived.test.ts` (`refusal_needing_approval_is_a_moment`, `message_to_person_is_a_moment`, `coordination_counts_pairs`, `single_agent_coordination`)

- [ ] **Step 1:** Write the failing tests over event fixtures shaped like `model/types.ts` events.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `npm test`, lint and build. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(ui): needs a human and coordination on the run page`
