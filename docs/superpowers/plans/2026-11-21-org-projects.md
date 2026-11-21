# A Project Folder per Org Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Each org on hosted Shakedown has its own project folder, so one org never sees or changes another's worlds, scenarios, setups or experiments. Open-source shaked keeps its one folder.

**Architecture:**
- **Engine (this repo):**
  - A layer hook picks the folder for a caller: `Layer::project_dir(&self, c: &Caller, configured: Option<&Path>) -> Option<PathBuf>`. Its provided default returns `configured`, which is `SHAKEDOWN_PROJECT`.
  - `AppState` keeps one `ProjectState` per folder (`Projects`), and `OpCtx` holds the caller's `Arc<ProjectState>`.
  - Project-level operations (`project.init`, `project.update`) act on `Target::Kind(ProjectFile)` instead of the instance, so the layer and token scopes decide them like item edits.
- **Cloud (`CopperTide-Cloud`, branch `hosted-shakedown`):**
  - `HostedLayer::project_dir` is `<SHAKED_HOSTED_PROJECTS>/<org id>`.
  - The authorize hook treats project files as the org's own: any member reads them; owners, admins and members change them; viewers can't.

**Tech Stack:** Rust (shaked, shaked-hosted).

**Spec:** `CopperTide-Cloud/docs/superpowers/specs/2026-11-19-hosted-shakedown-design.md`, which calls Shakedown multi-tenant per org (D3) with organisations only in the cloud (D16). Per-org project folders come from the 2026-11-21 owner go-ahead ("lets get all of those items up and running"), item 3: the project files from plans 1–7 came from one folder, so every signed-in user would have shared one project.

## Global Constraints

- Plan 1's code-quality rules hold: clippy `-D warnings`, fmt, functions of 30 lines or fewer, no unwrap outside tests, typed errors.
- **Open source names no org** (D16). The engine hook takes a caller and returns a folder. Org ids appear only in the Cloud layer.
- **A folder an org hasn't made yet doesn't exist on disk.** `project.status` says "no project yet", and `project.init` makes it.
- **Hosted never falls back to `SHAKEDOWN_PROJECT`.** A hosted caller without an org gets no project.

## Review Focus

1. **Two orgs, same item id.** Each sees only its own `plant` world (Task 3 test `orgs_see_only_their_own_project`).
2. **A viewer edits.** The edit is refused and nothing is written (Task 3 test `viewer_cannot_change_project_files`).
3. **The first request of an org with no folder.** Status answers "no project yet", and init creates the folder (Task 3 test `first_init_makes_the_org_folder`).
4. **A read token runs `project.update`.** 403, because project settings now need the admin scope like other project files (Task 2 test `update_needs_admin_token`).
5. **The same folder from two callers.** One cached `ProjectState` is shared, so an edit by one is seen by the other on their next read (Task 1 test `same_folder_shares_state`).

---

### Task 1: A project per caller in the engine

**Files:**
- Modify: `core/crates/shaked/src/layer.rs`, adding `fn project_dir(&self, c: &Caller, configured: Option<&Path>) -> Option<PathBuf>` with a provided default that returns `configured`.
- Modify: `core/crates/shaked/src/project_state.rs`:
  - add `Projects { configured: Option<PathBuf>, open: Mutex<HashMap<PathBuf, Arc<ProjectState>>> }`;
  - add `Projects::get(&self, dir: Option<PathBuf>) -> Arc<ProjectState>`, where `None` gives a shared unconfigured state;
  - add `AppState::project_for(&self, c: &Caller) -> Arc<ProjectState>`;
  - replace `AppState::project()` and `with_project_mut()` with caller-taking forms, or remove them where `OpCtx` covers them.
- Modify: `core/crates/shaked/src/app.rs` (`project: Projects`), `ops/mod.rs` (`OpCtx.project: Arc<ProjectState>`; `as_caller` resolves it), `api/assistant.rs` (the settings are the caller's project's), `assistant/prompt.rs`, `ops/stdio.rs` and `cli.rs`.
- Test: `core/crates/shaked/tests/project.rs`, with:
  - `layer_picks_the_callers_folder`: a test layer maps two token callers to two folders, and each lists only its own worlds;
  - `same_folder_shares_state`.

- [ ] **Step 1:** Write the failing tests.
- [ ] **Step 2:** Run `cargo test -p shaked --test project`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the shaked suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): the layer picks each caller's project folder`

### Task 2: Project settings are project files

**Files:**
- Modify: `core/crates/shaked/src/ops/project_ops.rs` and `ops/http.rs`, so `project.init` and `project.update` target `Kind(ProjectFile)` and `project.status` stays on the instance.
- Test: `core/crates/shaked/tests/ops.rs` (`update_needs_admin_token`).

- [ ] **Step 1:** Write the failing test.
- [ ] **Step 2:** Run it. Expected: FAIL (a read token gets past the check today).
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `--test ops --test parity --test routes` and clippy. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): project settings are authorized as project files`

### Task 3: Each org's folder in hosted Shakedown (Cloud repo)

**Files:**
- Modify: `crates/Cargo.toml` (pin the new Shakedown commit) and `crates/shaked-hosted/src/{layer.rs,authorize.rs}`:
  - `project_dir` is `projects_root.join(org id)`, or `None` without a root;
  - `HostedConfig.projects_root` comes from `SHAKED_HOSTED_PROJECTS`;
  - project files are allowed by org role.
- Test: `crates/shaked-hosted/tests/org_projects.rs`, with:
  - `orgs_see_only_their_own_project`
  - `viewer_cannot_change_project_files`
  - `first_init_makes_the_org_folder`
- Replace `hosted_project_files_are_refused_until_orgs_have_their_own` in `tests/auth.rs`, since it no longer applies.

- [ ] **Step 1:** Write the failing tests.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the hosted workspace suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit:** `Hosted Shakedown: a project folder per org`
