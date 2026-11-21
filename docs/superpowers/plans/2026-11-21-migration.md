# Migration Implementation Plan (7)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Existing work moves into the project folder in one click.
- Installed benchmarks and built worlds become worlds that run on their package.
- Each earlier experiment becomes a scenario, its setups and an experiment file, and the old record becomes that file's first deployment.

**Architecture:**
- A pure planner (`ops/migrate.rs`) maps installed benchmarks plus the unlinked engine experiments and their suites to the files to write, the links to make and what it can't carry.
- The `project.import` operation (Edit, `POST /api/project/import`) answers the plan without writing. With `confirm: true` it writes only files that don't exist yet and links each old experiment.
- The UI offers "Move into the project" on the Earlier experiments card.

**Tech Stack:** Rust (shaked, shakedown-project), React UI.

**Spec:** `docs/superpowers/specs/2026-11-20-primitives-and-editors-design.md` §12 (migration): built worlds and coded benchmarks become worlds with their package (still read-only packages); teams and Code connections become setups; earlier experiments keep their runs.

## Global Constraints

- The code-quality rules of plans 1 and 2 hold.
- **Never overwrite.** An item that already exists is left alone, and the plan says so.
- **Never lose runs.** Old experiments stay in the database. Linking only sets their `project_experiment` when it is empty.
- **No secrets:** an agent's provider becomes the key-store name. HTTP headers aren't carried; the plan notes them.
- Tests use the toy benchmark only.

## Review Focus

1. **Running import twice.** The second time it proposes nothing new and links nothing twice (Task 2 test `import_is_idempotent`).
2. **A Code-connection setup** (`external` agents) is skipped with "Your code setups need their command; add them as code setups" (Task 1 test `code_setup_skipped`).
3. **Two old experiments with the same name** get distinct experiment ids (Task 1 test `same_names_get_distinct_ids`).
4. **A team mixing built-in and Claude Code agents** is skipped with a note, not mapped wrongly (Task 1 test `mixed_team_skipped`).
5. **An existing world with the benchmark's id** is reused, not replaced (Task 1 test `existing_world_reused`).

---

### Task 1: The migration planner

**Files:**
- Create: `core/crates/shaked/src/ops/migrate.rs`, with `plan(project: &Project, benchmarks: &[(String, String)], old: &[(Experiment, Vec<Suite>)]) -> Plan` where `Plan { files: Vec<(Kind, Id, ItemBody)>, links: Vec<(String /*db id*/, Id)>, skipped: Vec<String> }`
- Test: unit tests in `ops/migrate/tests.rs`: `benchmarks_become_worlds`, `experiment_becomes_files_and_link`, `code_setup_skipped`, `mixed_team_skipped`, `same_names_get_distinct_ids`, `existing_world_reused`

**Mapping:**
- **Worlds:** a benchmark `b` with title `t` becomes world `b` with `{label: t, package: b}`.
- **Scenarios:** `<bench>-<task>` with `{world: bench, preset: task, task: "Task <task> of <bench>"}`.
- **Setups:** `<team id>` from the suite's team:
  - all agents `open` → built-in agents with `{id, role: prompt, model: model or "default", key: provider or "default", tools}`;
  - a single `claude_code`, `codex` or `http` agent → that team kind;
  - otherwise skipped with a note.
- **Experiments:** id from the name, made unique with the old id's first 6 characters, with `{world, scenario, setups, repeats: {per_setup: runs}}`.

- [ ] **Step 1:** Write the failing tests.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run them, then clippy. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): plan the move of earlier work into the project`

### Task 2: `project.import`

**Files:**
- Modify: `core/crates/shaked/src/store/experiments.rs` (`unlinked_experiments() -> Vec<String>` and `link_deployment(id, project_experiment)`, which updates only when the column is NULL), `ops/project_ops.rs` or a new `ops/import_ops.rs` (`project.import`)
- Test: `core/crates/shaked/tests/migrate.rs` (`import_proposes_then_writes_and_links`, `import_is_idempotent`)

- [ ] **Step 1:** Write the failing tests on the toy world: store an experiment as `tests/deploy.rs` does, import with and without confirm, check the files and `deployments_of`, then import again.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement. Benchmarks come from the hub registry, as `/api/worlds` lists them.
- [ ] **Step 4:** Run `--test migrate --test ops --test parity --test routes` and clippy. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): move earlier work into the project`

### Task 3: Move into the project, in the UI

**Files:**
- Modify: `ui/src/api/project.ts` (`importProject(confirm)`), and `ui/src/features/experiments/EarlierExperiments.tsx` (a "Move into the project" button opening a dialog that lists what will be written and what is skipped; Confirm writes; the list then refreshes)
- Test: `ui/src/features/experiments/list.test.tsx` (`move_into_project_confirms`)

- [ ] **Step 1:** Write the failing test.
- [ ] **Step 2:** Run it. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `npm test`, lint and build. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(ui): move earlier experiments into the project`
