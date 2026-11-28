# Shakedown UI: look, tokens, components, patterns

Shakedown's web UI is extra sparse: each screen shows only what a first-time user needs to do its one
main thing, with one primary action. Everything else is one click away under **⋯**, **More**,
**Details**, **Options** or ⌘K. No helper paragraphs on screens (a tooltip at most), no ids, no
secondary metadata lines, few borders, plenty of whitespace.

## Screens

- **Top bar** (`app/TopBar.tsx`): the wordmark, **Experiments** and **Runs**, and one **⋯** with Run
  once, Compare two runs, World view of a run, the Team editor, AI review on or off, the command
  palette, the theme and which Shakedown core is in use.
- **Sidebar** (`features/shell/nav.ts`, `Sidebar.tsx`): only the core: **Home**, **Experiments**, then under Build
  **Worlds**, **Scenarios** and **Agent setups**; at the foot **Get started** and **Settings**. Settings is marked on
  every page it holds, Runtimes and Connect an agent included.
- **Settings** (`features/settings/SettingsPage.tsx`): tabs **Connections & models** (`/settings/models`), **Project
  settings** (`/settings/project`) and **Advanced** (`/settings/advanced`, `AdvancedPage.tsx`): one link each to
  Runtimes, Observability (`/settings/observability`), API tokens (`/settings/tokens`) and Connect an agent; the
  Advanced tab stays marked on the two that live under `/settings`.
- **Get started** (`/get-started`, `features/onboarding/`): setup is two steps, both done on this page: **Your
  project** and **Connect a model**, where the key is added in place (`StepModel` shows the same add form as
  Settings → Models, `AddKeyForm` and `useModelKeys`, starting on OpenRouter; what we promise about keys shows on
  hovering **Your keys stay yours**). Each is ticked off from what shaked
  says exists (`steps.ts`); the page opens on the first step not done, moves on by itself when the step it
  shows gets done (`firstOpen`), and opens Shakedown (`/`) once both are. Worlds, agents and scenarios are made
  in the app; Home offers a ready example first. There is no skipping: until setup is done, the gate (`gate.tsx`, around every page in `App.tsx`)
  sends every page but Get started here. It reads `GET /api/project` and `/api/providers` again on each
  navigation while shut, never browser storage; a failed read opens it rather than lock you out. The header
  has the account menu, so a hosted user can still sign out. **Track with git** shows only where a
  project is a folder on disk (`folder` in the no-project answer); hosted projects live in a database.
- **Home** (`/`, `features/home/`): the week in figures, what needs attention and the project's contents. While
  the project has no worlds, **Start from an example** (`ExampleCard.tsx`) comes first: what it adds shows on
  hovering its title, and **Add example**, which calls `project.example` (`POST /api/project/example`, `confirm: true`) and
  nothing before the click. It adds a tiny retail order desk on the built-in `retail` benchmark (a world, a
  scenario, an agent setup on your newest working model key and a one-setup experiment), then opens that
  experiment (`/experiments/file/order-desk-first-run`) with a toast saying what a run of it shows, so the next
  click is **Deploy**. A refusal (409: an id is taken; 422: no model can be worked out) shows under the button.
  The assistant can propose the same as a card (`project_example` in `features/assistant/applyCard.ts`).
- **Experiments** (`/experiments`, `features/experiments/ExperimentsListPage.tsx`): every experiment file,
  sub-experiments under their parent; each row the name (with any problems), whether it ran
  (`features/experiments/runStatus.ts`, read from each experiment's deployments: ■ Has results, ◐ Running,
  □ Ran, no results in alarm ink, ○ Not run yet; hover gives the runs ended and the last deployment), its
  scenario and its setups. Search, **Any status** narrows to one status, and the sort is **Grouped** (parents
  with their sub-experiments), **Has results first** or **Last deployed** (both flat). Below, the earlier
  experiments made before the project folder. Empty: **New experiment**.
- **Runtimes** (`/runtimes`, `features/runtimes/`): Settings › Advanced, or **Manage runtimes** in a world's Runtime
  section. A short warning, **⚠ Runtimes run code on this machine** (the full warning on hovering it; hosted says it
  installs none); each install as a card (its source: git URL, ref and commit, or
  archive name and SHA-256, and folder; **Check again**, **Update**, **Remove**, a refused removal saying which worlds
  use it) with its packages (kind, version, the worlds they run, the conformance check folded to its first failing
  step); then the packages installed with Shakedown. **Install a runtime** (`InstallRuntime.tsx`, also in a world's
  Runtime section): Git URL (repository, ref, folder) or Archive (a file and a folder),
  **Check** shows the packages found, **Install** installs them and shows each package's check.
- **World** (`/worlds/:id`, `features/editors/world/`): **New world** picks a runtime among those installed; the
  Runtime section picks or installs one. **Data** uploads a file (`data/<name>`, marked "Every run starts from this"
  by default), with **View** (`DataFileViewer.tsx`: JSON records as tables, a database's sections as tabs, search, a
  click on a value edits it in place and saves the file at once; other files as text), **Replace** and **Delete file**.
- **Scenario** (`/scenarios/:id`, `features/editors/scenario/ScenarioEditor.tsx`): Overview, Inputs, Events, Gates,
  Checks and **Experiments (N)**: every experiment file that plays the scenario (`ScenarioExperiments.tsx`, also at
  the foot of Overview), sub-experiments under their parent, each a link with whether it ran and its setups.
- **An experiment file** (`/experiments/file/:id`, `features/experiments/FileExperimentPage.tsx`): where it sits
  (world › scenario), **Delete** and **Deploy**; any problems in its file; then its **Runs** (each deployment with its
  setups' passes and **Open results**); then **Configuration**, folded (`FileConfig.tsx`): closed, one line ("3 setups ×
  5 runs = 15 runs · paired seeds · $1.50 cap a run"); open, its parent and the editor for its world, scenario, setups,
  runs per setup and spend cap.
- **New experiment** (`/experiments/new`): the name inline as the page title; **Scenario** (the
  task by plain name, then disruptions as a picker); **Agent setups**, 1 to 50 cards (name, team,
  model for Claude Code agents, rules added to every prompt; a dashed card adds one, copying the
  last); **Runs per setup** (a number from 1 to 1000; default 5); a bottom bar with "2 setups × 5 runs = 10 runs",
  what blocks starting, and **Run experiment**.
- **Experiment** (`/experiments/:id`, `/experiments/compare?ids=`): the name, status and scenario
  line; **Stop** while it runs. The page is as wide as its setups need (two fit a narrow page, four a
  wide one; more wrap, four to a row, and the checks scroll sideways). One card per setup (`features/experiments/Results.tsx`, model in `features/experiments/model.ts`): name
  and team; the pass rate at 40 px over one bar (passes in the result ink, failures in the alarm
  ink) with the 95% Wilson range as a whisker under it; a square per run (ink passed, red ✕ failed,
  ! not scored, pulsing running, dashed not run yet, a dot for a safety violation; hover shows its
  checks and cost, click opens it); then any measure a check gives (`features/experiments/measures.ts`: the label,
  the best run in bold across setups with the mean beside it, or the mean per run for a count, and each
  scored run as a dot on a scale every setup shares, with the target as a solid tick and an offline
  reference as a dashed one, named once under the cards); then checks passed, cost and time, the best in bold. Safety
  violations, denied actions and flags are rows only when some setup has one; otherwise one quiet
  "No safety violations, denied actions or flags". Under the cards, the AI comparison for two
  setups, then the checks that some run failed, side by side, then **Configuration**, folded
  (`ExperimentConfig.tsx`): benchmark, runs per setup, seed, world time window, spend cap per run, when it
  started and ended, and each setup's runner and agents with their models.
- **Runs** (`/runs`, `features/runs/RunsPage.tsx`): name, team, result, when; reruns under their run as "↳
  Rerun". A row opens on Run once's live view; its menu has All details, Process view and Delete.
- **Run once** (`/run`, `features/start/StartPage.tsx`): the world's name, a task picker, the team board
  and one big **Run**; **Options** holds the disruptions, the runner and the team's Suggested, Import
  and Edit. After **Run** the same screen is the live run (`?run=&branch=`, `features/start/LiveView.tsx`):
  **Pause** / **Resume**, **Stop**, status and time, **Timeline | Digital Twin**; a step opens a slim
  drawer; at the end the result in one sentence, **Details** and the run's **⋯**.
- **Run page** (`/runs/:run/:branch`): the task, its status and the run's **⋯**; time, cost, check,
  safety and any refusal; the review line; for an optimisation run, **Best so far** (`features/run/BestSoFar.tsx`,
  `features/run/candidates.ts`: each candidate's objective by simulation number with its interval, ink for a new
  best, hollow for one that missed a limit, the best feasible as a step line, the target dashed, the
  verified value as a diamond; a dot selects its step); the timeline and event log with the step panel beside
  them; the trace once a step is picked; **More about this run**.
- **A run's ⋯** (`features/run/RunMenu.tsx`, on the run page, the world view, the process view and the live
  result): its other views, Compare with another run, its reruns, and under **Advanced** the only way
  to rewind: "Rewind to step N and rerun…" opens the fork panel in the step panel (`?fork=1`).
- **Connect your agent** (`/connect`, `connect/`): from **Connect an agent** on Agent setups, Settings › Advanced, a
  setup's team picker ("Connect your agent…") or Run once's Options (**Connect**). **HTTP | Bedrock**; a URL and optional headers (one `Name: value`
  per line, masked unless focused), or a Bedrock model and region; **Test connection**
  (`POST /api/agents/check`: the agent's name and round trip, or the reason with ⚠), then **Use this
  agent** goes back with a one-agent team selected. Any edit means testing again. This browser keeps
  the agent shaked answered (`shakedown.connected.v1`; header values only as shaked's secret references,
  never the values), listed as "<name> (connected)" in both pickers.
- **Build a world** (`/worlds/new`, `features/build/`): from ⋯ ("Build a world…"), ⌘K, or the last option of the
  world picker. A stepper (Template, Company, Systems, People, Events, Review; done steps ✓ and clickable; on
  phones "Step 3 of 6 · Systems"), one panel per step, and a bottom bar with **Back**, what the step comes to or
  what blocks it in alarm ink, and **Next** (on Review **Build world**). Template: cards. Company: the name and
  the size (presets or an exact number), the seed under Options. Systems: a checkbox per system (a required one
  stays ticked), its options, the memory line. People: the roles with their counts, then one role's names as
  inputs with ✕ and "+ Add person". Events: a checkbox, label, Trap or Event, params inline. Review: the company,
  the tasks with their checks, the images (✓ present, ⚠ missing with its command) and memory, and what was left
  out. Then "◐ Preparing · 23 s" with an indeterminate bar, "✓ Ready · 38 s" with **Run** and **New
  experiment**, or "✕ Failed", why and **Try again**. Model in `features/build/builder.ts`.
- **Import from git** (`features/editors/ImportFromGit.tsx`, beside **New world** on the Worlds page): a
  repository (a git URL or a path on the machine shaked runs on) and a ref (default `HEAD`). **Check**
  (`POST /api/project/import-git`) fetches the ref and shows the commit, the repo's packages, its project files with
  what happens to each (new, replaces the last import, already the same, or kept, why on hover), its views, its
  Compose files as `service` candidates, and any problems or taken package names in alarm ink; a taken name
  disables **Import**. **Import** (`confirm: true`) installs, registers and writes, says how many packages were
  registered, and reloads the list.
- **World picker** (`features/start/WorldPicker.tsx`, on Run once and New experiment above the task): the worlds
  by title, the current one chosen; choosing one makes it current (`POST /api/worlds/{name}/use`) and reloads the
  page; the last option is "Build a world…".
- **Run switcher** (`features/run/RunSwitcher.tsx`, on the run page under the header): for a run of an experiment, one
  folded line, "Experiment · <name>", ‹ Setup · run 2 of 3 › to hop through the experiment's started runs, and
  **Results**. Opened (remembered in `shakedown.run.switcher`), a strip of squares per setup (the experiment page's,
  the run on screen ringed) and this run's original and reruns; **Pick runs to compare** starts with the run on
  screen ticked, a click ticks another, and **Compare N** opens them side by side. The experiment is found from
  the experiments list (`features/run/experimentOf.ts`: the newest that ran the run's task and started before it,
  at most five read). A run outside an experiment shows only its reruns, or nothing.
- **Compare runs**: two at `/compare/:a/:b`; three or more at `/compare?branches=a,b,c`
  (`features/compare/CompareManyPage.tsx`): the outcome table with a column per run (A, B, C…, the best of a row in
  bold), and **Against A**: where each run first leaves A and how many final rows differ, from the pairwise
  `GET /api/compare` per run, with a link to that pair in detail. **World view** (`/world/…`), **Process view** (`/control/…`)
  and the **Team editor** (`/team`) are one click away under ⋯ and ⌘K.

**Which core.** The UI talks to the Shakedown core (shaked) it is served from. A build can name another
with `VITE_SHAKEDOWN_URL=<url>`, and the address can switch with `?core=<url>` (this browser remembers it;
`?core=` with nothing goes back to the serving core); every `/api` request and the event stream go
there (`src/api/core.ts`), and the ⋯ menu says which core is in use. A UI served from another
origin needs the core started with `SHAKEDOWN_CORS_ORIGIN=<the UI's origin>`.

Pause and Stop are shaked's (`POST /api/branches/{id}/pause`, `/resume`, `/stop`): while paused, the
branch's tool calls and gateway model calls wait and the world clock stops; `pause` and `resume` are
events, drawn on the timeline as a shaded band.

## References

- **High-performance HMI (ISA-101).** Grey surfaces, simple shapes for equipment. Colour appears only
  for abnormal states, and always comes with a glyph and a word. The process view and every status mark
  follow this.
- **Honeycomb's agent timeline.** One chronological sequence with a swim lane per agent and handoffs
  drawn between lanes. Clicking a step opens its detail. The run view's action timeline follows this.
- **Trace waterfalls** (Grafana, Elastic APM, OpenTelemetry). Time runs left to right, spans nest, and
  gaps mean waiting or unseen work. The run view's trace of the selected turn follows this.
- **Developer tools** (Linear, Vercel). Dense, calm type, a command palette, keyboard navigation, and
  empty states that say what to do next.

## Tokens

Tokens are defined once, in `src/styles/index.css`, as CSS variables for each theme (light by default,
dark under `[data-theme='dark']`). They are exposed to Tailwind v4 through `@theme`. Components use the
Tailwind names and never raw colours.

| Group | Tailwind names | Use |
|---|---|---|
| Surfaces | `bg`, `surface`, `raised`, `sunken`, `hover` | canvas, panels, controls, wells and code, row hover |
| Lines | `line`, `line-strong` | hairlines, strong borders and lane bars |
| Text | `fg`, `muted`, `faint` | body, secondary, tertiary |
| Accent (the only one) | `accent`, `accent-fg`, `accent-soft` | focus, selection, links |
| Primary | `primary`, `primary-hover`, `primary-fg` | the one primary button: solid, white text, both themes |
| Abnormal only | `alarm`, `alarm-soft`, `warn`, `warn-soft` | refused, failed, unsafe; approximate restore |
| Result ink | `ok` | a passed run's square, a bar of passes (an ink, never a colour) |
| Process view | `equip`, `equip-line`, `flow`, `flow-strong` | equipment shapes; read flows and write flows |

- **Type.** Inter (variable) for text and JetBrains Mono (variable) for ids, numbers and JSON, both
  bundled with `@fontsource-variable`; figures use Inter's tabular digits, mono is for ids, code and
  the timeline axis. The scale is `text-2xs` 11 px (labels), `text-xs` 12, `text-sm` 13 (body),
  `text-base` 14, `text-lg` 16, `text-xl` 20 (page titles), `text-2xl` 28 and `text-3xl` 40 (a result's headline).
- **Spacing.** Tailwind's 4 px scale. Pages use a 16 px gutter on phones and 24 px from `sm`, and one
  of five widths (`PageHeader`/`PageBody` `width`: narrow 768, default 1024, broad 1152, wide 1280, full). Panels
  are spaced 16 px apart. Table rows are 32 px tall.
- **Radii.** `sm` 3 px for chips, `md` 4 px for controls, `lg` 6 px for panels and dialogs.
- **Theme.** System, light or dark, chosen with the top bar button or the palette and kept in
  `localStorage` (`shakedown.theme`).

## Components (`src/components/ui`)

These are shadcn-style components: the code is ours, built on Radix primitives where a primitive exists.

- `button`: variants `default`, `primary`, `ghost` and `danger`. `asChild` styles a link as a button.
- `panel`: `Panel` and `PanelHeader` (a title, a meta note, and actions).
- `side-panel`: `SidePanel`, a slim panel beside the main area (a step, an agent's prompt).
- `page`: `PageHeader` (title, state, actions, children; no border or panel) and `PageBody`. `Stat` is a label
  over a tabular value, with an `abnormal` option.
- `status`: `StatusMark` shows a glyph and a word, and adds colour only when `abnormal`. `Tag` is a
  neutral label, with optional `warn` or `alarm` tones.
- `state`: `EmptyState`, `ErrorState` (says what to try and has a retry), `LoadingState` (skeleton rows
  and what is loading), and `Notice`.
- `input`: `Input`, `Textarea`, `NativeSelect`, `Checkbox` and `Field` (a label above its control).
- `table`: dense table primitives. `data-table` is a generic TanStack Table with sorting, ↑/↓ or j/k,
  Home/End and Enter, and its own loading, error and empty states.
- `segmented`: a few exclusive choices side by side.
- `code-block`: monospace text on the sunken surface (JSON, a diff, stderr). `step-label`: "step 12"
  and the step's kind.
- `tabs`, `tooltip`, `popover`, `dialog` (with `DialogActions`) and `dropdown-menu`: Radix.
- `command`: cmdk, used by the palette. `toaster`: sonner.
- `split`: a main area and a side panel that can be resized by dragging or with ←/→ on the handle; `expanded`
  gives the side panel the whole width.
  Its width is kept per page. The two stack on narrow screens.
- `chart`: `Sparkline` and `LineChart` on uPlot, which follow the theme and the box width.

## Code layout (`src`)

- `api/`: the HTTP client and the core address. `model/`: shaked's types and the pure event model
  (actions, lanes, permits, restores, reports). `hooks/`: data and state shared by pages.
- **Quiet text** (`components/ui/hint.tsx`): screens show titles, labels, core data and buttons only. Explanation
  lives on the title or label itself: `Hint` shows it on hovering (desktop) or tapping (phone), with no icon.
  `Card` takes `hint` (explanation, on the title) apart from `description` (a short data line that stays visible);
  `Field` `hint`, `Toggle` `description`, `EmptyState` `hint` and `ErrorState` `hint` all show through `Hint`. An
  ⓘ is used only where there is nothing to hover. Longer detail folds under a `<details>`.
- `lib/`: small helpers with no domain (class names, keys, formats, marks, errors). `components/ui`:
  the primitives above; `components/`: domain pieces several pages share (status marks, provider fields).
- `features/<name>/`: one folder per screen (`experiments`, `runs`, `run`, `process`, `world`, `compare`,
  `connect`, `team`, `start`, `review`, `scenario`) with its page, components, hooks and logic.
  `app/` is the shell: top bar, status bar, palette and theme.
- `lib/productAnalytics.ts`: PostHog for hosted Shakedown only (see
  [architecture](../../docs/internals/architecture.md)); the API calls of the core flow count their step with `tracked`.
- Imports name `@/…` across folders and `./…` within one.

Logic lives in pure modules with tests, and components only render. For example: `lib/keys.ts`
(keyboard moves), `lib/split.ts`, `lib/chart.ts`, `app/palette.ts`, `features/runs/runsTable.ts`,
`features/run/timelineLayout.ts`, `features/run/zoom.ts`, `features/run/flow.ts`, `features/run/result.ts`,
`features/run/stepSummary.ts`, `features/run/waterfall.ts`, `features/run/runStats.ts`, `features/team/board.ts`,
`features/team/importTeam.ts`, `model/summary.ts`, `features/process/flowLayout.ts`, `model/permit.ts`,
`features/experiments/model.ts`, `lib/format.ts` and `hooks/query.ts` (one way to read, poll and share a value).

## Patterns

- **Words.** Labels start with a capital, no all-caps; values start with a capital (Passed, Failed, Read,
  Write, Refused, One after another, Claude Code). No ids, hashes, step numbers or image tags on the
  Experiments, Runs or Run once: they live behind Details and on the run page. Explanations are tooltips,
  not sentences on the screen.
- **Status.** A status is always a glyph and a word: `✓ Passed`, `◐ Running`, `‖ Paused`, `✕ Failed`,
  `⚠ Infra error`, `■ Interrupted`, `■ Stopped`, `◆ unsafe`. Only abnormal states are coloured.
- **Action timeline** (`features/run/ActionTimeline.tsx`). There is one lane per agent, plus World (links and
  world events) and Check. Marks are placed on one time axis. The mouse moves it (`features/run/zoom.ts`):
  the wheel scrolls sideways when there is more to see (else the page scrolls), Ctrl/⌘ + wheel or a
  trackpad pinch zooms around the cursor (fit up to 64×), and a drag on empty space pans. A live run
  follows its newest step until you scroll or zoom away; **Follow** brings it back. Pauses are shaded
  bands. There is no toolbar row: a faint **⋯** over the ruler's right end holds what the marks
  mean, Model calls on or off, and −, Fit and + zoom. Lanes show the agent's name; its model is the
  tooltip.
  - Shapes: hollow circle for a read, solid square for a write (heavier), diamond for approvals, and
    an alarm-coloured triangle for a refusal or error. A model call is a thin tick.
  - Labels: an action carries its system kind ("ERP") where the label fits. The selected mark shows
    the full "ERP write".
  - Messages and handoffs are drawn as arrows between lanes.
  - ←/→ (h/l) move to the selected agent's previous or next step, staying on its lane; ↑/↓ (j/k), Home and End
    move through every agent's steps in order (`features/run/laneSteps.ts`, `useStepKeys.ts`). Not while typing
    in a field or with a tab focused, whose arrows switch tabs. The step panel's header has the lane moves as
    buttons, ‹ Planner · 4 of 37 › (`features/run/StepStepper.tsx`), on the run page, the process view and Run
    once's step drawer; both follow the timeline's order as filtered.
  - The Event log tab lists the same steps as rows (`features/run/EventLog.tsx`), the detail last and
    taking the rest of the width: each step's whole text, wrapped (a tool call with its arguments, a model
    call's reply or its model and tokens, a message in full). **Compact rows** keeps one line per step.
- **Trace waterfall** (`features/run/Waterfall.tsx`). It shows the selected step's turn: the agent's model call
  and every step up to its next model call. Each bar ends when its step was recorded and starts that
  step's `duration_ms` earlier. Shakedown's own work in a step (gate check, recording, save) is listed
  under it.
- **Inspector.** This is the step in full: the system it touched and its effect, the permit the write
  ran under (or the change it named that approvals never recorded), the arguments, reads and writes,
  the world rows at that step, and the result or error. Change and rerun opens in the same panel. A step's
  prose (messages, handoffs, reports, delegations, a finish's summary, a text result) renders as Markdown;
  data, and text that is JSON, shows as JSON. **Full width** in the panel's header gives the step the whole
  page (the timeline stays where it was, hidden); **Side panel** or Esc puts it back beside the timeline.
- **Process view** (`features/process/ProcessDiagram.tsx`, React Flow). Agents are units on the left and the
  world's systems are equipment on the right. Each kind of system has a simple shape: a cylinder for
  data stores, a box for applications, a diamond for approvals, a hexagon for APIs and a truck for
  logistics.
  - Flows: writes are heavier and solid, reads-only flows are thin and dashed. An abnormal flow is
    alarm-coloured and labelled ("⚠ 1 refused").
  - On phones the diagram scrolls sideways.
- **World view** (`features/world/WorldPage.tsx`, `/world/:runId/:branchId`). The twin's title and the run's
  status (the task and disruption are the title's tooltip), the KPIs as plain figures, the scrubber,
  and the benchmark's digital twin
  (`world_view` events, `GET /api/branches/{id}/views`) as an HMI overview: areas as zones left to right
  in process order (`features/world/layout.ts`; no item counts or alarm badges on the zones, the tiles say
  it), approvals as a dashed rail, items as tiles with a glyph and a word, and lot-to-order flows as
  pipes along a bus under the zones between them, one labelled lane each (ids in the tooltip). Tiles changed since the view before get an accent ring and a "what changed" line
  (`features/world/twin.ts`). The scrubber (`features/world/scrub.ts`) stops at each view, marks disruptions (▼) and
  refusals (▲; what each is in its tooltip), plays and pauses (space), and moves with ←/→. The run's ⋯ → Advanced opens the run page's fork
  panel at the next agent step; "Swap architecture" (`features/run/architecture.ts`): Team v2 adds its
  rules to every agent's prompt; the single agent replaces the team (a fork's `team`, which only a run
  an external system drives can take). The selected
  step is `?step=` in the address, shared by the run page, the process view and the world view. On
  phones the zones stack and flows are listed.
- **World views** (`features/views/`, on the World page). When the run's world ships view files
  (`GET /api/branches/{id}/world-views` is not empty), the World page draws them instead of the twin: the view's
  label as the serif title, its screens as tabs (`?screen=`, `?view=` when there are several), each screen a CSS grid
  of ruled, square panels from the file's `areas`. One renderer per panel kind, never per world: figures (`kpis`),
  a MapLibre map on OpenFreeMap's positron tiles repainted in the warm grays (lazy-loaded; points ringed when toned,
  moving docs as chevrons placed on the great circle by the world clock each frame), a board (sticky header, sort,
  quick filters by the payload's bitmask, search, windowed rows), a timeline (windowed lanes, UTC axis, copper now
  line, −/Fit/+ zoom), bars against a limit and a feed. Tones come from the row (`--tone-*` tokens), never from
  the world. A live run polls `at=live` each second and moves aircraft between polls by `speed`; a recorded run
  scrubs over the steps where world time moved plus the end (`?step=`, ←/→, Space). A panel that can't be
  evaluated shows its error alone. Dev fixtures: `src/dev/worldViews.ts` (branches `wv-recorded`, `wv-live`).
- **Team board** (`board/`, React Flow). Any team as a read-only CopperTide-style board: agents as
  cards (name, a one-line job from the prompt, model badge, Read-only, the decisions it owns), group
  zones and "hands work to" arrows from a CopperTide design; any other team laid out from its runner
  (in order: rows of three left to right with an arrow to the next; at once: one row;
  `features/team/board.ts`). It opens readable: a board that only fits below 80% opens at 80% from its top-left corner. The wheel pans, Ctrl/⌘ + wheel or a pinch zooms, a drag pans; −, Fit and + zoom by button and **Expand** opens it full screen (these show on hover). A card shows its prompt beside the board. **Import** (`features/team/importTeam.ts`) takes a
  CopperTide design (`TeamDraft`: `claude` becomes Claude Code, `codex` Codex; the model from the
  agent, its group or the team; places, groups and hands-work-to connections kept; agents put in work
  order; an agent whose `data_access.writes` is empty is read-only) or a Shakedown team, picked, dropped
  or pasted. A kind Shakedown can't run (OpenCode) is refused by name. The board is kept with the team in
  this browser and never sent to shaked. **Edit** opens the team editor (`/team`).
- **Scenario builder** (`scenario/`). On Run once and New experiment as a picker, from the benchmark's
  `scenario_options` (`GET /api/benchmark`): a toggle and parameters per disruption (label and
  description filled with them, as shaked fills the template), and a plain-words preview.
- **Outcome comparison** (`compare/`). Under What matters on the compare page, the rows the two
  branches have, for both: checks passed (the goal checks), safety violations (when a check is a
  safety one), a row per figure a check measured (its label and unit, better side by the measure's
  `better`), a row per KPI of the twin at the end (no better side), agent cost (as the agents
  reported it, labelled reported, and illustrative for scripted usage) and escalations (when an
  agent asked for approval). No world's figures are built in, so a row neither branch has never
  shows; what each row means is its tooltip. The divergence strip on one step axis is under Details. World
  views are not steps: the timeline, run figures, process view and divergence skip them.
- **Fixtures.** A build with `VITE_SHAKEDOWN_FIXTURES=1` answers `/api` from the pharma fixtures
  (`dev/fixtureApi.ts`), for screenshots without shaked.
- **Trends.** Actions per minute for each system kind, tokens, cost and tool latency as sparklines,
  and context per model call as a chart. A cost the record does not give per call draws no line.
- **Data tables.** Rows open with a click or Enter; ⌘K finds any run or experiment.
- **Command palette.** Opens with ⌘K or Ctrl+K. It lists the pages (Experiments, Runs, Team
  editor); the deeper views of the run on screen; New experiment and Run once; compare two runs,
  picked in two steps (a run's ⋯ opens it at the second); the theme; and every experiment and run
  branch.
- **Status bar.** Only what needs attention, otherwise nothing: no model to run agents with (no agent
  CLI and no API key, with each provider, its reason and Check one click away), or saves 90% full or
  more, as a small analog bar that turns to alarm when new runs are refused. It has the class
  `status-line`, which the capture scripts use to hide it.
- **Empty, loading and error states.** Every list and panel has its own, and each says what to do
  next.

## Typed project homes

The item API can return a `revision` for an item kept in tables. Editors send that revision
with a put; a stale edit is a 409. Folder projects keep their existing response shape. Project
history and base64 file export/import are available through operations; this model change does
not add a history drawer or import/export panel.

When `/api/me.features` includes `projects`, the sidebar name is a Radix dropdown of the caller's
projects with a New project dialog. The layer remembers selection per user. Switching or creating
reloads the experiment list under the build's base path, clearing all editors and query caches.
A refused request stays on the current project and shows its error. Local UI keeps the plain name.
