# UI Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The app takes the v7 look and layout. It gets a sidebar shell, a design system that follows the identity rules, a Home page with your information, an Experiments list over project experiment files, and a page for each file experiment. Everything reads the plan 1 project API.

**Architecture:**
- Tokens live in `ui/src/styles/index.css` and are replaced with the v7 identity palette and fonts. Existing components keep working through the same token names.
- A new `AppShell` holds the sidebar and top bar; it replaces the TopBar/StatusBar column.
- New pages live in `features/home`, `features/experiments` (list and file page) and `features/shell`.
- A typed `api/project.ts` client wraps `/api/project/...`.

**Tech Stack:** React 19, react-router 7, Tailwind 4, Radix, vitest + Testing Library, fontsource packages for Instrument Sans, Instrument Serif and IBM Plex Mono.

**Spec:** `docs/superpowers/specs/2026-11-20-primitives-and-editors-design.md` (§2 two windows, §4 visual system and identity rules, §8 page inventory, §14b no paths, blank by default). Mockups: https://claude.ai/artifact/UaHstP5A8oDhLXzUYgZpL6 (home, experiments, experiment).

## Global Constraints

- **Code quality:**
  - `npm run lint` passes with `--max-warnings 0`; functions stay at 40 lines or fewer (the ESLint rule).
  - `npm run build` passes; it runs `tsc --noEmit`.
  - `npm test` passes; it runs vitest with one worker.
  - One component per file for anything over about 60 lines. No `any`.
- **Identity:**
  - Instrument Sans for UI text at weight 430+, and Instrument Serif for page titles and big numbers only. IBM Plex Mono only for IDs, code and table headers. No Inter.
  - Light theme: surfaces `#fdfcfa` on `#f6f4f0` and ink `#1b1815`, with warm grays only. Copper `#ee7a3a` is the one accent, and primary buttons are ink.
  - Dark theme: the CopperTide ink, cream and stone colours, as in the mockups' dark mode.
  - Switches are green (`--ok`) when on.
  - The CopperTide mark is the logo.
- **No paths in the UI:** show names, not folders or file paths. Every page opens clean: no panel open, no unsaved state, nothing preselected.
- Every run uses a real model. Tests mock `fetch`; they never call shaked or a model.
- Machine limits: vitest with one worker. Run `npm run build` once per task at most.

## Review Focus

1. **No project open.** Every new page shows a clear empty state that links to Get started when `/api/project` answers 409, never a crash or a blank page (Task 4 test `home_without_project`).
2. **Problems in files.** An experiment whose file has problems still lists. It shows a warning badge and the first problem's message (Task 5 test `lists_problem_badge`).
3. **Unknown experiment id.** `/experiments/file/nope` shows "not found" with a link back (Task 6 test `unknown_file_experiment`).
4. **Dark mode.** Tokens are defined for both themes, and no component hard-codes a colour (Task 1 test `tokens_defined_for_both_themes`).
5. **Old addresses.** `/` and `/experiments/:id` (database experiments) keep working (Task 3 test `old_routes_still_resolve`).

---

### Task 1: Tokens, fonts and the identity

**Files:**
- Modify: `ui/package.json` (add `@fontsource-variable/instrument-sans`, `@fontsource/instrument-serif`, `@fontsource/ibm-plex-mono`; remove the Inter and JetBrains packages), `ui/src/main.tsx` (font imports), `ui/src/styles/index.css` (tokens for `:root` and `[data-theme='dark']`, and `--font-sans`, `--font-display`, `--font-mono` in the `@theme` block)
- Create: `ui/src/components/brand/Mark.tsx` (the CopperTide mark SVG; its slate lines follow `currentColor`, while the copper line stays `#ee7a3a`)
- Test: `ui/src/styles/tokens.test.ts`

**Interfaces:**
- Produces:
  - CSS variables, with every existing name kept: `--bg --surface --raised --sunken --hover --line --line-strong --fg --muted --faint --accent --primary --primary-hover --primary-fg --alarm --warn --ok`.
  - New variables: `--copper`, `--font-display`.
  - `<Mark className?>` and `<Wordmark/>` (Mark plus "Shakedown").

- [ ] **Step 1:** Write `tokens.test.ts`. It reads `index.css` as text and asserts:
  - `tokens_defined_for_both_themes`: each listed variable appears in both the `:root` and `[data-theme='dark']` blocks.
  - `no_inter`: there is no `Inter` font in the file.
  - `light_palette`: `--surface: #fdfcfa`, `--bg: #f6f4f0`, `--fg: #1b1815`, `--copper: #ee7a3a`.
- [ ] **Step 2:** Run `npx vitest run src/styles/tokens.test.ts`. Expected: FAIL.
- [ ] **Step 3:** Rewrite the token blocks. Install the fonts with `npm install`, and use the version the registry resolves.
- [ ] **Step 4:** Run the test, then `npm run lint`. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(ui): v7 identity tokens and fonts`

### Task 2: Core components

**Files:**
- Modify: `ui/src/components/ui/button.tsx` (one primary style in ink; variants `primary | secondary | ghost | danger`, sizes `sm | md`)
- Create: `ui/src/components/ui/toggle.tsx`, `ui/src/components/ui/card.tsx`, `ui/src/components/ui/subnav.tsx`, `ui/src/components/ui/save-bar.tsx`, `ui/src/components/ui/form-row.tsx`
- Test: `ui/src/components/ui/core.test.tsx`

**Interfaces:**
- Produces:
  - `Toggle({checked, onChange, label, description?})`: a `role="switch"` element with `aria-checked`. When checked it carries `data-on` and uses `bg-[var(--ok)]`.
  - `Card({title?, description?, actions?, footer?, children})`
  - `SubNav({items: {id, label, icon?, status?: 'ok'|'warn'|'new'}[], active, onSelect})`: the active item gets `aria-current="page"`.
  - `SaveBar({dirty, summary, onSave, onDiscard, saving?})`: renders nothing when `!dirty`.
  - `FormRow({label, help?, children})`

- [ ] **Step 1:** Write the tests:
  - `toggle_is_green_when_on`: clicking calls `onChange(true)`, and when checked the element has `data-on`.
  - `save_bar_hidden_until_dirty`
  - `subnav_marks_active`: `aria-current` is on the active item only.
  - `button_primary_uses_ink`: the class contains `bg-[var(--primary)]`.
- [ ] **Step 2:** Run `npx vitest run src/components/ui/core.test.tsx`. Expected: FAIL.
- [ ] **Step 3:** Implement the components with tokens only; no hex values in TSX.
- [ ] **Step 4:** Run the tests and `npm run lint`. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(ui): core components in the v7 style`

### Task 3: App shell

**Files:**
- Create: `ui/src/features/shell/AppShell.tsx`, `Sidebar.tsx`, `TopBar.tsx` (replaces `src/app/TopBar.tsx`, which is deleted along with its test), `nav.ts` (nav groups as data), `Crumbs.tsx`
- Modify: `ui/src/App.tsx`:
  - routes go inside `AppShell`;
  - `/` becomes Home;
  - `/experiments` becomes the list;
  - `/experiments/file/:id` becomes the file experiment;
  - `/experiments/:id` stays the database experiment page;
  - the StatusBar is removed (its health line moves to the sidebar footer as a dot).
- Test: `ui/src/features/shell/shell.test.tsx`

**Interfaces:**
- Consumes: `api/project.ts` `projectStatus()` (Task 4; the shell shows the project name or "No project").
- Produces:
  - `NAV`: groups `[{items: Home /, Experiments /experiments, Acceptance tests (hidden until plan 6)}, {label: 'Build', items: Worlds /worlds, Scenarios /scenarios, Agent setups /setups}, {label: 'Settings', items: Connections & models /settings/models, Project settings /settings/project}]`, plus footer items `Get started /get-started`, `Connect an agent /connect`, `Docs`. Routes that don't exist yet render an empty state saying the page comes with the editors.
  - `TopBar({crumbs, actions})` holds the search button (opens the existing palette, ⌘K) and an `Ask` button. Ask dispatches the `shakedown:ask` window event; plan 4 listens for it.
  - `usePageChrome({crumbs, actions})` lets pages set their top bar.

- [ ] **Step 1:** Write the tests:
  - `sidebar_groups_and_active`: renders Build and Settings, and the current route's item has `aria-current`.
  - `old_routes_still_resolve`: `/experiments/abc` renders the database experiment page (mock its fetch).
  - `ask_dispatches_event`
  - `shows_project_name`: with a mocked `/api/project` returning `{name: "Larkspur Ops"}`.
- [ ] **Step 2:** Run `npx vitest run src/features/shell`. Expected: FAIL.
- [ ] **Step 3:** Implement. The sidebar is 240px, collapses into a menu button under 900px, and shows the Mark at the top and "Shakedown by CopperTide" at the bottom.
- [ ] **Step 4:** Run the tests, `npm run lint` and `npm run build`. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(ui): sidebar shell with breadcrumbs, search and Ask`

### Task 4: Project client and Home

**Files:**
- Create: `ui/src/api/project.ts`, `ui/src/model/project.ts`, `ui/src/hooks/useProject.ts`, `ui/src/features/home/HomePage.tsx`, `home/Stats.tsx`, `home/NeedsAttention.tsx`, `home/ProjectCard.tsx`, `home/home.ts` (pure functions over runs)
- Test: `ui/src/api/project.test.ts`, `ui/src/features/home/home.test.tsx`

**Interfaces:**
- Produces:
  - `projectStatus(): Promise<ProjectStatus>`
  - `listItems(kind: ItemKind): Promise<{items: ItemSummary[]}>`
  - `getItem(kind, id): Promise<Item>`
  - `putItem(kind, id, body, confirm = true): Promise<Item | Proposed>`
  - `deleteItem(kind, id)`
  - Types: `ItemKind = 'worlds'|'scenarios'|'setups'|'experiments'`; `ProjectStatus {name, counts, problems}`; `ItemSummary {kind, id, label, hash, problems: Problem[]}`; `Problem {kind?, id?, pointer, message}`.
  - `useProject()` is a `useQuery` that maps a 409 answer to `{open: false, message}`.
  - `weekStats(runs, now): {runs, passRate, safety, cost}` and `attention(runs): Run[]` (failed or safety-violating runs, newest first, at most 5).

- [ ] **Step 1:** Write the tests:
  - `put_sends_confirm`: the body is `{body, confirm: true}` and the method is PUT.
  - `status_409_is_not_open`
  - `week_stats_counts_last_7_days`
  - `home_without_project`: a 409 renders "Set up your project" with a link to `/get-started`.
  - `home_with_project`: shows the project name and counts, with Worlds, Scenarios and Agent setups as links.
- [ ] **Step 2:** Run `npx vitest run src/api/project.test.ts src/features/home`. Expected: FAIL.
- [ ] **Step 3:** Implement. Runs come from the existing `api/runs.ts` list. The page title is in the display font ("Good afternoon", chosen by local hour).
- [ ] **Step 4:** Run the tests and lint. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(ui): project client and the Home page`

### Task 5: Experiments list

**Files:**
- Create: `ui/src/features/experiments/ExperimentsListPage.tsx`, `experiments/list.ts` (filter and sort, pure)
- Test: `ui/src/features/experiments/list.test.tsx`

**Interfaces:**
- Consumes: `listItems('experiments')`, `getItem`, and the existing `experiments()` (database experiments).
- Produces:
  - A table of file experiments: label (linking to `/experiments/file/:id`), scenario, setup count and problems.
  - Sub-experiments (those with `parent`) are indented under their parent.
  - A section "Earlier experiments", listing the database experiments with links to `/experiments/:id`; plan 7 migrates them.
  - Search and a segmented "All / With problems" filter.
  - `filterExperiments(rows, {query, problemsOnly})`

- [ ] **Step 1:** Write the tests:
  - `children_follow_parent`
  - `lists_problem_badge`: a row with a problem shows a "1 problem" badge and its message in a tooltip/title.
  - `search_filters_by_label_and_scenario`
  - `empty_project_shows_new_experiment`
- [ ] **Step 2:** Run `npx vitest run src/features/experiments/list.test.tsx`. Expected: FAIL.
- [ ] **Step 3:** Implement. Each row needs the experiment body for its scenario and parent, so fetch with `getItem` per row; projects are small.
- [ ] **Step 4:** Run the tests and lint. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(ui): experiments list over project files`

### Task 6: File experiment page

**Files:**
- Create: `ui/src/features/experiments/FileExperimentPage.tsx`, `experiments/FileOverview.tsx`
- Test: `ui/src/features/experiments/file.test.tsx`

**Interfaces:**
- Consumes: `getItem('experiments', id)`, plus `getItem` for its scenario and setups (their labels).
- Produces:
  - Header: label in the display font, then the scenario and world names.
  - Tabs: Overview (setups, repeats, budget, parent link, problems list), Runs (an empty state "No runs yet. Deploy runs this experiment."), and Settings (read-only fields for now; plan 3 makes it editable).
  - A "Deploy" primary action, disabled with the title "Deploying comes with the world runtime" until plan 3 enables it.

- [ ] **Step 1:** Write the tests:
  - `shows_setups_and_repeats`: 3 setups × 5 repeats gives "15 runs".
  - `unknown_file_experiment`: a 404 renders "No experiment named nope" and a link to `/experiments`.
  - `problems_listed`
- [ ] **Step 2:** Run `npx vitest run src/features/experiments/file.test.tsx`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the full UI gate: `npm test`, `npm run lint` and `npm run build`. Expected: all pass.
- [ ] **Step 5: Commit:** `feat(ui): file experiment page`
