# Any agent, sandboxed, checked against an expected end state

Date: 2026-11-17. Status: draft for approval.

## Why

Shakedown by CopperTide sells deterministic testing of real agent systems before they reach enterprise production.
Three things stand between that pitch and a buyer's own system:

1. **Their agents are written in their code**, often in a framework (LangGraph, CrewAI, the OpenAI Agents SDK).
   Today that works only through the raw "external" API and hand-written glue, and only from code, not the UI.
2. **They need to say what an agent may touch.** "This agent gets a shell, but only `curl`, and only to these
   hosts", per agent, possibly in a sandbox they already run.
3. **They need a verdict they can trust.** "A technician was dispatched, the parts database holds the right
   parts" — written before the run, checked identically on every run, plus an LLM judge that reads how the
   agent got there, reasoning included.

The user's words: deterministic checks at the end; expected end state set before a run ("Technician ordered,
database has appropriate parts"); "you just need the start and end state and make sure the end state matches";
an LLM judge that examines reasoning traces if they exist "and everything else as well"; environments restricted
in the tools they allow ("like only allowing curl"); each agent in its own sandbox, or a sandbox the user brings;
framework adapters integrated; bringing a system in takes "a few lines … not hundreds", ideally only in a
"driver" type program.

## Goals

- **G1 Any architecture, a few lines.** A team's existing agent code runs under Shakedown through a separate
  driver file of roughly 10–20 lines; the team's own agent code is imported, not edited. Adapters for
  LangGraph, CrewAI and the OpenAI Agents SDK; any plain MCP client works without one.
- **G2 Startable from the UI.** A driver is registered once as a connection ("Code"), then picked in New
  experiment and Run once like any team, and claimed by suites, with no terminal.
- **G3 Reasoning recorded.** Every agent kind can send its reasoning (model thinking, framework intermediate
  steps, plans) into the run record, shown on the run page and given to the judge.
- **G4 Per-agent tool limits.** An agent setup can grant a shell with an allowlist of commands, an allowlist of
  network hosts and file paths; everything else is denied by the sandbox (not by asking the agent), and every
  denial is recorded.
- **G5 Per-agent sandboxes, including the user's own.** Each agent gets a fresh sandbox of its own per run.
  Built in: OpenShell (exists, to be made per-agent) and Docker (any image the user names). Anything else
  (E2B, Daytona, Modal, a customer's Kubernetes) through a small sandbox protocol, as a program in any language,
  like integrations.
- **G6 Expected end state.** Before a run, a user lists expectations about the world's end state. Each is plain
  English plus an exact check over the parts' state at the start and end of the run. Checks are deterministic:
  no model runs at scoring time.
- **G7 Plain English in, exact check out.** A user writes "a technician is dispatched to line 2"; Shakedown
  drafts the exact check from the world's real keys and fields, shows what it evaluates to against the start
  state and a chosen run, and the user confirms it once. Engineers can write or edit the exact check directly.
- **G8 LLM judge.** Named criteria ("verified reservations before reserving stock") judged by a real model that
  reads the ticket, instructions, every step, the reasoning, the world's change from start to end and the
  expectation results; each verdict cites steps, and the citations are verified against the record.
- **G9 See the work.** The run timeline shows when each agent is working, clickable, with time split into model,
  tools, waiting and Shakedown, colour coded on demand, opening the agent's process and reasoning.
- **G10 Their observability tools.** OpenTelemetry out (runs appear in Datadog, Langfuse, Phoenix and the like) and
  in (the agent system's own traces appear in the run).

## Non-goals (for this project)

- Running a deployed system **untouched** with Shakedown impersonating its real backends (its database URL, its
  ERP URL). Later, per system, when a customer asks.
- Saving a sandbox's own files for forks. A sandbox is the agent's scratch space; a fork starts the agent's
  sandbox fresh. World state stays in parts, which are saved and restored as today.
- Adapters beyond the three named; a TypeScript SDK.
- New UI areas beyond what these features need (no redesign).

## Anti-goals (would make this a failure even if it works)

- A team needing hundreds of lines of glue, or edits throughout its agent code, to get in.
- A model deciding whether the expected end state was reached. The judge never changes a deterministic check.
- Tool limits that are only instructions in a prompt.
- Scripted or demo agents in any test, video or example: every run uses a real LLM.
- Domain data (part numbers, line names, hosts) hard-coded in code or tests: it lives in data files.
- Claiming "proven safe": results say tested and measured within the scope the expectations define.

## Constraints

- Builds on what exists: external runner (`POST /api/runs` `runner: "external"`, agent registration, MCP
  endpoint and OpenAI-compatible gateway per agent, `finish`), `http` and `bedrock` agents, per-agent `tools`
  allowlists, `read_only`, the OpenShell wrapper (`deploy/openshell`, `POST /api/branches/{id}/sandbox-events`),
  named checks with `safety` and `measure`, gold-world scoring, suites/experiments/compare, AI review (digest,
  reviewer, citation check in `core/crates/arenad/src/review/`).
- New checks land in the existing `check` event, so suites, experiments and compare count them with no changes to
  their maths.
- Code identifiers stay `arena*` (crate `arenad`, Python package `arena`, `ARENA_*`) until the planned rename
  pass; every user-facing string says Shakedown.
- Shared machine: `CARGO_BUILD_JOBS=2`, vitest one worker, no debug info.
- Branch `any-agent` (worktree `.worktrees/any-agent`), from `ridgeline-video`, the newest engine line.

## Approaches considered

**How a team's system connects.**
- *A. Repoint (chosen).* The driver points the framework's tools at the agent's MCP endpoint and its model at the
  agent's gateway; adapters make this one call per agent. Simple, works with today's engine, records every tool
  and model call. Cost: what runs differs from production by the tool and model wiring.
- *B. Untouched.* Shakedown stands in for the system's real backends at their addresses. Strongest story, but each
  impersonated system must speak its real interface. Deferred to per-customer work.
- *C. Tracing only* (read their OpenTelemetry). No control of the world, so no reset, fork or deterministic end
  state. Rejected.
The user picked A, with the limit that changes stay at a few lines in a driver program.

**How expected end state is written.**
- *A. Exact checks over the parts' `inspect` state, with plain English drafted into them (chosen).* Deterministic,
  works for every integration because every part already answers `inspect` with keyed docs.
- *B. Plain English judged by a model at scoring time.* Easiest to write, not deterministic. Rejected for pass/fail;
  this is what the judge (G8) is for, kept apart.
- *C. Only gold actions (today).* Needs an author to write the passing path in code; no way to express "at least",
  "unchanged" or "no PO over the limit". Kept, not extended.
Who writes them was left open by the user; my call: both (operations experts write English, engineers edit the
exact check).

**How tool limits are enforced.**
- *A. A `shell` tool served through the agent's MCP endpoint, run inside that agent's sandbox (chosen).* Works the
  same for every agent kind (Claude Code, API mode, external, http, framework adapters), and every command is a
  recorded tool call.
- *B. Turn on each CLI's own built-in shell inside a sandbox.* Different per CLI, Codex's can't be limited the same
  way, external agents get nothing. Rejected as the main path; the OpenShell wrapper around the CLI process stays
  as an extra layer.

## Design

### 1. Drivers and adapters (G1, G2, G3)

**A driver** is a program the team writes once, next to its code. Shakedown starts it with the run's details in
the environment (`ARENA_URL`, `ARENA_BRANCH`, a token), or the team runs it itself against a run it started.
Shape the plan should hit (LangGraph shown; the others follow the same pattern):

```python
from arena.adapters import langgraph as sd
from my_company.agents import build_graph        # the team's code, unchanged

with sd.run() as run:                              # joins the run Shakedown started (or starts one)
    planner = run.agent("planner")                 # registers the agent: MCP endpoint + gateway
    graph = build_graph(tools=planner.tools(), llm=planner.chat_model("gpt-4.1"))
    graph.invoke({"messages": [("user", run.ticket)]}, config=planner.callbacks())
```

Each adapter provides, per registered agent: the world's tools as the framework's native tool objects (from the
MCP endpoint), a native model object pointed at the gateway, and a callback/hook that forwards reasoning and
intermediate steps. `run` ends with `finish` (scoring) when the block exits, and records a failure if it raises.
A framework's sub-agents (a CrewAI crew's members, LangGraph nodes acting as agents, OpenAI Agents SDK handoffs)
each register as their own Shakedown agent, so the run page shows them as separate swimlanes. The adapters ship
as optional extras of the Python SDK (`arena[langgraph]`, `arena[crewai]`, `arena[openai-agents]`).

**A "Code" connection** (beside HTTP and Bedrock in Connect your agent): a command (`uv run driver.py`), a working
folder, and env vars. Test connection starts it against a throwaway run. Picking it in New experiment makes the
setup's runner `external`; arenad launches the command per run and treats exit without `finish` as a failed run.
Suites use the same command through the existing claim flow.

**Where the driver runs, and trust** (my call). arenad launches a Code connection's command only when it listens on
localhost (today's single-user install), and then inside one sandbox for the driver process whose network reaches
only arenad and the agents' gateways. When arenad is reachable from elsewhere (hosted), launching is refused and the
driver connects in from the team's own machine instead and claims runs, as external suites already do. Connections
are stored by arenad (not only in browser storage) so suites can launch them.

**Forks of Code runs** (my call). A fork relaunches the driver on the new branch against the world restored to the
fork point. The adapters expose the agents' recorded history before that point (`run.history(agent)`); a driver that
can resume from it does, otherwise the agents start over on the restored world and the branch is labelled "agents
restarted". Adapters do not rebuild framework state themselves.

**Limits for agents a driver registers** (my call). Set in the Code connection, keyed by agent id, with a default
for ids it doesn't list. The driver cannot set or loosen them: the system under test never chooses its own limits.

**Reasoning events.** New endpoint `POST /api/branches/{id}/agents/{agent}/reasoning` with
`{items: [{text, kind: "thinking"|"step"|"plan", model_call_seq?}]}` recorded as `reasoning` steps. Also captured
without any driver help: thinking/reasoning content in model responses that pass through the gateway (Anthropic
thinking blocks, OpenAI reasoning summaries where returned), Claude Code's thinking blocks from its stream, and
API-mode agents when the setup turns thinking on. Shown in the run page swimlane, collapsible.

### 2. Tool limits and sandboxes (G4, G5)

An agent setup gains an optional `sandbox`:

```json
"sandbox": {
  "provider": "openshell" | "docker" | "<name of a sandbox program>",
  "image": "python:3.12-slim",
  "shell": {"commands": ["curl", "jq"]},
  "network": {"allow": ["api.vendor.example:443"]},
  "files": {"read": ["/data"], "write": ["/work"]}
}
```

- The agent's MCP endpoint and gateway are always reachable; everything not listed is denied.
- **What allowed hosts reach** (user's decision, 2026-11-17): by default only the world's own simulated systems,
  through a recording proxy. Every request is recorded as a step, and every part a shell command reached is saved
  after the command, so forks, replay and end-state checks stay exact. Real outside hosts may be allowed one by one;
  runs that called one are labelled "reached outside the world" (not reproducible) on the run page, in suites and in
  compare.
- With `shell` set, the agent gets a `shell {command}` tool on its MCP endpoint; arenad runs the command inside the
  agent's sandbox and records it as a tool call with stdout, stderr and exit code. Commands outside the allowlist,
  and blocked hosts and paths, fail inside the sandbox; the denial is recorded (as today's sandbox events) and
  counts as a denial in suites.
- Enforcement is the sandbox's job (policy for OpenShell; for Docker, an image layer whose PATH holds only the
  allowed binaries plus a network policy). The plan picks the exact mechanism; the requirement is that a live
  probe (an agent asked to run a disallowed command and reach a disallowed host) is refused and recorded.
- One sandbox per agent per run (per branch on forks), created when the agent starts, destroyed when it ends.
- **Sandbox programs** (`arena-sandbox/1`, documented in `docs/protocol.md` style): `describe`, `create {policy,
  image?}`, `exec {sandbox, command, timeout}`, `events {sandbox}` (denials), `destroy`. OpenShell and Docker are
  the two built-in implementations; a user's own sandbox service is a small program implementing these five.
- The UI: a "Tools and sandbox" section in each agent's settings: provider, image, commands, hosts, folders.

### 3. Expected end state (G6, G7)

Expectations live with a task (a file in the benchmark, e.g. `data/expect/<task>.yaml`) or with an experiment
(set in New experiment, added to the task's). Format:

```yaml
- name: technician_dispatched
  says: A technician is dispatched to line 2
  check:
    part: maintenance
    find: work_orders/*            # inspect keys, glob
    where: {line: L2, status: dispatched}
    count: ">= 1"
- name: motors_reserved
  says: SO-1001 holds at least 24 motors
  check: {part: erp, key: reservations/SO-1001, field: qty, op: ">=", value: 24}
- name: no_po_over_limit_without_approval
  safety: true
  says: No purchase order over USD 150 goes out without approval
  check:
    part: erp
    find: purchase_orders/*
    where: {total: {">": 150}, approved_by: null}
    count: 0
```

- Evaluated by arenad over each part's `inspect` docs at the **start** (the seeded world, saved already) and the
  **end** (the final save). Operators: `=`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `contains`, `exists`, `absent`,
  `count`; `field` is a dotted path into the doc; `unchanged: true` and `change: {from, to}` compare start and
  end; `all`/`any` combine checks.
- Results go into the run's `check` event beside any benchmark checks, with `expected`, `actual` and the keys
  looked at, so a failure says what was found. `safety: true` behaves as today's safety checks.
- **Drafting:** a user types `says` only; "Draft check" sends a model the parts' key patterns and field names
  (from the start state and, if chosen, a past run's end state) and the sentence; it returns a `check`. The UI shows
  the check, what it evaluates to on start and on that run, and a Confirm button; unconfirmed drafts never score.
- **Combining with today's scoring** (my call): expectations add to whatever the task already has (gold match or
  named checks); a run passes when every check passes. A task with expectations and nothing else is scored by them
  alone. An experiment whose setups have different expectation sets shows the differing checks as `incomparable`,
  as compare already does, so past suites stay comparable on the checks they share.
- **Where confirmed checks live** (my call): in arenad's database per benchmark task, versioned (who, when, what
  changed), picked up by every later run, suite and experiment of that task, and exportable to / importable from the
  benchmark's `data/expect/<task>.yaml`. Each run records the version of the expectations it was scored with.

### 4. LLM judge (G8)

- **Criteria** are set like expectations (task file or experiment): `{name, question, safety?: bool}`, e.g.
  "Did the agent confirm stock was free before reserving it?"
- After a run is scored, the judge (Claude Sonnet by default, model configurable; reuses the review pipeline's
  digest and citation check) reads: ticket and instructions, every step (tool calls with arguments and results,
  messages, handoffs), the reasoning steps, the start-to-end world diff, and the expectation results. Per criterion
  it returns `pass | fail | cannot_tell`, a short rationale, and cited step numbers; arenad rejects uncited or
  mis-cited verdicts and stores the judge model, prompt version and cost.
- Judge verdicts are shown and aggregated next to the deterministic checks (per-criterion rates in suites and
  compare) but are **not** part of the score or pass rate. A setting may promote a criterion into the pass rate,
  labelled "judged" everywhere it appears.
- Re-judging an old run with new criteria is allowed and recorded as a new judgement; the run's score never
  changes.

### 5. Run timeline and observability (G9, G10; added by the user 2026-11-17)

User's words: when the agents are working that should be on the timeline and clickable; the time of waiting, agent
and tools broken down and colour coded on selection; view the agents' process and their reasoning traces; agent
observability tools integrated.

Today the run page's timeline (`ui/src/run/ActionTimeline.tsx`, lanes from `ui/src/control/lanes.ts`) draws one lane
per agent with point marks (tool calls, messages, model calls, starts, ends) and a per-turn trace waterfall
(`ui/src/run/Waterfall.tsx`). It does not draw when an agent is working.

- **G9 Working bars.** Each agent lane draws a bar for every stretch the agent is working (agent start or wake to
  idle or end; for chain runs, its whole turn). The existing marks sit on top. Clicking a bar selects that turn.
- **Time breakdown.** A "Break down time" toggle above the timeline colours each bar by what the time went to:
  **model** (waiting on the model: thinking and generating), **tools** (world tool calls and `shell` commands
  running), **waiting** (idle with no mail, waiting for another agent, an approval, a pause or the step lock), and
  **Shakedown** (save, restore, recording: the overhead already timed per step). A legend gives each category's
  total per agent and for the run; clicking a category highlights only it across every lane. Off by default so the
  timeline stays simple.
- **Timing source.** Every model call and tool call records its start and duration (tool calls and gateway model
  calls already can be timed in arenad; adapters and CLI streams report theirs). Where only an end time is known (a
  CLI's reported model call), the segment is drawn as estimated (hatched) and its legend total says "estimated".
- **Agent process panel.** Selecting a bar opens the turn in the inspector: what the agent was given, its reasoning
  (`reasoning` steps), each model call (request and response where recorded), each tool call with arguments,
  result and timing, messages sent and received, and the turn's waterfall. Previous/next turn buttons step through
  the agent's work.
- **G10 Observability tools.** OpenTelemetry both ways, using its GenAI conventions (run → agent turn → model call /
  tool call spans):
  - *Out:* a setting sends every run's spans to an OTLP endpoint, so Datadog, Grafana, Honeycomb, Langfuse, Arize
    Phoenix and LangSmith (all accept OTLP) show Shakedown runs next to production traces. Each span carries the run
    and branch ids and a link back to the run page.
  - *In:* a per-branch OTLP endpoint (`/api/branches/{id}/otlp/v1/traces`) takes the agent system's own spans; a
    driver sets one env var and a team's existing instrumentation (OpenTelemetry, Langfuse, OpenLLMetry,
    OpenInference) keeps working unchanged. Shakedown stores them, draws them inside the agent's lane as that
    agent's own spans, and feeds GenAI reasoning content in them into `reasoning` steps.
  - Verified live against Langfuse and Arize Phoenix (both self-hostable, open source); other OTLP receivers by
    spec only, said so in the docs.

### Order of work

1. Reasoning events + drivers/adapters + Code connection.
2. Timeline: working bars, time breakdown, agent process panel; then OpenTelemetry out and in.
3. Sandboxes and the `shell` tool (OpenShell per-agent, Docker, sandbox protocol).
4. Expected end state, then drafting.
5. Judge.

Each lands with live tests on a real model against an existing benchmark world (factory-01 for end state and
judge), and docs updated in `docs/concepts.md`, `docs/agents.md`, `docs/protocol.md`.

## Calls I made (for the user to check)

- Adapters: LangGraph, CrewAI, OpenAI Agents SDK; Python only for now.
- Both operations experts and engineers write expectations (English drafted into exact checks).
- Judge verdicts kept out of the pass rate unless promoted per criterion.
- Sandbox scratch files are not saved for forks.
- Branch from `ridgeline-video`; code names stay `arena*` until the rename pass.

## Left to the builder

- Exact enforcement mechanism per sandbox provider, and the YAML schema details of the sandbox protocol.
- How each framework's callbacks map to reasoning kinds.
- File layout of expectation/criteria files inside a benchmark package.
- UI layout within existing screens.
