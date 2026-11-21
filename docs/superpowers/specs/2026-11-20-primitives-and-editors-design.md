# Shakedown app v7: main window, configure window, every core page

Draft v7 for the owner's reaction, 2026-11-20, branch `factory`. Mockups: `scratchpad/primitives-mockups/v7-*.html`
(sitemap `v7-index.html`), built from shared partials in `scratchpad/v7src/` (`base.css` tokens, `build.py`).
Personal use: no orgs, teams or tenancy. Cloud appears only as a deploy target.

How we got here: **v1** full editors ("too convoluted"); **v2** three screens, no editors; **v3** every editor a
step-by-step flow ("the world should be more like a configuration page"); **v5** configuration pages, a Configure
window, graphs, timelines and Deploy; **v6** plain SaaS chrome; **v7** (this doc) a professional visual system
(§4), the classic settings layout for config pages (§3) and a database as the world's default state (§10).

## 1. The model (revised 2026-11-20)

| Concept | What it is |
|---|---|
| **World** | The production systems as they are before any agent: connectors (MCP servers, REST APIs, product connectors such as SAP S/4HANA or Salesforce, or built-in stand-ins) and the hooks each exposes, the **deterministic software flows** that already run between them (trigger → steps → systems), and the data that seeds them; optional people, views and metrics. No agents, events or gates. Built once into a sandbox |
| **Scenario** | An incident or test inside one world (failures, tests, behaviors). **Inputs**: ticket/task, starting changes, injected events and failures with parameters and timing. **Gates**: rules that must hold or that block actions during the run. **Outputs**: expected results as checks on the final state (goal and safety). Events and gates live here and nowhere else |
| **Setup** | An agent system to test: built-in agents, your code, a deployed endpoint, Claude Code or Codex |
| **Experiment** | Runs one scenario against one or more setups: deployment settings, repetitions and results. Every run belongs to an experiment; there is no Runs list. Branching an experiment, or forking from a run step, creates a **sub-experiment** that keeps its link to the parent |

**Run once** is an experiment with one setup and one repeat.

## 2. Two windows

| | Main window | Configure window |
|---|---|---|
| Job | See your information, act on it | Set things up; always re-enterable |
| Pages | Home, Experiments, an experiment, a run, a step, compare, acceptance tests, connect your agent | World, scenarios, agent setups, connections & models, project settings |
| Chrome | Top bar: Home · Experiments · Acceptance tests, project chip, **Configure ↗** | Its own title bar ("Configure · Larkspur Ops", Back to the app) and a section nav: World, Scenarios, Agent setups, Project |
| Opens | `shakedown` / `localhost` tab | A second window (or a full-screen editor in the desktop app) |

**The flow of the main window** is one path: **Home → Experiments → an experiment → a run → a step**. Side doors:
New experiment (guided), Deploy, Compare, Fork from a step, Acceptance tests, Connect your agent.

**Home is your information first:** this week's runs, pass rate, safety violations and cost against the cap;
deployments running now with progress; "Needs your attention" (failed checks and safety violations, each linking
to the run and step); recent experiments with a View all link to the Experiments page (no separate Runs list); acceptance verdicts;
and quick Configure entries that show each item's status (World: Needs rebuild; Scenarios 2; Setups 3).

**Links between the windows:** Configure ↗ in the top bar and on Home, "＋ New setup ↗" in the new-experiment flow;
back the other way, every config page has a **Used by** line.

## 3. Configuration pages (world, scenario, setup, connections)

One pattern for every editor except the experiment, the classic settings layout (v7):
- **Left sub-nav** of sections (World: Overview, Connectors, Software flows, Data, People, Views & metrics) and
  **one section on screen** at a time. **Overview** shows a summary card per section: status and **Configure →**.
- Each section is a stack of **form rows**: label and helper text on the left (~1/3), control on the right (~2/3),
  thin dividers. Options beyond the common ones fold into **Advanced** accordions; never a separate page.
- A **sticky save bar** ("Unsaved changes · Discard · Save") appears only after a change. Saving writes the
  section's file; nothing runs until Build / Dry run.
- **Complex additions** (add connector, add database, add event) open a focused **side panel with steps**.
- Status and its one action stay at the top (world **Needs rebuild** + Rebuild; scenario **Dry run**), with
  **Used by** under the title and the YAML behind **View file**. Empty sections suggest a next step from what exists.

| Page | Sections | Graphic |
|---|---|---|
| World | Connectors (catalog: MCP server, OpenAPI/REST, product connectors, built-in stand-ins) · Software flows (deterministic automations: trigger → steps) · Data (upload `.sql`/CSV/XLSX/JSON, or import via a connector) · More: People, Views & metrics (optional) | World graph (systems + flows) |
| Scenario | Inputs (task, starting changes, injected events with timing) · Gates · Outputs / expected results (checks, goal vs safety; plain language, "✓ understood", selector under Advanced) · Dry run | Scenario timeline + scenario graph |
| Setup | How it runs · Agents & models · Tools per agent · Rules | Team graph (lead → agents → tools) |
| Connections & models | MCP servers · APIs · Model providers & keys · API tokens · Where runs can go · Project settings | — |

## 4. Visual system (v7)

Owner on v6: "it should look like a professional tool not a toy … the UI should guide, not be so convoluted and
compressed." The bar is Linear, Vercel, Stripe Dashboard.
- **Type:** Geist for UI, Geist Mono for code, IDs and paths (Google Fonts; fallback Inter with `cv11`/`ss01` and
  JetBrains Mono). 14px body, 12–13px meta, 15–16px card titles, 20px section titles, 24px page titles; weights
  500–600; no all-caps except tiny eyebrows.
- **Color:** zinc neutrals and one accent, blue `#2563eb`, for the primary button, selection, links and the
  database; green, amber and red only for status. Light and dark tokens on `:root`.
- **Spacing:** 8-pt grid, 24px card padding, 32px gutters, ~1120px reading width (wider only for graphs and
  timelines), 48px table rows.
- **Every page:** a title, a one-line description and one primary action in the top bar; empty states say what to
  do next; YAML only behind View file.
- **Components** (one shared token sheet): primary/secondary/ghost buttons, labelled inputs, selects, toggles,
  segmented controls, radio cards, tabs, breadcrumbs, badges, tables, stepped side panels, modals, toasts,
  skeletons and empty states.

**Identity rules (2026-11-20, supersede the crisp pass's Inter/white).** Product in one sentence:
*Shakedown tests your agent systems against a sandboxed copy of your production systems, with repeatable
incidents, before they touch anything real.* Visual idea: a lab notebook for agent systems, with editorial
page titles over instrument-grade data.
- Fonts: Instrument Sans for UI text (weight 430+), Instrument Serif for page titles and big numbers only;
  IBM Plex Mono only for IDs, code and table headers. No Inter.
- Colour: no pure white or black (light surfaces #fdfcfa on #f6f4f0, ink #1b1815). Warm grays only. One accent:
  copper. Primary buttons are ink. Status colours only for status. The dark mode keeps the CopperTide ink, cream
  and stone colours.
- One primary button, one surface (card) style, one input style, one navigation pattern, with slight depth.
- Dense data (tables, logs) with confident space around page headers. Avoid identical spacing everywhere.

## 5. The experiment: a guided flow, then Deploy

The experiment is the only guided editor, because it is created once and then run many times:
**World → Scenario (in that world) → Setups → Review → Create** (optionally branched from an existing experiment). Done steps collapse to a one-line summary with Edit; "＋ New
setup ↗" opens the configure window. Create lands on the experiment page.

**The experiment page**: the answer in one sentence, setup cards (pass rate, run squares, measures against the
target), "What went wrong" (failed checks linked to runs and steps), a **Deployments** panel, and a prominent
**Deploy** button. The deploy dialog:

Deploy settings (defaults bold): runs per setup (**5**) and parallelism (**4**); where (**This machine** · Cloud,
one sandbox per run · your cluster); **paired seeds**; cost caps per run and total with an estimate; schedule
(**Now** · nightly · on push · cron); notify. Actions: **Deploy 15 runs** · Save as a deployment.

A **deployment** is a saved run plan of one experiment (`experiments/<id>.deploy/<name>.yaml`): the list shows
active and past deployments with target, schedule, progress, pass rate and cost. Every run still belongs to the
experiment; a deployment is how they were started.

**Sub-experiments.** "Branch experiment" (change a setup, prompt or repetition plan) or "Fork from this step" on a
run creates a child experiment that keeps its link to the parent. The experiment page shows a small experiment tree
("Supplier slip: solo vs team › Branch: buyer prompt v2") and a **Sub-experiments** tab listing each child's change
and its result against the parent. Children are files too (`experiments/<parent>/<child>.yaml`).

## 6. Graphs and timelines

Drawn by **generic components** from the definitions; no world-specific UI.
- **World graph** (world config): systems and connectors only, no agents and no gates. Nodes show type and hooks with
  the read/write split; edges are labelled with the software flow that drives them; files sit beside the node they
  seed (a changed file is flagged); the database is a data node with replica or shard sub-nodes (§10).
- **Scenario timeline** (scenario config): world time across; lanes for starting changes, timed events (point or
  bar for duration), triggers (floating, e.g. "after 20 agent calls"), and the check point at the end.
- **Scenario graph**: injected events and gates → the systems and software flows they touch → the data they change; checks → the data they read.
- **Run timeline / process / trends** (run page): the same node and lane components, filled from the recording.

## 7. The run page and world views

Header facts (time, world hours, cost, checks, refused actions, safety, seed, models), the checks strip, tabs
**Timeline · Process · World view · Trends · Event log**, the **step panel** (call, arguments, reads/writes, gate
refusal by a scenario gate, world rows at that step, result, turn waterfall with Shakedown's overhead), **Fork from this step** (restore
world, or world + agent memory; change the setup or prompt; creates a sub-experiment linked to the parent) and **Review this
run** (AI review citing steps). **World view** is generic: KPI tiles, tables and state cards declared in the
world's Views & metrics, with a scrubber to any step. Optimiser tasks add a Best-so-far chart.

## 8. Page inventory

| Page | Window | Mockup (published) | From |
|---|---|---|---|
| Map of every page | — | `index.html` | — |
| Home (recent experiments) | main | `home.html` | `/`, StartPage |
| Experiments (list: sub-experiments nested under parents, status, best result, safety, schedule, filters, drafts with Deploy, upcoming deployments) | main | `experiments.html` | new |
| Experiment (+ Deploy, deployments) | main | `experiment.html` | ExperimentPage; new |
| New experiment (guided) | main | `new-experiment.html` | NewExperimentPage |
| Run: timeline, process, world view, trends, event log, step, fork, review | main | `run.html` | RunPage, ProcessPage, WorldPage; both videos |
| Compare runs or setups + AI review | main | `compare.html` | ComparePage, `/experiments/compare`; Ridgeline A vs B |
| Acceptance tests, test plan, report | main | `acceptance.html` | `fat` acceptance pages |
| Connect your agent | main | `connect.html` | ConnectPage |
| World | configure | `config-world.html` | BuildPage, WorldPage |
| Scenario | configure | `config-scenario.html` | ScenarioBuilder, DisruptionPicker |
| Agent setup | configure | `config-setup.html` | TeamPage, ImportDialog |
| Connections & models, project settings | configure | `config-connections.html` | ModelsPage, TokensPage |

## 9. How connectors and data come in

**MCP server.** shaked connects once, lists the tools (`tools/list`) and records their schemas. Runs never call the
real server: the sandbox answers the same tools from seeded data. Read/write comes from `readOnlyHint` when present,
otherwise from the name and schema; the user can flip it.

**API.** An OpenAPI document (URL or file); each operation becomes a tool, `GET` read and others write, overridable.

**Product connectors.** A catalog: SAP S/4HANA, Oracle (E-Business Suite or NetSuite), Salesforce, ServiceNow,
Workday, Microsoft Dynamics, each listing its hooks ("SAP S/4HANA: 42 BAPI/OData operations · 9 write"). **Connect
real product** (operation list read once) or **Use a stand-in** (built-in simulation seeded from the world's data).
Plain-text names, no logos. **Built-in stand-ins:** ERP, MES, CRM, roster, CMMS, SQL database.

Larkspur mixes them: SAP S/4HANA stand-in, Salesforce, an MCP server for the in-house MES (backed by the
larkspur-prod database, §10), and a carrier rates API.

**Data import.** Files are parsed on upload (`.sql` into tables, CSV/XLSX into a table, JSON into records); a
connector import reads once and stores a snapshot file. Shakedown proposes the connector each item seeds by matching
table and field names. **Build world** loads everything and reports tables and rows. People and the default views
are proposed from the connectors and data, which is why those sections can stay optional.

**Software flows.** Deterministic automations already running in production: a trigger (event or schedule), ordered
steps (system + operation) and the systems touched, e.g. "New Salesforce order → SAP sales order → MES job", "PO over
$150 → approval request". The sandbox runs them exactly.

**Gates and events belong to the scenario.** A gate is a rule a scenario enforces ("PO over $150 needs approval"),
with a scope and block or flag; the world's own approval workflow is a software flow. A world has no "what can
happen" section: each injected event (name, parameters, what it changes, when it fires) lives in the scenario file.
Checks and events written as sentences are translated once into the check/event language and run deterministically;
no model scores a run.

## 10. Database as default state

World → Data has a first-class **Database** source next to files and connector imports, for a customer with a
mirror of production. It is added in a five-step side panel (database topology removed 2026-11-21; see §15):
1. **Engine & exact version:** PostgreSQL 13–17 with a minor version (15.6), MySQL 8.0.x, SQL Server 2022, Oracle
   19c. The default, **Match my dump**, reads the dump header. Runs use that exact server build.
2. **Source:** upload a dump (`pg_dump` custom `.dump`, directory or tar, plain `.sql`, `mysqldump`) as a resumable
   upload with size and progress; snapshot a read replica (connection string from the key store, read-only;
   snapshot now or nightly); or a path on this machine.
3. **Schema preview:** schemas, tables, rows, sizes and extensions, read without restoring. Unsupported parts are
   flagged: `pg_cron` jobs are offered as software flows, `postgres_fdw` foreign tables are skipped.
4. **Privacy:** auto-detected email, phone, name and IP columns with a mask (hash keeps joins, fake keeps format,
   null). Masking runs before the snapshot is stored. **Keep a copy only on this machine** turns off cloud runs.
5. **Default state:** "Every scenario starts from this snapshot." The snapshot (ID, compressed and restored size) is
   restored **per run as a copy-on-write clone** in seconds, so a run never sees another run's writes. **Refresh
   snapshot** takes a new one. **Experiments pin the snapshot** they ran on; comparisons warn across snapshots.

Data summary: "Postgres 15.6 · prod mirror 38.2 GB · masked · default state". The world graph draws the database
as a data node with edges to the connectors, flows and
views that read or write it. **Database faults are scenario events, never world settings:** connection pool exhausted,
query errors or timeouts on a table, disk nearly full (read-only), slow query / lock contention, each with
parameters and timing in Scenario → Inputs → Injected events.

## 11. Files in the project folder

```
larkspur-ops/
  shakedown.yaml
  worlds/larkspur-plant/world.yaml      connectors, flows, data, people, metrics, views
  worlds/larkspur-plant/data/           larkspur-prod-2026-11-19.dump, suppliers.csv, plant_layout.xlsx, roster.json, imports/
  scenarios/supplier-slip.yaml          inputs (task, starting changes, events), gates, checks; + supplier-slip/ticket.md
  setups/planning-team/setup.yaml
  experiments/supplier-slip-solo-vs-team.yaml
  .shakedown/                           git-ignored: built sandboxes, caches
```

```yaml
# worlds/larkspur-plant/world.yaml (excerpt)
format: shakedown-world/3
connectors:
  - {id: erp, label: SAP S/4HANA, product: sap-s4hana, mode: stand-in}     # seeded from data below
  - {id: mes, label: Shop floor MES, mcp: https://mcp.larkspur.internal/mes, auth: {key: larkspur-mes}}
flows:
  - {id: order-to-job, trigger: {on: crm.order.created}, steps: [erp.create_sales_order, mes.create_job]}
data:
  - {database: larkspur-prod, engine: postgres, version: "15.6", snapshot: snap_0412-7f3a,
     masked: true, default_state: true}
  - {import: accounts, from: crm, snapshot: data/imports/accounts.json}    # read once from Salesforce
```

- **Secrets never go in files:** auth is a key-store name (`key: larkspur-mes`) or an env variable. Deployments are
  files next to the experiment (`experiments/<id>.deploy/nightly.yaml`).
- **Runs stay in the database,** each with a resolved copy and hash of the files, data and database snapshot it used,
  so a run can be reproduced after edits; the experiment marks runs whose inputs have changed since.

## 12. Migration

Built worlds and coded benchmarks become worlds with built-in tools and seeded data (coded benchmarks stay read-only
packages); teams, designs and Code connections become setups; loose runs group into read-only one-run experiments.

## 13. Assistant and MCP: every operation by chat or by agent (2026-11-20)

Owner direction: "every operation can just be done with an agent chat window, so all of our systems have MCP as well."

**Today:** shaked serves MCP only to the agent under test (the world's tools at `/mcp/{branch}/{agent}`).
Nothing lets an agent operate Shakedown itself, and the UI has no chat.

**One operation registry.** Each operation (worlds, data, scenarios, checks, setups, experiments,
deployments, runs, forks, reviews, acceptance, connections, project) is defined once, with a typed
input/output schema and an access class: `read`, `edit` (changes project files), `spend` (starts runs
or costs money), `delete`. The registry generates the HTTP route, the CLI command, the MCP tool and the
assistant's tool. A parity test fails if a UI action has no registry operation.

**Shakedown MCP server (control).** `shakedown mcp` over stdio (local, runs as the user in the project)
and streamable HTTP at `http://127.0.0.1:4510/mcp` (API token with `mcp:*` scopes). The mockup lists
64 tools in 8 areas. It is separate from the per-run world MCP that an agent under test gets. Hosted
Cloud exposes the same tools at its own address, with org scopes (Cloud-only, per D16).
Secret values never cross MCP; tools return a secret's name and kind only.

**Permissions** (shared by the assistant and every MCP client, in `shakedown.yaml`):
reads always run. Edits show a diff first by default (option: apply, then report). Spend always asks
by default (option: allow under a daily cap). Deletes always ask. Every edit is an ordinary file change,
so it can be undone or reverted in git.

**Assistant.** An "Ask" button (⌘J) in the top bar of every page opens a right-hand drawer. It knows the
page it was opened from ("Looking at …"), calls the same MCP tools, shows the tools each answer used
(collapsed), and cites runs and steps. It proposes changes as action cards: a diff card (Apply / Open in
editor / Discard) for edits, and a confirm card with the estimate and cap (Confirm / Edit / Cancel) for
spend. It runs on the user's own model key (`assistant:` in `shakedown.yaml`); in Cloud the trial keys
apply (D8). Chat history is kept in the project folder.
Mockups: assistant drawer on every page (examples on Home, World, Scenario, Experiment, Run);
Connections & models → Assistant & MCP.

## 14. Agent setups are MADL (2026-11-20)

Owner direction: "Make sure the agent setup supports all MADL systems … and can visualize / coordinate
it all." MADL (Multi-Agent Descriptor Language, `~/MLSys/MADL`) is the owner's open format.

**Today:** the team importer refuses MADL files ("Open it in CopperTide…").

**Decision (revised 2026-11-21):** an agent setup can be any team (built-in agents, your code, an HTTP endpoint, Claude Code or Codex, any framework). MADL is optional. When a team is described in MADL, Shakedown shows more: the full system graph, validation, resolved settings and the coordination view. A MADL setup is stored as `setups/<name>/team.madl.yaml`. It covers every
participant (agents on any runtime: built-in, Claude Code, Codex, CrewAI, LangGraph, OpenAI Agents, CAMEL,
remote A2A; people; steps), tools, nested groups, contracts (`data_access`, `authority`, acceptance),
layered settings (preferences override, restrictions only tighten), channels and connections (open or
closed mode, bans), topology patterns, workflows, policies and budgets, and layout. Shakedown-only data
(binding MADL tools to the world's connectors, enforcement) lives under the `ai.coppertide.shakedown`
extension. Import accepts MADL YAML/JSON, CopperTide exports, Google ADK and Microsoft Agent Framework
(through MADL's importers). Export writes `.madl.yaml`. Validation shows MADL error codes with
JSON Pointer paths. The resolved-settings view mirrors `madl resolve`. Use the `madl` Rust crate; don't
re-implement the spec.

**Visualize:** the setup page draws the whole system from the document with a generic renderer
(no setup-specific UI code).

**Coordinate:** at run time Shakedown runs each agent on its runtime; enforces connections, bans and
closed mode; provides channels inside the run sandbox; runs workflow stages and round limits; and
enforces restrictions and budgets. Every message and handoff is recorded. The run page has a
Coordination view: the graph with message counts, channel activity, workflow progress against round
limits, refused communication and escalations.

## 14b. Credentials and paths (2026-11-21)

- **Credentials without pasting:** use an existing sign-in (Anthropic console, AWS SSO or profile, gcloud ADC, Azure CLI), environment variables, or a secrets provider (1Password, Vault, AWS Secrets Manager, Google Secret Manager, Azure Key Vault, Doppler, Bitwarden). Shakedown stores only a reference, fetches the value at run start and keeps it in memory. Pasting a key is one option, not the default.
- **What we promise about keys:** they stay on this machine. They are never written to project files, logs, traces or recordings, and never sent to CopperTide. Shakedown can use a key but nobody can view it again (only the last 4 characters show).
- **Where data lives (2026-11-21):** replaces the "Keep a copy only on this machine" toggle. Each world database or data source has a storage location the customer chooses: this machine, their own bucket (S3, GCS or Azure Blob, in their account, through a key-store reference), or CopperTide Cloud. Runs can only go where the data may go: a world kept on this machine runs locally or on their own runner, never on CopperTide Cloud.
- **Blank by default:** pages open clean, with no panel open, no unsaved state, empty fields and nothing preselected. Results such as a key test, a detected version or an assistant draft appear only after the user acts. (Some mockups opened mid-flow only as a demo; the product never does.)
- **No paths:** don't show file or folder paths in the UI. Name things instead. File viewers show content.

## 15. Open questions for the owner

- **Everything configurable in simulation (raised 2026-11-20, parked).** Owner: nothing should be a hard-coded creation such as the database topology; the user should be able to set it up. Direction when picked up: presets (single, primary + replicas, sharded, multi-region) only fill in an editable spec (nodes, roles, links, latency and lag ranges, regions) that the user can change freely. The same applies to connectors, flows and people. Owner said "maybe we ignore that stuff for now".
- **Visual pass (2026-11-20):** neutral white surfaces, Inter for UI text, higher contrast, tighter density, matching the crisp look of the earlier demo videos. CopperTide mark, Slate primary and Copper accent stay.


1. **Configure window:** a real second browser window (proposed), or a full-screen overlay inside the main window?
2. **Deploy targets:** is "Your cluster / runner" needed in open source, or Cloud and This machine only?
3. **Sandbox fidelity for MCP/API tools:** answer from seeded data by schema (proposed), or record once and replay?
4. **Database engines:** ship PostgreSQL first and add MySQL, SQL Server and Oracle after, or all four at once?
