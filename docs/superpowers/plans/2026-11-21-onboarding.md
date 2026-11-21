# Onboarding and Project Settings Implementation Plan (5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A first-run Get started flow takes a new person from nothing to a first scenario. A Project settings page edits the project's name, the assistant's model and what agents may do without asking. The assistant can be opened with a question already written.

**Architecture:**
- A new operation `project.update` (Edit, `PUT /api/project`) writes `shakedown.yaml` through the same diff-first rule as items.
- `/get-started` renders outside the sidebar shell. Each of its steps reads live state (project status, item counts, providers) and ticks itself off. Nothing is preselected or prefilled.
- `ASK_EVENT` can carry `{prompt}`, which the drawer puts in its composer.

**Tech Stack:** Rust (shakedown-project, shaked), React UI.

**Spec:** `docs/superpowers/specs/2026-11-20-primitives-and-editors-design.md` §13 (permissions, assistant settings), §14b (credentials reassurance; blank by default; no paths). Onboarding mockup: https://claude.ai/artifact/UaHstP5A8oDhLXzUYgZpL6 (onboarding).

## Global Constraints

- The code-quality rules of plans 1 and 2 hold.
- **No paths:** the project's folder is never shown. MCP setup uses `SHAKEDOWN_PROJECT`, e.g. `claude mcp add shakedown -- shaked mcp`.
- **Credentials:** onboarding repeats what we promise about keys:
  - they stay on this machine;
  - they are never written to files, logs or recordings;
  - they are never sent to CopperTide;
  - only the last four characters ever show.
- **Blank by default:** inputs start empty, and a step is ticked only from live state.

## Review Focus

1. **shaked started without a project folder.** Get started explains how to start it with `SHAKEDOWN_PROJECT` and doesn't offer Create (Task 3 test `not_configured_explains`).
2. **The folder is already a project.** The project step shows the project's name as done and skips Create (Task 3 test `existing_project_is_done`).
3. **Changing settings to `apply_then_tell`.** The page saves, and the next put writes without a diff (Task 1 test `apply_then_tell_writes_directly`).
4. **A malformed spend cap,** such as a negative or non-number value, is refused with a message (Task 2 test `bad_cap_refused`).
5. **The prompt prefill** opens the drawer even when it is already open (Task 3 test `ask_with_prompt_prefills`).

---

### Task 1: `project.update`

**Files:**
- Modify: `core/crates/project/src/project.rs` (`Project::set_config(&mut self, config: ProjectFile) -> Result<(), ProjectError>`, atomic and checked with `parse` first)
- Modify: `core/crates/shaked/src/ops/project_ops.rs` (`project.update`: `PUT /api/project`, Edit, input `{body: ProjectFile, confirm?}`. Under `diff_first` and without `confirm` it answers `{proposed, diff}`; `project.status` also returns `settings`, which is the ProjectFile)
- Test: `core/crates/project/tests/project.rs` (`set_config_round_trips`), `core/crates/shaked/tests/ops.rs` (`update_settings_diff_first`, `apply_then_tell_writes_directly`)

- [ ] **Step 1:** Write the failing tests.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the tests, plus `--test parity --test routes` and clippy. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): project settings as an operation`

### Task 2: Project settings page

**Files:**
- Create: `ui/src/features/settings/ProjectSettingsPage.tsx`, with sections:
  - **Project:** the name.
  - **Assistant:** model and provider.
  - **What agents may do without asking:**
    - edits: Show a diff first, or Apply, then tell me;
    - runs and spending: Always ask, or Allow under a daily cap;
    - deletes: always ask.
  - **Use from an agent:** the MCP command, with a Claude Code / Codex switch.
- Create: `settings/project.ts` (`settingsFrom(form)` with checks)
- Modify: `ui/src/api/project.ts` (`updateSettings(body)`), `ui/src/model/project.ts` (`ProjectSettings`), `App.tsx` (route `/settings/project`)
- Test: `ui/src/features/settings/project.test.tsx`

- [ ] **Step 1: Write the failing tests:**
  - `loads_and_saves_permissions`: the put body has `permissions: {edit: 'apply_then_tell', spend: 'always_ask'}`.
  - `under_cap_needs_a_number`
  - `bad_cap_refused`
  - `mcp_command_has_no_path`
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement with the EditorFrame look: a SubNav, FormRows and the save bar.
- [ ] **Step 4:** Run the tests, lint and build. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(ui): project settings page`

### Task 3: Get started

**Files:**
- Create: `ui/src/features/onboarding/GetStartedPage.tsx`, `onboarding/steps.ts` (`stepStates(project, counts, providersOk)`, pure) and `onboarding/Step*.tsx`
- Modify: `ui/src/App.tsx` (`/get-started` renders outside AppShell, with a header of the Mark, "Shakedown" and "Skip for now"), `ui/src/features/assistant/AskDrawer.tsx` (an `ASK_EVENT` detail `{prompt}` fills the composer)

**Steps, with a progress rail; each step is done from live state:**
1. **Your project:**
   - Create: name plus a Track with git switch, which is off until turned on.
   - Done when the project is open.
   - Not configured: explain `SHAKEDOWN_PROJECT`.
2. **Connect a model:**
   - Providers status (`/api/providers`), plus a link to Connections & models.
   - The ways to connect: an environment variable, or a key saved to the OS keychain.
   - The promise about keys.
   - Done when a provider is available.
3. **Your world:** create one (opens its editor) or "Describe it to the assistant" (opens Ask with a prompt). Done when there is at least one world.
4. **Your agents:** create a setup (opens its editor), or Connect your own agent (`/connect`). Done when there is at least one setup.
5. **First scenario:** create one, or "Draft it from an incident" (opens Ask with a prompt asking for the incident). Done when there is at least one scenario.
6. **Ready:** a checklist, "Create an experiment" and the MCP one-liner.

- **Test:** `ui/src/features/onboarding/onboarding.test.tsx`, with:
  - `step_states_from_live_state`
  - `not_configured_explains`
  - `existing_project_is_done`
  - `create_project_posts`
  - `ask_with_prompt_prefills`
  - `nothing_preselected` (the git switch is off and the name is empty)

- [ ] **Step 1:** Write the failing tests.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `npm test`, lint and build. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(ui): get started`
