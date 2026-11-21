# Runtime Bridge and Deploy Implementation Plan (3a)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A file experiment can be deployed. Shakedown turns its world, scenario and setups into a real experiment on the existing engine, records the link, and the experiment's page shows its deployments and runs.

**Architecture:**
- A world file names the installed package (a benchmark or built world) that runs it: `package: factory`.
- A scenario names the package task it plays (`preset: t1`). Its events with a `fault` become the package's disruptions.
- `ops/deploy.rs` maps files to the existing `NewExperimentReq`. The shared creation path, extracted from the `POST /api/experiments` handler, stores and starts it with a new `project_experiment` column.
- Operations gain the caller: `OpCtx.caller`.

**Tech Stack:** Rust (shaked, shakedown-project, shakedown-schema), React UI.

**Spec:** `docs/superpowers/specs/2026-11-20-primitives-and-editors-design.md` §1, §5 (Deploy), §11 (runs stay in the database with a link to their files). Bridge decision: the plan series note in `2026-11-21-project-files-and-ops.md`. A YAML-native world runtime is plan 8.

## Global Constraints

- Plan 1's code-quality rules hold: clippy `-D warnings`, fmt, functions of 30 lines or fewer in Rust and 40 in TS, no unwrap outside tests, typed errors.
- Deploy is `Access::Spend`. Over stdio (no running shaked) it answers 409 "deploying needs a running shaked".
- Tests never call a real model. They use the toy benchmark (`toybench`, tasks t1 and t2) and a silent provider, as `tests/experiments.rs` does.
- The schema stays personal-use: the new column holds an item id and names no owner.

## Review Focus

1. **World without a runtime.** Deploying is 422 with "Larkspur Valve Works has no runtime yet: set package: to an installed benchmark or built world" (Task 3 test `no_package_is_explained`).
2. **Setups that can't run yet.** `code` and `madl` setups are 422, naming the setup (Task 3 test `code_and_madl_setups_refused`).
3. **Files with problems.** An experiment whose files have problems can't be deployed. The answer is 422 and lists the first problem (Task 3 test `problems_block_deploy`).
4. **A read-only token.** It can list deployments but not deploy (Task 4 test `read_token_cannot_deploy`).
5. **Stdio.** `experiments_deploy` over stdio is a tool error, not a crash (Task 4 test `deploy_over_stdio_needs_shaked`).

---

### Task 1: Formats know the runtime

**Files:** Modify `core/crates/project/src/format/world.rs` (`package: Option<String>`) and `format/scenario.rs` (`preset: Option<String>`). Test: `core/crates/project/tests/formats.rs`.

- [ ] **Step 1: Write the failing tests:**
  - `world_package_round_trips`
  - `scenario_preset_round_trips`
- [ ] **Step 2:** Run them. Expected: FAIL (unknown field).
- [ ] **Step 3: Add the fields,** documented as the bridge to installed packages.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(project): worlds name their runtime package, scenarios their preset task`

### Task 2: Experiments remember their file

**Files:**
- Create: `core/crates/shakedown-schema/migrations/shakedown/0003_project_experiments.sql`, adding `project_experiment text` to `shakedown.experiments` plus an index on it.
- Modify: `shakedown-schema/src/stream.rs` (register 0003); `shaked/src/store/experiments.rs` (`NewExperiment.project_experiment: Option<&str>`, insert it, and `deployments_of(&self, project_experiment: &str) -> Result<Vec<String>>`, newest first); `shaked/src/api/experiments.rs` (extract `pub(crate) async fn create_experiment(state, caller, req: NewExperimentReq) -> Result<Detail, ApiError>`, and make `NewExperimentReq`/`SetupReq` `pub(crate)` with a `project_experiment: Option<String>` that serde skips)
- Test: `shaked/tests/deploy.rs`

- [ ] **Step 1: Write the failing test** `create_experiment_records_its_file`: calling `create_experiment` with `project_experiment: Some("slip")` makes `deployments_of("slip")` return the new id.
- [ ] **Step 2:** Run `cargo test -p shaked --test deploy`. Expected: FAIL.
- [ ] **Step 3: Implement.** The HTTP handler becomes a thin wrapper around `create_experiment`.
- [ ] **Step 4:** Run the test and `cargo test -p shaked --test experiments`. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): experiments remember the project experiment they were deployed from`

### Task 3: Mapping files to an experiment

**Files:** Create `core/crates/shaked/src/ops/deploy.rs`. Test: unit tests in that file's `#[cfg(test)]`, plus `tests/deploy.rs`.

**Interfaces:**
- Produces `deploy::request(project: &Project, id: &Id) -> Result<NewExperimentReq, OpError>`:
  - `benchmark` is `world.package`.
  - `task` is `scenario.preset`.
  - `scenario` holds a disruption `{id: fault, params}` for every event with a `fault`.
  - `runs` is `repeats.per_setup`, and `max_cost_usd` is `budget.per_run_usd`.
  - `name` is the experiment's label.
  - `setups` come from the setups:
    - `builtin` agents become `AgentSpec { id, name: id, kind: open, model, prompt: role, tools }`;
    - `claude-code` and `codex` become one agent of that kind;
    - `http` becomes one `http` agent with the url;
    - `code` and `madl` are refused.

- [ ] **Step 1: Write the failing tests:**
  - `maps_builtin_setup`
  - `faults_become_disruptions`
  - `no_package_is_explained`
  - `no_preset_is_explained`
  - `code_and_madl_setups_refused`
  - `problems_block_deploy`
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3: Implement,** with small functions: `world_of`, `scenario_of`, `setup_req`, `agent_of`.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): map a project experiment onto the engine`

### Task 4: Deploy and deployments as operations

**Files:**
- Modify: `ops/mod.rs` (`OpCtx.caller: Option<Arc<Caller>>`, set by HTTP and MCP; stdio uses `Caller::person()` with no app); `ops/http.rs` (extract `Who`); `ops/mcp.rs` (pass the caller); `ops/item_ops.rs` or a new `ops/deploy_ops.rs`, which adds:
  - `experiments.deploy`: `POST /api/project/experiments/{id}/deploy`, Spend, which answers the experiment detail;
  - `experiments.runs`: `GET /api/project/experiments/{id}/runs`, Read, which answers `{deployments: [detail…]}`.
- Test: `tests/deploy.rs`, `tests/ops.rs` (the access rule maps `deploy` to Spend), `tests/control_mcp.rs`

- [ ] **Step 1: Write the failing tests:**
  - `deploy_starts_an_experiment_and_lists_it`: on the toy world with a silent provider and a temp project whose world is `package: toybench` and whose scenario is `preset: t1`, POST deploy is 200; then GET runs lists one deployment with 1 setup.
  - `read_token_cannot_deploy`
  - `deploy_over_stdio_needs_shaked`
- [ ] **Step 2:** Run the tests. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** Run `cargo test -p shaked --test deploy --test ops --test control_mcp --test parity --test routes`, then clippy. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): deploy a project experiment and list its deployments`

### Task 5: Deploy and Runs in the UI

**Files:**
- Modify: `ui/src/api/project.ts` (`deployExperiment(id)`, `experimentRuns(id)`) and `ui/src/features/experiments/FileExperimentPage.tsx`:
  - Deploy is enabled and opens a confirm dialog: "Deploy N runs (S setups × R runs each)", with the cap per run if set; Confirm calls deploy.
  - The Runs tab lists deployments: date, status, setups' pass counts, and a link to `/experiments/<db id>`.
  - A 422 shows its message in the dialog.
- Test: `ui/src/features/experiments/file.test.tsx`

- [ ] **Step 1: Write the failing tests:**
  - `deploy_confirms_then_posts`
  - `deploy_error_shown`
  - `runs_tab_lists_deployments`
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** Run `npm test`, lint and build. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(ui): deploy file experiments and see their deployments`
