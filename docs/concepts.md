# Concepts

How Shakedown models an experiment and a run: the world a run happens in, the team that works in it, and how a
run is recorded, scored and compared.

Three nouns and one verb. An **environment** is the simulated systems (a benchmark's world). A **scenario** is
what happens to it (a task and its disruptions). An **agent setup** is a model, a team structure and its rules.
An **experiment** runs each of its setups (any number, up to a safety cap of 50) N times on the same environment and scenario and compares them.

Shakedown is a staging environment. It observes and never dictates: it knows every agent so that it can monitor,
record and save it, but it imposes no team shape, order, permission or handoff protocol of its own; limits are
the environment's, enforced by its systems.
Whatever the design does in production, it does here. Shakedown is slower than production (it saves the world and
writes the record at every step), so it times its own work and reports what the run would take without it. The protocol each program speaks is in [protocol.md](protocol.md); routes, settings and disk use
are in [reference.md](reference.md).

## Project files

What a person builds lives in a **project**: `shakedown.yaml` at its root and one YAML file per item, edited in the
app's editors (Worlds, Scenarios, Agent setups, Experiments), through the control MCP server, or by hand. Locally the
project is a folder (`SHAKEDOWN_PROJECT`). Hosted Shakedown keeps each org's project in its database, one row per
file at the same paths, so the files below and every rule about them are the same in both. Code:
`core/crates/project` (formats in `src/format/`, where files live in `src/store/`); routes in
[reference.md](reference.md#http-api); how a project is stored in [internals/project-store.md](internals/project-store.md).

| Item | File | `format:` | Holds |
|---|---|---|---|
| Project | `shakedown.yaml` | `shakedown-project/1` | name, defaults, `permissions` (`edit: diff_first \| apply_then_tell`, `spend`), the assistant's model |
| World | `worlds/<id>/world.yaml` | `shakedown-world/3` | `label`, `group` (optional, e.g. the customer it is for), `package` (its runtime), `connectors`, `flows`, `data`, `people`, `observability` |
| Scenario | `scenarios/<id>.yaml` | `shakedown-scenario/1` | `world`, `task` (the ticket), `preset` (the package's task it plays), `changes`, `events`, `gates`, `checks` |
| Agent setup | `setups/<id>.yaml` | `shakedown-setup/1` | `team`: `builtin` agents, `claude-code`, `codex`, `http`, `code` or `madl` |
| Experiment | `experiments/<id>.yaml` | `shakedown-experiment/1` | `world`, `scenario`, `setups`, `repeats`, `budget`, `parent` |

Every file parses strictly: an unknown field, a wrong `format:` or a credential written inline (a value shaped like a
key, or a literal `password`/`token`-like field) is a problem with a JSON pointer, and a problem never stops the rest
of the project from loading. Broken references (a scenario's world, an experiment's world, scenario, setups and
parent, and an experiment whose world is not its scenario's) are problems of the item that holds them. Ids are 1–64
lowercase letters, digits and dashes. Writes are atomic (in hosted Shakedown a change that touches several files, such
as a cascading delete or a group rename, is saved in one transaction); with `edit: diff_first` a put answers a diff
and writes only when confirmed.

**World data.** A file source in a world's `data` is a path in its data folder, `worlds/<id>/`. The world editor's Data
section has a View button on each file source that shows the file (or the files of a folder) read-only: text up to
1 MiB, or the size of a binary file. The same is `worlds.data` for MCP, the assistant and the API.

**Groups.** A world's optional `group` is the name lists show it under, so the worlds of one customer sit together:
the Worlds page lists groups A to Z, the worlds in no group last, and worlds by label within each. A file without `group` loads and saves exactly as before (the
field is written only when set); a blank name, or one with spaces at either end, is a problem at `/group`. Set it in
the world's Overview (a choice of the project's groups or a new one), drag a world onto a group's header (a toast
offers Undo), or rename a group from its header, which rewrites every world in it, each file checked before any is
written (`worlds.rename_group`, `core/crates/project/src/groups.rs`; an empty new name takes them out of any group,
and an existing name merges the two). Moving or renaming writes `world.yaml` again from its parsed form, so comments
and key order in the file are not kept.

**What uses what, and deleting.** References are walked in one place (`references` in
`core/crates/project/src/validate.rs`, used by the cross-check too): a scenario names its world; an experiment its
world, scenario, setups and `parent`. Deleting an item takes with it every scenario and experiment that names it or a
deleted item through a world or scenario, and an experiment left comparing no setup; an experiment that compares other
setups too only loses the deleted one (`setup_dropped`), and a sub-experiment of a deleted experiment loses its
`parent` and stands on its own (`parent_cleared`); an experiment can do both, and the preview lists every `fates`
that applies (`Project::dependents` and `Project::fallout`,
`core/crates/project/src/dependents.rs`). A delete is refused while anything uses the item (409 with the
`dependents`); "Delete only if unused" in the app is then off. With `cascade: true` the whole delete is planned and
every rewritten file checked before anything changes, then carried out from the leaves in (rewritten experiments,
deleted experiments, deleted scenarios, the item), so a failure part way leaves no reference to a missing item. A
cascade sent with `expect` (what the preview showed) is refused with 409 and the fresh `dependents` if they have
changed; under `edit: diff_first` an unconfirmed cascade answers `{proposed, dependents}` and deletes nothing.

Deleted files are not unlinked: they move to `.shakedown/trash/<stamp>/` at their paths in the project, with copies of
the files the delete rewrote and a `manifest.json` holding a SHA-256 of what the delete wrote to each
(`core/crates/project/src/trash.rs`). `.shakedown/` holds a `.gitignore` of `*`, so a project never commits its trash;
every delete of the last 30 days is kept, and always the last 20. `project.restore` puts the most recent delete (or
the one named by `trash`) back, refusing with 409 while a removed file's path is taken again or a rewritten file was
edited since (a missing one is put back); unconfirmed under `diff_first` it answers the files and the `trash` it would
restore, so confirming restores that delete even if another delete came in between. The app offers Undo in the toast
after a delete and under Project settings → Deleted files (`project.deleted` names the most recent delete). A world's
data folder and a setup's own folder (a MADL team file) stay where they are: only the item's YAML file goes. The
deployments of a deleted experiment are unlinked from it (`project_experiment` set to null) and noted in the delete's
trash (`runs.json`), so its runs list under Earlier experiments, a new file with the same id starts with none, and
undoing the delete links them again (`runs_relinked`). The files have moved by then, so a database failure answers
`runs_unlinked: false` (or `runs_relinked: false`) and is logged rather than failing the request. The delete
dialog, and the assistant's delete card, show all of this before anything is deleted. The Worlds page opens each world
onto its scenarios and each scenario onto its experiments; a world's page lists its scenarios, a scenario's page its
experiments, and an experiment's page shows World › Scenario › Experiment.

**Deploying** a project experiment (`POST /api/project/experiments/{id}/deploy`, `core/crates/shaked/src/ops/deploy.rs`)
maps the files onto an engine experiment: the world's `package` is the benchmark it runs on, the scenario's `preset` is
the task, each event's `fault` and `params` become the package's disruptions, each setup becomes a team (runner
`chain`; a built-in agent's `key` becomes its provider `key:<name>`, and an `http` setup's `auth` becomes a
reference the endpoint's requests are signed from), and `repeats.per_setup` and `budget.per_run_usd` become runs per setup and each run's spend limit (see [Spend limits](#spend-limits); in the app,
the **Spend cap per run (USD)** field of New experiment and of the experiment's Settings tab, a positive number or empty
for no cap). Until
worlds run from their files alone, nothing else in the files reaches a run: a world's connectors, flows, data, people
and observability; a scenario's `task` text (runs get the preset task's ticket), `changes`, `gates`, `checks` and each
event's `at`, `after_calls` and `effects`; an experiment's `per_deployment_usd` (seeds are always paired). A `key` that is not stored, or is another
member's and names its own server, refuses the deploy with a 400 that names it and the setup. An `auth` is checked the
same way: `auth.key` must name one of your own stored keys of kind `custom` (a model provider key or a key shared with
you is refused), the secret must not be empty, and naming both `key` and `env` is refused. `auth.env` is read only where
the layer lets environment keys serve callers (never on a hosted shaked), and only for names starting with
`SHAKEDOWN_AGENT_`; otherwise the 400 does not say whether the variable exists. The agent keeps only the reference
(`{key: name}` or `{env: NAME}`); the secret is read from your keys each time a request is made, under the caller the
run is for, and appears in no event, record, fork note or log. Header values typed into a team are kept in memory for
the caller that typed them (the scope and the principal: the person, the token's holder or the user), and another caller citing their reference, a colleague in the same scope included, gets nothing.
Setups of kind `code` and `madl` are refused ("can't be deployed from a file yet"). Each deployment is an experiment
linked to its file (`GET /api/project/experiments/{id}/runs`). `project.import` moves installed benchmarks and earlier
experiments into the project as files.

## Glossary

- **Integration**: a program, in any language, that puts one kind of system in the world (a database, a folder
  of files, a Docker Compose project, a simulator). It starts instances, runs tool calls, saves an instance's
  state, starts from a save, shows the state (`inspect`) and compares two states (`diff`). In `describe` it
  declares its tools, how it can restore, and whether it uses the Shakedown clock.
- **Part**: one instance of an integration in a world, with a name and a config.
- **World**: the list of parts a benchmark names. Tool names must be unique across a world. A project's world file
  names the benchmark (or built world) whose world it runs on (see [Project files](#project-files)).
- **Benchmark**: a package that names a world and brings tasks, instructions for the agents, tools, the data
  that seeds the world, and for each task the gold actions that define the expected result.
- **Run**: one task, one team, one world. A run has one or more branches; its first branch is the original.
- **Step**: one recorded event of a branch (an agent start, a model call, a tool call, a handoff, ...),
  numbered by `seq`.
- **Save**: after a step that wrote to a part, and for every clocked part whose state changed, shaked saves
  those parts and records the save ids with the step. Parts that can only replay are never saved.
- **Fork**: a new branch of a run that starts at step N of an existing branch with a change (an agent's prompt,
  model or kind). The world is restored to how it was just before the fork point, and the branch reruns from
  there.
- **Restore**: a fork restores the world exactly or approximately, or for evaluation leaves it as the parent
  ended (see [Restore modes](#restore-modes)).
- **Scoring**: when a branch ends, shaked saves every part and diffs the world with the task's gold world (the
  base world plus the task's gold actions, made once and kept). The branch passes when no key differs and at
  least one part could be checked.
- **Experiment**: one task and scenario, 1 to 50 agent setups, N runs each (default 5, at most 1000). Each setup is stored as a
  suite; shaked runs the setups interleaved (round 1 of every setup, then round 2, ...), one run at a time, so
  they meet the same conditions. The UI's main screen (`/api/experiments`; see [reference.md](reference.md)).
- **Suite**: one team run over a set of a benchmark's tasks, each repeated, on the same seeded world and scored
  by the same checks, then aggregated (see [Test suites](#test-suites)). Suites made before experiments show up as
  one-setup experiments.
- **Denial**: a tool call the world refused (its error says refused, denied, not allowed, forbidden, or that it
  exceeds a limit). Counted per run; see [protocol.md](protocol.md).
- **Flag**: a call that asks a human: a write to a part whose system kind is `approvals`, or a tool whose name says
  escalate, flag or ask_human. Counted per run.
- **Safety check**: a named check a benchmark marks `safety: true`: an unsafe action, not a missed goal. Suites
  count its failures apart from the score.
- **Clock**: parts with `uses_clock` get the Shakedown clock, which runs with real time (times
  `SHAKEDOWN_CLOCK_SPEED`) while agents work and stands still while the world is saved or restored and while the
  record is written, so time-based world logic sees about the time it would see in production.

## Restore modes

A fork asks for one restore mode for the whole world. Each integration declares what it can do in `describe`:

- **exact** (`restore.exact`: `save`, `replay` or `none`): the part is exactly as it was. From an exact save
  when the part has one (fast), or by replaying from the start with the same seed and every recorded input up to
  the step (slower the later the step). If a replay gives a different result than the recorded one, the fork
  fails with "replay of <part> diverged at step N, so it can't be restored exactly". A replay is refused when
  the part's fingerprint (versions, source hashes, ...) changed.
- **approximate** (`restore.approximate`: true or false): from a save that may not capture everything (fast).
  Branches restored this way are labelled "Approximate restore" in the run page, the branch list and compare.

| Part declares | exact | approximate |
|---|---|---|
| exact save | from its save | same |
| replay and approximate | replay | from its save, approximately |
| replay only | replay | replay |
| approximate only | refused, naming the part | from its save, approximately |
| none | refused | refused |

The table is implemented in `core/crates/shaked/src/restore.rs`. `GET /api/branches/{id}/restore-plan` and the
rerun panel show, per part, how it will be restored and, for a replay, an estimate of the time.

**none** is for evaluation only (the snapshots-off ablation): the agents get their context up to the fork point,
as in any fork, but the world is left as the parent branch ended, the way forks that keep no world state work.
Each part starts from the save of its end state (the check's final saves, else its last save; a part with only
replay repeats every recorded input) and the clock from the parent's last step. Only a fork request that says
`restore: "none"` gets it, never a default. The branch records `restore_mode: "none"` and each part's `how` as
`none`, so results can be filtered. Such a branch's own steps can't be forked (its world at its start isn't what
the saves before it say; fork its parent instead), and the UI doesn't offer it.

## When the world is saved

By default shaked saves a part after every step that wrote to it, so a fork can start at any step. A part whose
`describe` says `save_at: "points"` (a whole stack, where each save is costly) is saved only at **save points**:

- before a model call through shaked's gateway, when the part changed since its last save;
- when a driver asks (`POST /api/branches/{id}/save`, answered `{"seq", "saves"}`; `seq` is null when nothing
  changed);
- at the run's end, with the final saves.

Each save point is a `save_point` event (`{reason: "model_call"|"asked", saves, error?, paused_ms}`). It is
bookkeeping, not a step: metrics, compare's divergence and the step lists leave it out. A fork whose point has no
save of such a part (it was written since its last save point) moves back to the latest model call or agent start
whose world was saved, with the reason "the world was last saved before an earlier step, so the fork starts there";
the run's start always restores, from the base. A write whose save failed (a part saved after each step) still
refuses the fork, as before.

## Teams and agents

A team is a description: agents, each with an id, a name, a kind, a model and a prompt. It is not a protocol.
Every agent may call every one of the world's tools, writes included, unless its team marks it `read_only`: then
it is given only the read tools, and a write call is refused and recorded. Who does what is the team's design,
not Shakedown's. `GET /api/team/default` gives the team the benchmark suggests (the retail example suggests a
read-only triage, then a resolver; pharma and factory suggest a single agent), every agent Claude Code with model
`sonnet` unless the query names another kind, model or provider. A benchmark that suggests none gets one generic
agent. A benchmark may suggest more teams (`GET /api/teams`), each with the runner its prompts are written for:
pharma offers a dispatching team and its coordinated chain beside the single agent, factory its incident team.
The Run screen opens with the default and offers the others one click away; picking one picks its runner. An agent setup may also list the world tools an agent gets (`tools`): it sees only those, and a call of another
world tool is refused, so a team can give each role only its own systems.

Every agent also gets three tools for talking to the others, all optional and recorded when used (a dispatch run
has its own set, below):

- `handoff {note, to?}`: a note for the agents that start after it (or for one agent). It ends nothing, and an
  agent may leave several.
- `message {to, text}`: a message to one agent, or to every agent with `to: "*"`.
- `inbox {}`: the notes and messages sent to the agent so far, oldest first.

**Who drives the agents** is the run's runner, chosen when it starts and recorded in its `RunStart`:

- `chain` (the default): shaked runs the agents one after another, in the team's order. Each agent's input is the
  task's ticket, the benchmark's instructions, and the handoff notes left for it so far.
- `concurrent`: shaked starts every agent at once with the ticket and instructions. The branch is scored when all
  of them are done.
- `dispatch`: a lead and its team (see [Dispatch runs](#dispatch-runs)).
- `external`: the design's own code drives the agents (see [External designs](#external-designs)).

An agent already deployed elsewhere joins a team as an `http` agent (shaked POSTs it the run and its MCP
endpoint, and it answers when done) or a Bedrock agent (API mode on provider `bedrock:<region>`); see
[agents.md](agents.md).

In every runner the world freezes (no tick, no clock) once the agents are done, and is then scored.

### Dispatch runs

The lead is the agent the team marks `lead`, else its first. It starts with the ticket, the instructions and a
short note on how the run works; every other agent starts idle. The talk tools are:

- `delegate {to, task}` (the lead only): gives an agent work. It is recorded as a `delegation` event.
- `report {text}` (every agent but the lead): reports to the lead, recorded as a `report` event.
- `message {to, text}` and `inbox {}` as in the other runners (`inbox` also holds delegations and reports).
- `finish {summary}` (the lead only): ends the run, recorded as a `finish` event. No other call goes through
  after it.

An agent wakes whenever work, a report or a message arrives for it while it is idle: shaked records a `wake`
(`{reason, mail, idle_wake?, input}`: `mail` the steps whose mail the turn takes, `input` what the agent was
given) and starts a turn with that mail. Mail for an agent that is working waits and comes, all of it, in its next
turn. When a turn ends shaked records an `idle` (`{ok, error?, text}`): the agent waits, it isn't done. An agent
that ends a turn without reporting has what it said last passed on to the lead as its report (a `report` with
`turn_end: true`). Agents keep their context across turns: Claude Code runs as one long-lived headless process
(`-p` with `stream-json` in and out, with the same isolation as in the other runners; each model call its stream
shows is recorded as a `model_call` reported by the client), and an API-mode agent keeps its conversation in
shaked's loop (at most 40 model calls a turn). Codex can't keep a session yet, so a dispatch team refuses it.
Agents work at the same time; their world steps go through the branch's step lock as in every runner, so the
record is one ordered sequence.

The lead's `finish` ends the run: shaked refuses every later call, gives agents still working up to 45 seconds to
end their turn, stops every agent, records each agent's end (tokens, cost, and the time it spent working), then
freezes and scores the world. Guard rails (see `SHAKEDOWN_DISPATCH_*` in [reference.md](reference.md#configuration)):
a time limit for the run (60 minutes by default, time paused left out), and an idle rule: when every agent has
waited with no mail for 20 seconds and the lead hasn't finished, the lead is woken with the state of the team
(each agent's last words); after 3 such wakes the run ends `failed`, scored, with the reason. The time limit also
ends it `failed`, scored. A lead whose turn fails ends the run (`failed` when it was the model's doing, else
`infra_error`); an agent whose turn fails gets no more work, and the lead is told. Passing the run's spend limit
ends it `budget_exceeded`, scored, whichever agent's call passed it (see [Spend limits](#spend-limits)). Pause holds back new wakes as
well as calls, and stop drops every agent at once, as in the other runners.

**Runs recorded before runners** (no `runner` in their `RunStart`) keep the rules they ran under, and so do their
forks: a chain where every agent but the last must call `handoff` with a note, or the branch fails; the last
agent has no `handoff`; an agent's input has only the previous agent's note; and the world freezes before the
last agent's end is recorded.

Runs recorded before `read_only` existed ran under the old rule, where only the last agent could write. A stored
team whose agents carry no `read_only` is read that way (every agent but the last read-only), so those runs, and
forks of them, show and keep the permissions they ran with. Every team stored since records `read_only` for each
agent.

An agent's `kind` picks its mode:

- **CLI mode** (`claude_code` or `codex`): shaked starts the CLI headless and gives it only the world's tools
  (the benchmark's tools plus any tools the parts declare) and the talk tools, through an MCP endpoint of shaked.
  It records every tool call (arguments, result, keys read and written), every note and message, and the tokens
  and cost the CLI reports. It does not see the raw model calls.
- **API mode** (`open`): shaked runs the agent loop itself and sends each model call through its gateway to
  the agent's **provider**. It records everything CLI mode does, plus every model call. Each agent picks one:
  - `openrouter` (the default, also for agents and runs made before providers): needs `OPENROUTER_API_KEY`;
    the model is an OpenRouter model id, `deepseek/deepseek-chat-v3.1` when none is set.
  - `openai`: OpenAI's Chat Completions API; needs `OPENAI_API_KEY` and a model.
  - `anthropic`: Claude through Anthropic's Messages API, translated by the gateway; needs `ANTHROPIC_API_KEY`
    and a model.
  - `local:<name>`: any OpenAI-compatible server (Ollama, vLLM, LM Studio, llama.cpp's server), configured in
    `SHAKEDOWN_LOCAL_MODELS`, for example `ollama=http://127.0.0.1:11434/v1`; needs a model.

  Every call is recorded in one format (OpenAI chat completions), whatever the provider, so replay, forks and
  compare treat all providers alike. The details, and what each provider costs, are in
  [reference.md](reference.md#model-providers). No API-mode run has been measured yet with any provider.

Isolation of the CLI agents (`core/crates/shaked/src/agents/`; checked live by the `live_` tests in
`core/crates/shaked/tests/cli.rs`):

- Claude Code: settings sources empty, strict MCP config, slash commands off, built-in tools off (`--tools ""`),
  so the only tools are the MCP tools.
- Codex: user config, rules and session storage ignored; apps, plugins, hooks and similar features disabled;
  web search off; read-only sandbox. Codex's built-in shell cannot be turned off. A probe from inside the
  sandbox could not open a socket, not even to the loopback Postgres port (`Operation not permitted`). Codex
  still listed its sub-agent tools (`collaboration.*`) in the live checks; they run under the same restrictions.
- Both CLIs run in a fresh temporary working directory, and of shaked's environment they get only `PATH`, `HOME`,
  `USER`, `LOGNAME`, `SHELL`, `LANG`, every `LC_*`, `TERM`, `TZ`, `TMPDIR`, every `XDG_*`, `SSL_CERT_FILE`,
  `SSL_CERT_DIR` and `NODE_EXTRA_CA_CERTS`. Nothing else reaches them: no key, token, secret or password of any
  service, no database or vault setting, and no proxy setting (`HTTPS_PROXY` and the like, which can carry
  credentials). Besides that base each keeps only its own vendor's key, which it may sign in with: Claude Code
  gets `ANTHROPIC_API_KEY`, Codex `OPENAI_API_KEY`. The `provider` of API mode does not apply to them on this
  machine.
- In a sandbox (`shaked worker`), which holds no model key, their model calls go through the worker's gateway
  instead: Claude Code's `ANTHROPIC_BASE_URL` and Codex's `OPENAI_BASE_URL` point at it, the agent's token as the
  key, and the control box sends each call on with the run's key (an agent's `provider`, if it names one, picks
  that key) and records it. A dispatch session there prices its calls into the run's estimate but leaves their
  `model_call` events to the control box. Each Claude Code or Codex process there runs in its own container as
  an unprivileged user, on a network that reaches only the worker's MCP endpoint and gateway.
- A CLI session cannot be resumed in the middle, which drives the fork rules below.

## Forking

You choose a step N and a change (prompt, model, provider, kind, `read_only` or `tools` of one or more agents) and
a restore mode.

- Each part of the world is put back as it was just before the fork point, in the restore mode you choose.
- Agents that finished before the fork point keep their recorded output (for example the handoff note).
- The fork point moves back when the step can't be redone on its own: to the start of the agent when the agent
  active at step N is a CLI agent (a CLI session can't be cut at a tool call) or the change switches its mode;
  to the start of the earliest changed agent when the change is for an agent that finished before step N; and,
  for an API-mode agent, to the model call that chose the tool call at step N. The response and the UI say
  where the fork started (`start_seq`), where you asked for (`moved_from`), and why (`reason`). These rules are
  the chain runner's. A fork of a `concurrent` run restarts every agent from the run's first agent start (agents
  that ran at once can't be cut at one step each). A fork of a `dispatch` run starts where asked (asked for
  `RunStart`, at the first agent's start; asked for a step after the lead's `finish`, at the finish), and every
  agent restarts there in a new session: its first turn's input has the ticket, the instructions and its own
  history before the fork point (its wakes with their input, its tool calls with their results, the work it gave,
  its reports and messages, and how its turns ended). The agents that were working at the fork point (woken, their
  turn not ended) carry on at once; mail recorded before the fork point that no turn of its agent had taken is
  delivered; the others wait for work; the lead's idle wakes so far still count. Shakedown's own notices to the lead
  (an agent failed) are in the input of the turn that got them, not recorded on their own. A fork of an
  `external` run starts where asked: how its agents resume is its design's choice
  ([External designs](#external-designs)).
- A change is never silently ignored.
- If there is no restore point for the step, or a part can't be restored in the mode asked for, the fork is
  refused with an error that names the part.
- Changing `kind` resets the agent's model to the new mode's default, unless the same change also sets a model.
  An empty model in a change clears the model.
- Changing an API-mode agent's `provider` is a model change for these rules. It drops the old model, which
  belongs to the old provider, unless the same change sets one; only OpenRouter has a default model, so the web UI
  always sends a model with a provider. Naming the agent's own provider is no change.
- Changing `read_only` is treated like a prompt change; setting it to what the agent already has is no change.
- `disruptions: {add, remove}` changes the world's scheduled disruptions from the fork point on: `add` takes options
  of the benchmark's scenario (as a run's `scenario` does), `remove` the ids of disruptions the branch's setups armed
  before the fork point (one that already fired is dropped too, but what it changed stays). The change is one call of
  each part's scenario tool, recorded as a setup step at the new branch's start ([protocol.md](protocol.md), "A
  run's scenario"); a part whose tool can't take it refuses the fork. Retail's faults take it, and so do the
  scenario tools on the Python kit's `jsonstate` (utility, OT network, roster, the pharma parts) and the hub's.
- `tools` gives the agent exactly the world tools it lists from the fork on, like a setup's `tools`; it is treated
  like a prompt change, and the same list as the agent's is no change. A tool the world doesn't offer agents is
  refused (400), and so is an empty list, which would mean every tool rather than none (make the agent `read_only`
  or list what it keeps). Tools the fork gives an agent beyond its setup's list are named in the fork note.

## External designs

A design that lives in its own code (or in CopperTide) drives a run itself, and Shakedown records it:

1. `POST /api/runs` with `runner: "external"` starts the world and records `RunStart`; the team may be empty.
2. `POST /api/branches/{id}/agents` registers an agent, at any time during the run. The answer gives its MCP
   endpoint (the world's tools and the talk tools, streamable HTTP) and its gateway (an OpenAI-compatible base
   URL whose calls go to the agent's provider). Its kind is `external` unless it says otherwise.
3. The design drives its agents however it likes, through those endpoints. Every tool call, model call, note and
   message is recorded as in shaked's own runners. A CLI agent's model calls don't pass through the gateway, so
   the design reports each one its client saw (`POST .../agents/{agent}/usage`: the call's tokens, cache tokens
   included, and the model's context window; or a compaction of its context), and Shakedown records it as a
   `model_call` without request or response. `POST .../agents/{agent}/reasoning` records what an agent thought
   or said it would do (`{"items": [{"text", "kind": "thinking"|"step"|"plan", "model_call_seq"?}]}`, up to
   100 items, each cut at 32 768 characters); Shakedown also pulls reasoning out of the model responses it
   records itself. `POST .../agents/{agent}/end` records that an agent is done, with
   the tokens and cost the design reports for it. `GET /api/tasks` gives the ticket and the instructions to give
   the agents.
4. `POST /api/branches/{id}/finish` freezes, scores and stops the world.

A fork of an external run starts at the step asked for, restores the world as it was just before it, and leaves
the new branch live. When a Code connection drove the run, Shakedown starts its driver again for the new
branch, with `SHAKEDOWN_FORK_OF` set to the branch it came from (the answer says `driver: "relaunched"`); the
driver's agents restart from the fork point, and `run.history(agent_id)` gives what each did before it. Its answer gives each agent's recorded history before the fork point and its endpoints on the
new branch, so the design resumes its agents its own way. A branch whose design never calls `finish` stays running
until shaked restarts (it is then `interrupted`).

## Pausing and stopping

Any live branch can be paused (`POST /api/branches/{id}/pause`), whoever drives its agents: shaked's own runners,
in CLI or API mode, or an external system. While it is paused, every new tool call and every model call through
the gateway waits (it doesn't fail), and the world clock stands still; `resume` lets them go on. A CLI agent's
own model calls don't pass through shaked, so it may think on, but it can't act until the branch resumes; time
paused doesn't count against its time limit. `pause` and `resume` are recorded as events, and the pause's length
is counted as overhead (`pause`), so the estimate in production leaves it out. `stop` ends a live branch
`stopped` without scoring it: the agents are dropped where they are and calls that wait are refused.

## Spend limits

A run may have a model-spend limit, `max_cost_usd` (on `POST /api/runs`, `/api/suites` and `/api/experiments`, for
each of their runs; else `SHAKEDOWN_MAX_COST_USD`, else none). It is recorded in `RunStart` and enforced while agents
work, not after:
- Each model call is priced as shaked sees it, at the list prices of `docs/prices.toml` (with their cache and
  long-context rates): a Claude Code agent's calls from its stream-json `usage` (dispatch sessions), an API-mode
  agent's calls through the gateway (the provider's own cost when it reports one). The sum is the run's running
  estimate (`GET /api/branches/{id}`'s `budget.estimate_usd` while it runs). A turn's reported cost, when it comes,
  replaces that agent's estimate.
- Once the estimate passes the limit, the agent's turn is stopped at once (a CLI's process is killed), every other
  agent is stopped, and the run ends `budget_exceeded`, scored as the world stood: a failure, counted in suites as a
  scored run. An agent stopped mid-turn records its live estimate as its cost (label `list_price_estimate`, or
  `api_equivalent_estimate+list_price_estimate` with its earlier turns'), so the run's cost is known.
- Agents whose usage shaked sees only at a turn's end (`claude -p` in a chain or concurrent run, Codex, `http`
  agents) are checked when that turn ends. A fork starts its own estimate from zero under the same limit.
- A run an external system drives (or a Code connection's driver) keeps its limit too: once the estimate is past
  it, the gateway answers each further model call 402 `{error}` without sending it upstream, and the first refusal
  is recorded as the world event `budget/limit_reached`. The run goes on (its driver decides to stop) and is scored
  when it finishes or its window ends.
- A run in a sandbox (`shaked worker`) is held to its limit by the control box, not by the sandbox:
  - The limit is the one the control box planned the run with, kept in the store with the branch's rules, never
    what the worker records; a linked branch with no rules on record makes no model calls (503).
  - The control box's gateway prices every model call of the branch (all three routes) into its own estimate. A call
    whose cost can't be known (output without a reported cost, a zero one, or a stream cut before it said what it
    used: it broke off, said `error`, or ended without its final event) is charged a conservative estimate: its
    input (as reported, else its request's size in bytes over 4) at the provider's highest listed input price times
    1.25 (it could all have been a cache write), plus its output at the highest output price, the output being the
    largest of every output limit the request names and the output it reported (4096 when it names none, unless it
    used more). An OpenRouter model `vendor/model` is bounded by its vendor's prices. When even that can't be had, a
    run with a limit is put over it at once; a run without one goes on. Models without a known cost are refused
    (400).
  - Once the estimate is past the limit, every further call is refused 402 before it reaches a provider,
    `budget/limit_reached` is recorded once, and the branch is stopped for its limit: it ends `budget_exceeded`,
    scored, whatever its worker reports, and the control box ends it itself if the worker does not within 30 s.
  - A branch has a pool of 4 times its agents' count of calls in flight at once, shared by all its agents (Claude
    Code makes some in parallel), so at most that many in-flight calls can finish past the limit: the overshoot is
    bounded, not zero.
  - The estimate, the hold and the refusal are kept in the store: a control-box restart rebuilds them (the spend
    from the `charged_usd` of the calls it recorded).
- The worker keeps its own estimate too, so a well-behaved one stops first. On a local run only the run's own count
  decides: a person's stop stays `stopped`, and a failed agent's infrastructure error stays `infra_error`, over the
  limit or not.

Pausing and stopping a run in a sandbox works the same as a local one for whoever presses it: the control box keeps
the pause or stop itself, refuses the branch's model calls while it is paused (409) or stopping, tells the worker
when it can, and has the last word on how a stopped branch ends. A stopped run in the cloud ends even across a
control-box restart: the restarted control box gives its worker the same 30 s to report its end, then ends it
itself.

## World time windows

A run an external system drives may have a world time window, `world_minutes` (on `POST /api/runs`, more than 0 and
at most 1440; recorded in `RunStart`, and given to a Code connection's driver as `SHAKEDOWN_WORLD_MINUTES`). When its
branch clock reaches the window's end, shaked records the world event `window/time_up` and scores the run as
`finish` would, whatever its driver is doing, so runs of different designs are scored at the same world time. A
paused clock stands still, so a pause extends the window; a fork keeps its run's window (it counts from the run's
clock 0). A driver that finishes earlier is held: `finish` answers `{status: "running", held_until_clock_ms}`, the
run's agents are ended (their endpoints answer no one), and the run is scored when the window ends, `failed` with
the finish's `error` when it carried one. A driver that exits holding a finish does not fail its run, and a driver of
a windowed run is cut off only after the longer of `SHAKEDOWN_CODE_RUN_SECS` and the window's wall time plus the driver
grace.

## What the world says about itself

An integration may say in `describe` what kind of system it is: `system: {kind, product, stands_in_for?,
simulated}` (for example an ERP, `ERPNext`, standing in for `SAP S/4HANA`). Each tool already says whether it
reads or writes (`effect`). A run's `RunStart` records both: `systems` maps each part that says to its `system`,
and `tools` maps each tool of the world to `{effect, part?}` (a benchmark's own tool has no fixed part; its step
records the part it ran on). With each tool call's `part`, a UI can tell from the run's record alone which system
a step touched and whether it read or wrote. A run recorded before this, or a part that does not say, has no such
entry: unknown, not guessed. Named checks keep their `safety` flag in the run's `check` event.
Each tool call also records `duration_ms`, how long the tool ran in the world (gate checks and saves left out;
absent for a call that reached no part, and in older runs).

Shakedown's web UI shows a run in its process view (`/control/{run}/{branch}`): the agents
as units and the world's parts as equipment, labelled with their kind, product, what they stand in for and
whether they are simulated, with the action flows between them; a timeline with a lane per agent, each tool call
marked by system kind and read or write (writes solid, reads hollow, approvals and refusals as interlocks, errors
and refusals the only colour), with its duration and outcome; trends (actions per minute per system, context per
model call, tokens and cost); and the inspector for one step. It follows a running branch over the event stream.
Suites show as experiments (`/experiments/{id}`: pass rates, safety, spread; `/suites/{id}` redirects there), and
several side by side at `/experiments/compare?ids=a,b`. Checked by eye on two external suites of the retail benchmark driven by a script
before scripted drivers were removed; not yet checked again with real agents.

## Built worlds

A built world is a benchmark made from a template in the UI (**Build a world**, `/worlds/new`): pick a template,
name the company, set its size, choose its systems, edit its simulated people, choose its events, review, build.
The world builder (`benchmarks/world`, a package of `kind = "builder"`) generates it from the template, the choices
and a seed, the same way every time, and shaked writes it to `SHAKEDOWN_WORLDS` as a benchmark package, installs it,
makes its base saves in the background and then makes it the current benchmark (new runs use it). Runs, suites
and experiments keep the benchmark they were created with, so choosing another world never changes a run that is
going. Run once and Experiment on a benchmark have a world picker; the choice is remembered across a restart (unless
`SHAKEDOWN_BENCHMARK` names another). A built world is never changed or deleted: a change is a new world under another
name. See `docs/reference.md` (`/api/worlds`)
and `docs/packages.md` ("Templates and built worlds").

## Test suites

A suite runs one team (a deployment) over a set of the benchmark's tasks, each `repeats` times. Every run starts
from the same seeded world and is scored by the same checks, so what varies between runs is the agents. The models
are not deterministic, so a suite measures how reliable the deployment is: how often each check passes, how often
it does something unsafe, and how much its runs vary.

- **Runners.** With `chain` or `concurrent`, shaked makes the runs itself, one after another (each waits for the
  previous one to end). With `external`, the design's own system claims each next run (`POST
  /api/suites/{id}/claim`), and shaked starts it as an external run that the system drives as described in
  [External designs](#external-designs). A built-in suite that shaked was running when it stopped is `stopped`
  at the next start; the runs it had made keep their results.
- **What is counted.** Only scored runs (ended with a `check` event) count in rates and spreads; runs that ended
  without one (an infrastructure error, an interruption) are `unscored`. A run scored against the task's gold
  world instead of named checks has one check, `matches_gold`. Per task and check, the pass rate. The score is
  the share of goal checks passed (safety checks left out); safety violations (failed safety checks) are counted
  apart, with the runs that had one. The pass rate is the share of runs that passed every check, safety included.
- **Spread.** Per task and over the whole suite: the checks passed, measured and estimated time, tokens, cost and
  peak context of the scored runs, each as `{n, unknown, min, max, mean}`.
- **Compare.** Two suites (two deployments, or one before and after a change): each check's pass rate with its
  change, safety, the pass rate, time, tokens and cost, each flagged improved, regressed or the same. Any change in
  a pass rate counts; time, tokens and cost count only beyond 10% of the first suite's mean, as runs vary a little.
  A suite whose task lacks a check the other has shows it as `incomparable`.

Verified by `core/crates/shaked/tests/suites.rs` (`live_`) on a toy world with agents on a real model (Claude Code,
haiku): a built-in suite of 2 tasks, whose report must add up its runs' checks; a comparison whose verdicts must
follow each check's pass rates; and an external suite claimed and driven over HTTP with Claude Code as its agent.
Only the built-in suite has been run (2026-11-07).

## Overhead and estimates

On every step Shakedown times its own work apart from the agents' and the world's. Each recorded event carries
`overhead_ms`, the time Shakedown spent since the branch's previous event, per kind: `restore` (starting the world from
a save or by replay), `save`, `record` (writing the record), `gateway` and `mcp` (the endpoints' own handling,
besides the provider's answer and the tool's run), `gate` (gate checks), `link` (link calls a step set off),
`wait` (waiting for the branch's step lock, which keeps steps and saves in one order) and `score` (the final saves
and scoring). The time spent writing an event is counted on the next one; the last write of a branch is not
counted.

A branch's metrics (`GET /api/branches/{id}`, compare, and the run page) count only its own steps (`seq >=
start_seq`), as compare does:

- **measured**: wall time from the branch's creation to its end;
- **estimated in production**: measured minus the overhead, with the breakdown per kind. When agents run at once
  their overheads may overlap in time, and the sum can then exceed what they added to the wall time, so the
  estimate is a lower bound. Runs recorded before overhead was timed have no estimate;
- **tokens** per agent and model (input and output where they are known), and per agent its model calls, the most
  context one was sent with (`peak_prompt_tokens`), the compactions its client reported, and the overhead recorded
  on its own events (`overhead_s`; when agents run at once, an event's overhead may have held up others too);
- **cost** per agent: the cost the provider reported (`reported`; Claude Code's own figure is an API-equivalent
  estimate); `no API charge` for a local model; `list-price estimate` when shaked worked the agent's cost out at list
  prices itself (cost source `list_price_estimate`: an API-mode agent on a provider that reports no price, or a turn
  stopped before Claude Code said its cost); otherwise tokens times the list price in `docs/prices.toml`,
  labelled `list-price estimate` with the price's source and date (cache and long-context rates included where the
  table lists them; batch discounts are not modelled); otherwise `unknown`. Shakedown never makes up a price. The branch's
  cost is the sum when every agent's is known (labelled `mixed` when their labels differ), else `unknown`.

## AI review

Optional, and off with `SHAKEDOWN_REVIEW=off` or in the web UI's Advanced menu. A real model (Claude Code Sonnet by
default) reads a compact digest of a run's record and says where it went wrong (each claim citing steps), what to
change (a prompt, the team, a tool, the model or the process; for a prompt change the sentences to add) and what
went well; for two runs or two suites, the few differences that matter and which did better. shaked checks every
citation against the record and stores the review. Details in [reference.md](reference.md#ai-review).

## Cost labels

- `openrouter`: the cost OpenRouter reports (API mode on OpenRouter).
- `local (no API charge)`: API mode on a local server; the cost is 0 and the UI shows `no API charge`.
- API mode with OpenAI, Anthropic or Bedrock: they report no price, so each call is priced at `docs/prices.toml`
  (cache writes and cache hits at their own rates, as the gateway charges them) for the models it lists, and the
  agent's end is labelled `list_price_estimate`; a model it doesn't list leaves the cost `unknown`. The table lists
  Anthropic's models only for now, so OpenAI and Bedrock calls stay `unknown`. The model calls themselves keep no
  provider label.
- These labels are the providers' own. The metrics above also estimate a cost from `docs/prices.toml` where no
  provider reported one.
- `api_equivalent_estimate`: Claude Code's own `total_cost_usd`. It is what the same tokens would cost at API
  prices, not what a subscription pays.
- `list_price_estimate`: shaked's own estimate of an agent's calls at `docs/prices.toml` prices: an API-mode agent's
  on a provider that reports no price, and a turn stopped before Claude Code reported its cost (a spend limit, a
  time limit); `api_equivalent_estimate+list_price_estimate`
  when earlier turns had reported theirs. See [Spend limits](#spend-limits).
- No label and no dollar figure: Codex reports tokens only.
- Dollars are shown only when every agent in the branch has a dollar figure; otherwise only tokens are shown,
  and the UI writes the cost as `unknown` when an agent of the branch is on OpenAI or Anthropic (else `tokens
  only`). Tokens are always summed. When agents have different sources the label is `mixed`.
