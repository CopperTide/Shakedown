# Part 1: Drivers, framework adapters and reasoning — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A team's existing LangGraph, CrewAI or OpenAI Agents SDK code runs in Shakedown through a 10–20 line driver. Shakedown can start the driver itself from a saved "Code" connection in single runs, experiments and forks. Every agent's reasoning is recorded in the run.

**Architecture:** Reasoning becomes a new event kind, `reasoning`. Drivers report it explicitly, and shaked also pulls it out of the model responses it already records (from the gateway and from Claude Code's stream). Drivers are ordinary external-runner programs. A thin Python `shakedown.driver` module joins or starts the run, and per-framework adapters turn an agent's MCP tools and gateway into native framework objects. shaked stores Code connections, spawns the driver per run with the run's ids in its environment, and ends the run `failed` (scored) if the driver exits without finishing.

**Tech Stack:** Rust (shaked: axum, tokio, tokio-postgres), Python ≥3.10 SDK (httpx; optional extras for langchain-core/langchain-openai/langgraph, crewai, openai-agents), React/TypeScript UI (vitest).

**Spec:** `docs/superpowers/specs/2026-11-17-any-agent-sandboxes-expected-state-design.md`, section 1. Parts 2–5 get their own plans.

## Global Constraints

- Shared machine: `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, vitest with `--maxWorkers=1`. Clean only your own build output.
- Code identifiers stay `shakedown*` (`shaked`, the Python package `shakedown`, `SHAKEDOWN_*` env vars). Every user-facing string says **Shakedown**.
- No scripted or demo agents in any test, example or doc. A live test uses a real model and skips, saying why, when no key is set.
- No domain data (part numbers, hosts, task ids) hard-coded in code or tests. Live tests take the task from the benchmark (`GET /api/tasks`).
- A driver for one framework fits in about 20 lines and never edits the team's agent code.
- shaked launches drivers only because it binds `127.0.0.1` alone (`core/crates/shaked/src/main.rs:147`). The launcher must not be reachable any other way.
- Docs: update `docs/agents.md` and `docs/sdk.md` in the task that adds the behaviour, in the existing plain style.

## Review Focus

1. **The driver crashes or hangs.** If the process exits non-zero, or exits zero without calling `finish`, the run ends `failed`, scored, with the exit code and the tail of stderr. If it still runs 30 s after the branch ends, it is killed. (Task 6 tests both.)
2. **Reasoning text is huge or empty.** Items over 32 768 characters are cut and marked `truncated: true`. Empty or whitespace-only items are dropped. More than 100 items in one call is a 400. (Task 1.)
3. **A model response carries reasoning in a provider-specific shape.** Covers `reasoning_content`, a `reasoning` string, `reasoning_details[]` and Anthropic `thinking` blocks. `redacted_thinking` is never recorded. (Task 2 tests each.)
4. **A tool returns an error or a refusal.** Every adapter hands the agent the error text (`error: …`) and doesn't raise, so the refusal is recorded and the agent can react. (Task 4 tests each adapter.)
5. **A JSON schema the adapter can't map cleanly** (nested object, array of objects, enum, missing `type`). CrewAI's pydantic model accepts it as `dict`/`list`/`Any` instead of failing at import. (Task 4.)

---

### Task 1: The `reasoning` event and its endpoint

**Files:**
- Modify: `core/crates/shaked/src/model.rs:247-320` (add `EventKind::Reasoning`, `"reasoning"`)
- Create: `core/crates/shaked/src/reasoning.rs` (types and payload; extraction functions come in Task 2)
- Modify: `core/crates/shaked/src/lib.rs` (`pub mod reasoning;`)
- Modify: `core/crates/shaked/src/api/external.rs` (route `POST {agent}/reasoning`)
- Modify: `ui/src/types.ts:42-68` (add `'reasoning'` with its doc comment), `sdk/python/src/shakedown/models.py` (its event-kind literal, if it lists kinds), `sdk/python/src/shakedown/SCHEMA.md`
- Modify: `docs/protocol.md` / `docs/concepts.md` (External designs: step 3 gains the reasoning call)
- Test: `core/crates/shaked/tests/external.rs`

**Interfaces:**
- Produces:
  - `pub enum ReasoningKind { Thinking, Step, Plan }` with `#[serde(rename_all = "lowercase")]`, default `Thinking`.
  - `pub struct ReasoningItem { pub text: String, #[serde(default)] pub kind: ReasoningKind, #[serde(default)] pub model_call_seq: Option<i64> }`
  - `pub struct ReasoningReport { pub items: Vec<ReasoningItem> }`
  - `pub const MAX_ITEMS: usize = 100; pub const MAX_TEXT: usize = 32_768;`
  - `pub fn payload(item: &ReasoningItem, source: &str) -> Option<Value>`: `None` for blank text. Otherwise `{text, kind, source, model_call_seq?, truncated?}`, where text is cut to `MAX_TEXT` chars on a char boundary and `truncated: true` is set when cut.
  - Sources: `pub const SOURCE_AGENT: &str = "agent"; SOURCE_GATEWAY = "gateway"; SOURCE_CLAUDE_CODE = "claude_code";`
  - HTTP: `POST /api/branches/{id}/agents/{agent}/reasoning` body `ReasoningReport`. Answers `{seqs: [i64]}` (one per recorded item, blanks skipped). 400 for 0 items, more than `MAX_ITEMS`, or an unknown `kind`. 404 for an unknown agent. 409 for a branch shaked runs itself (same rule as `usage`).

- [ ] **Step 1: Write the failing tests** in `tests/external.rs` (non-live, next to `only_external_runs_take_registrations`):

```rust
#[test]
fn an_agent_reports_its_reasoning() {
    // start an external run, register agent "a", then:
    // POST .../agents/a/reasoning {"items":[{"text":"Check stock first.","kind":"plan"},{"text":"   "},{"text":"x".repeat(40000)}]}
    // -> 200, seqs.len() == 2
    // events of kind Reasoning by agent "a": payloads
    //   [0] == {"text":"Check stock first.","kind":"plan","source":"agent"}
    //   [1]["truncated"] == true, [1]["text"].chars().count() == 32768, [1]["kind"] == "thinking"
}

#[test]
fn reasoning_reports_are_checked() {
    // {"items":[]} -> 400; 101 items -> 400; {"items":[{"text":"t","kind":"musing"}]} -> 400
    // agent "nobody" -> 404; a chain run's branch -> 409
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cd core && CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 cargo test -p shaked --test external reasoning`
Expected: FAIL (404 on the route, or `EventKind::Reasoning` not found).

- [ ] **Step 3: Implement** the enum variant (`as_str`/`parse`), `reasoning.rs` as specified, and the route handler `report_reasoning`. The handler follows `report_usage`: validate, then `ctx.append(..., EventKind::Reasoning, payload)` once per item, in order.

- [ ] **Step 4: Run them and see them pass.** Also run `cargo test -p shaked --lib` to check nothing that matches on `EventKind` broke. Fork/replay/digest code that matches exhaustively must treat `Reasoning` as a non-world step, like `Message`.

Expected: PASS.

- [ ] **Step 5: Commit** `git commit -m "Record agents' reasoning: reasoning events and POST …/agents/{agent}/reasoning"`

### Task 2: Reasoning pulled from recorded model calls

**Files:**
- Modify: `core/crates/shaked/src/reasoning.rs` (extraction)
- Modify: `core/crates/shaked/src/gateway.rs:160-170` (`record`: after appending the `ModelCall`, append its reasoning)
- Modify: `core/crates/shaked/src/agents/claude_session.rs:201-213` (`on_line`: an `assistant` message's blocks)
- Test: unit tests in `reasoning.rs`, plus one assertion added to the existing live Claude Code test in `tests/cli.rs`

**Interfaces:**
- Consumes: Task 1 `ReasoningItem`, `payload`, sources.
- Produces:
  - `pub fn from_chat_response(canonical: &Value, provider_raw: Option<&Value>) -> Vec<ReasoningItem>`. Checks, in order:
    - each `choices[*].message.reasoning_content` (string);
    - `.reasoning` (string);
    - `.reasoning_details[*]` (`text` or `summary`);
    - then, only if none of those were found, `provider_raw.content[*]` blocks with `type == "thinking"` (`thinking` field).

    All come out as `kind: Thinking`. `redacted_thinking` is skipped.
  - `pub fn from_claude_message(message: &Value) -> Vec<ReasoningItem>`: `message.content[*]` with `type == "thinking"` becomes `Thinking`, and `type == "text"` becomes `Step`. Other blocks are skipped.
  - The gateway's reasoning events carry `model_call_seq` = the seq of the `ModelCall` just appended, with source `gateway`. Claude Code's carry the seq of the `ModelCall` recorded for that message when there is one, with source `claude_code`.

- [ ] **Step 1: Write the failing unit tests** in `reasoning.rs`:

```rust
#[test] fn deepseek_style_reasoning_content_is_thinking() { /* choices[0].message.reasoning_content = "r" -> [Thinking "r"] */ }
#[test] fn openrouter_reasoning_string_and_details() { /* reasoning:"a", reasoning_details:[{text:"b"},{summary:"c"}] -> ["a","b","c"] */ }
#[test] fn anthropic_thinking_blocks_from_the_raw_response() { /* canonical has none; raw content [{type:"thinking",thinking:"t"},{type:"redacted_thinking",data:"x"},{type:"text",text:"hi"}] -> [Thinking "t"] */ }
#[test] fn no_reasoning_gives_nothing() { /* plain {choices:[{message:{content:"hi"}}]} -> [] */ }
#[test] fn claude_code_thinking_and_text_blocks() { /* [{type:"thinking",thinking:"plan"},{type:"tool_use",…},{type:"text",text:"Reserving now."}] -> [Thinking "plan", Step "Reserving now."] */ }
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --lib reasoning::`. Expected: FAIL (functions missing).

- [ ] **Step 3: Implement** both functions and wire them in. In the gateway, a failure to append reasoning is logged with `eprintln!`, like `record`, and never fails the model call. In `on_line`, reasoning is recorded after the message's `ModelCall`.

- [ ] **Step 4: Run them and see them pass.** Then run the live Claude Code test with the new assertion (at least one `reasoning` event with `source == "claude_code"` and `kind == "step"`): `cargo test -p shaked --test cli -- --ignored live_`. It skips without a signed-in `claude`.

- [ ] **Step 5: Commit** `git commit -m "Reasoning from model responses: gateway (reasoning_content, reasoning, details, thinking blocks) and Claude Code's stream"`

### Task 3: `shakedown.driver`, the few-lines entry point

**Files:**
- Create: `sdk/python/src/shakedown/driver.py`
- Modify: `sdk/python/src/shakedown/client.py:599-670` (`AgentEndpoint.report_reasoning`), `sdk/python/src/shakedown/__init__.py` (export `driver`)
- Modify: `core/crates/shaked/src/api/external.rs` (`finish` accepts an optional body `{"error": "…"}`), `core/crates/shaked/src/runner.rs` (new `finish_failed`)
- Modify: `docs/sdk.md` (section "Drivers")
- Test: `sdk/python/tests/test_units.py`, `sdk/python/tests/test_core.py`, `core/crates/shaked/tests/external.rs`

**Interfaces:**
- Produces (Rust):
  - `pub async fn finish_failed(state: &AppState, ctx: &BranchCtx, why: String) -> Option<Ending>` in `runner.rs`. It works like `finish` (claim, freeze, score, so the `check` event is recorded), but a scored ending becomes `(BranchStatus::Failed, Some(why))`. An infra error stays an infra error.
  - `POST /api/branches/{id}/finish` with `{"error": "…"}` calls it. An empty body keeps today's behaviour.
- Produces (Python):
  - `AgentEndpoint.report_reasoning(text: str | None = None, kind: str = "thinking", model_call_seq: int | None = None, items: list[dict] | None = None) -> list[int]`
  - `class DriverRun` with:
    - `.shakedown: Shakedown`, `.branch: Branch`, `.task: Task`
    - `.ticket: str` (`task.ticket`, plus `"\n\n" + task.instructions` when there are instructions)
    - `.agent(id: str, *, provider: str | None = None, model: str | None = None, name: str | None = None) -> AgentEndpoint`: registers once, then returns the cached endpoint
    - `.history(agent_id: str) -> list[Event]`: the branch's events by that agent with `seq < record.start_seq`; empty when the branch is not a fork
    - `.is_fork: bool`
  - `driver.run(task: str | None = None, url: str | None = None) -> ContextManager[DriverRun]`:
    - With `SHAKEDOWN_BRANCH` set (shaked launched the driver), it joins that branch. `SHAKEDOWN_URL` and `SHAKEDOWN_TASK` come from the environment.
    - Otherwise it starts `shakedown.external_run(task or env SHAKEDOWN_TASK or the benchmark's first task)`.
    - On a clean exit it ends every registered agent (`end(ok=True)`) that the driver hasn't ended, then calls `branch.finish()`.
    - On an exception it ends them with `ok=False, error=repr(exc)`, calls `finish` with `{"error": f"driver raised {type(exc).__name__}: {exc}"}`, and re-raises.
- Env contract shaked sets (Task 6 relies on it): `SHAKEDOWN_URL`, `SHAKEDOWN_RUN`, `SHAKEDOWN_BRANCH`, `SHAKEDOWN_TASK`, and on forks `SHAKEDOWN_FORK_OF`.

- [ ] **Step 1: Write the failing tests.**
  - Rust (`tests/external.rs`, non-live): `finish_with_an_error_ends_failed_and_scored`. Register an agent, `POST /finish {"error":"driver raised X"}`, then assert the branch status is `failed`, its error is `"driver raised X"`, and its events contain a `Check`.
  - Python unit (`test_units.py`, with `httpx.MockTransport`):
    - `test_driver_joins_the_branch_shaked_launched`: with env `SHAKEDOWN_URL`/`SHAKEDOWN_BRANCH`/`SHAKEDOWN_TASK`, `run()` makes no `POST /api/runs`, `.agent("a")` POSTs `/api/branches/<b>/agents` once even when called twice, and exiting POSTs `…/agents/a/end` and then `…/finish`;
    - `test_driver_failure_finishes_with_the_error`: raising `ValueError("boom")` inside the block POSTs `/finish` with `{"error": "driver raised ValueError: boom"}` and re-raises;
    - `test_report_reasoning_posts_items`.
  - Python core (`test_core.py`, `@pytest.mark.core`): `test_driver_round_trip` against a real shaked with a real model through the gateway. It uses the `bring_your_own_agent.py` gateway loop, reads `run.ticket`, and asserts the branch ends with a `check` event. It skips without a provider key.

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test external finish_with_an_error`; `cd sdk/python && uv run --with pytest pytest -q tests/test_units.py -k "driver or reasoning"`. Expected: FAIL.

- [ ] **Step 3: Implement** `finish_failed`, the finish body, `report_reasoning`, and `driver.py` per the interfaces.

- [ ] **Step 4: Run them and see them pass** (including `pytest -q -m core tests/test_core.py -k driver` with shaked up).

- [ ] **Step 5: Commit** `git commit -m "shakedown.driver: join or start a run in a few lines; finish with an error ends failed, scored"`

### Task 4: Framework adapters, with an example driver each

**Files:**
- Create: `sdk/python/src/shakedown/adapters/__init__.py`, `_schema.py`, `langgraph.py`, `crewai.py`, `openai_agents.py`
- Create: `sdk/python/examples/drivers/langgraph_driver.py`, `crewai_driver.py`, `openai_agents_driver.py` (each about 20 lines or fewer, building a small team in that framework's usual style)
- Modify: `sdk/python/pyproject.toml` with `[project.optional-dependencies]`:
  - `langgraph = ["langgraph>=0.2", "langchain-core>=0.3", "langchain-openai>=0.2"]`
  - `crewai = ["crewai>=0.80"]`
  - `openai-agents = ["openai-agents>=0.0.17"]`
- Modify: `docs/agents.md` (new section "Your own code: drivers and adapters", with the LangGraph driver inline)
- Test: `sdk/python/tests/test_adapters.py` (unit, fake endpoint) and `sdk/python/tests/test_adapters_live.py` (`core`, real model)

**Interfaces:**
- Consumes: `AgentEndpoint` (`.tools: list[Tool]`, `.call(name, args) -> ToolResult` with `.text` and `.is_error`, `.gateway_url`, `.report_reasoning`), and `driver.run`.
- Produces:
  - `_schema.model_from_schema(name: str, schema: dict) -> type[pydantic.BaseModel]`. Maps `string`/`integer`/`number`/`boolean` to their types. Arrays become `list`, objects `dict`, and enums the base type. Anything else becomes `Any`. A property not in `required` becomes `Optional[...] = None`.
  - `_schema.result_text(r: ToolResult) -> str`: `r.text` if ok, else `"error: " + r.text`.
  - `langgraph.tools(agent) -> list[langchain_core.tools.StructuredTool]`, with `args_schema` = the MCP JSON schema dict.
  - `langgraph.chat_model(agent, model: str, **kw) -> langchain_openai.ChatOpenAI`, with `base_url=agent.gateway_url` and `api_key="shakedown"` (the gateway holds the real key).
  - `crewai.tools(agent) -> list[crewai.tools.BaseTool]`, `crewai.llm(agent, model: str, **kw) -> crewai.LLM` (`model=f"openai/{model}"`, `base_url`, `api_key="shakedown"`), and `crewai.step_callback(agent) -> Callable[[Any], None]`. The callback records an `AgentAction`'s `.thought` (and `.text` when there's no thought) as `kind="step"`.
  - `openai_agents.tools(agent) -> list[agents.FunctionTool]` (`params_json_schema` = the MCP schema, `strict_json_schema=False`, async `on_invoke_tool` running `agent.call` in a thread), and `openai_agents.model(agent, model: str) -> agents.OpenAIChatCompletionsModel` over `openai.AsyncOpenAI(base_url=agent.gateway_url, api_key="shakedown")`.
  - Each module re-exports `run = shakedown.driver.run`, so a driver needs one import.
  - A framework's sub-agents each call `run.agent("<id>")`, one Shakedown agent each. The examples show it.

- [ ] **Step 1: Write the failing unit tests** in `test_adapters.py`. Each framework block starts with `pytest.importorskip`, and uses a `FakeEndpoint` with two tools: `get_stock {sku: string}` and `reserve {order: string, qty: integer, note?: string}`, where `reserve` returns `is_error=True, text="refused: exceeds the limit"`. Tests:
  - `test_schema_required_and_optional`: `reserve` gives a model with required `order: str` and `qty: int` and optional `note`;
  - `test_schema_odd_shapes_do_not_fail`: nested object, array of objects, enum, a property without a type;
  - `test_langgraph_tool_calls_the_endpoint`: invoking `get_stock` with `{"sku": "A"}` calls the fake with exactly those args and returns its text;
  - `test_langgraph_refusal_is_text`: `reserve` returns `"error: refused: exceeds the limit"` and doesn't raise;
  - the same pair for crewai (`_run`) and for openai_agents (`on_invoke_tool(ctx, '{"sku":"A"}')` awaited);
  - `test_chat_model_points_at_the_gateway` for each framework's model object;
  - `test_crewai_step_callback_records_thought`: posts `report_reasoning(kind="step")` with the thought.

- [ ] **Step 2: Run them and see them fail:** `cd sdk/python && uv run --with pytest --extra langgraph --extra crewai --extra openai-agents pytest -q tests/test_adapters.py`. Expected: FAIL (modules missing).

- [ ] **Step 3: Implement** the modules and the three example drivers. Check that each driver is ≤ 25 lines including imports: `wc -l examples/drivers/*.py`.

- [ ] **Step 4: Run the unit tests and see them pass.** Then run `test_adapters_live.py` (`-m core`, shaked up, `OPENAI_API_KEY` in shaked's environment). For each framework, it runs the example driver as a subprocess against the benchmark's first task and asserts:
  - the branch has `tool_call` events from every agent the driver registered;
  - it has a `check` event;
  - it has at least one `reasoning` event (gateway or step);
  - the driver exited 0.

  Each test skips, naming what's missing (the package or the key).

- [ ] **Step 5: Commit** `git commit -m "Adapters for LangGraph, CrewAI and the OpenAI Agents SDK, with a ~20-line driver each"`

### Task 5: Reasoning in the run page

**Files:**
- Modify: `ui/src/run/stepSummary.ts:103` (`stepSummary` handles `reasoning`), `ui/src/control/lanes.ts:102-115` (a `reasoning` mark, kind `message`, label `Reasoning` / `Plan` / `Step`, detail = text)
- Modify: `ui/src/run/Inspector.tsx` (a reasoning step shows its text in full, with `white-space: pre-wrap`, and a "from model call N" link when `model_call_seq` is set)
- Test: `ui/src/run/stepSummary.test.ts`, `ui/src/run/timeline.test.ts` (or the lanes test that sits next to `lanes.ts`)

**Interfaces:**
- Consumes: event `{kind: 'reasoning', agent, payload: {text, kind: 'thinking'|'step'|'plan', source, model_call_seq?, truncated?}}`.
- Produces: nothing new for later tasks. Part 2's process panel reads the same events.

- [ ] **Step 1: Write the failing tests:**
  - `stepSummary` of a reasoning event returns a title of `Reasoning` (or `Plan`/`Step` for those kinds) and a detail of the first 160 characters with `…` when cut;
  - a truncated event's detail ends with `(cut at 32,768 characters)`;
  - the lanes builder makes one mark per reasoning event, on its agent's lane, with `kind: 'message'`.

- [ ] **Step 2: Run them and see them fail:** `cd ui && npx vitest run --maxWorkers=1 src/run/stepSummary.test.ts src/run/timeline.test.ts`. Expected: FAIL.

- [ ] **Step 3: Implement** the code.

- [ ] **Step 4: Run them and see them pass,** plus `npx tsc --noEmit`.

- [ ] **Step 5: Commit** `git commit -m "Run page shows agents' reasoning"`

### Task 6: Code connections, launched by shaked

**Files:**
- Create: `core/crates/shaked/src/store/connections.rs` (table and CRUD, following `store/reviews.rs`), `core/crates/shaked/src/driver.rs` (launch and watch), `core/crates/shaked/src/api/connections.rs` (routes)
- Modify:
  - `core/crates/shaked/src/model.rs:133` (`Rules` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub connection: Option<String>`);
  - `core/crates/shaked/src/external.rs:47` (`start` takes `rules: Rules`; callers pass `Rules::new(Runner::External)` with `connection` set when there is one);
  - `core/crates/shaked/src/api/runs` (wherever `create_run` is reached: `POST /api/runs` accepts `connection`);
  - `core/crates/shaked/src/api/experiments.rs:38-70` (`SetupReq.connection: Option<String>`; an `External` setup is allowed only with a connection);
  - `core/crates/shaked/src/suite/mod.rs:38` (`Suite.connection: Option<String>` and `ALTER TABLE suites ADD COLUMN IF NOT EXISTS connection text`);
  - `core/crates/shaked/src/experiment/drive.rs:52` (`next_run`: when `suite.connection` is set, call `driver::start_slot` instead of `suite::start_builtin`).
- Modify: `docs/reference.md` (routes), `docs/agents.md` (Code connections)
- Test: `core/crates/shaked/tests/connections.rs` (new; fixture drivers are tiny Python scripts in `tests/fixtures/drivers/` run with `uv run` or `python3`; the live one uses a real model)

**Interfaces:**
- Consumes: Task 3's env contract and `finish_failed`. Also `external::start`, `store.claim_slot`, `store.fill_slot`, `suite::wait_ended`.
- Produces:
  - `pub struct Connection { pub id: String, pub name: String, pub command: Vec<String>, pub workdir: Option<String>, pub env: BTreeMap<String, String>, pub created_at: DateTime<Utc> }`. Only `kind = "code"` exists for now. Env values are stored but redacted (`"••••"`) in GET answers for keys matching `KEY|TOKEN|SECRET|PASSWORD`.
  - Routes:
    - `GET /api/connections` returns `[Connection]`;
    - `POST /api/connections` takes `{name, command, workdir?, env?}` and answers `Connection` (400 for an empty command or a workdir that doesn't exist);
    - `DELETE /api/connections/{id}`;
    - `POST /api/connections/{id}/test` starts a run of the benchmark's first task named `Connection test · <name>` with `max_cost_usd: 0.50`, waits up to 5 min, and answers `{ok, branch_id, status, agents: [id], tool_calls: n, error?, stderr_tail}`.
  - `pub async fn launch(state: &Arc<AppState>, conn: &Connection, run_id: &str, branch_id: &str, task_id: &str, fork_of: Option<&str>) -> Result<()>` in `driver.rs`:
    - spawns `command` in `workdir` with the conn env plus `SHAKEDOWN_URL=self_base`, `SHAKEDOWN_RUN`, `SHAKEDOWN_BRANCH`, `SHAKEDOWN_TASK` and `SHAKEDOWN_FORK_OF`;
    - keeps the last 8 KiB of stdout and stderr;
    - watches the process: an exit while the branch is still live calls `runner::finish_failed` with `"the driver exited with code N before finishing: <stderr tail>"`;
    - a process still alive 30 s after the branch ends is killed;
    - on exit, records a `WorldEvent` `{part: "driver", kind: "exit", text: "exit N", data: {stderr_tail}}`.
  - `pub async fn start_slot(state: &Arc<AppState>, suite: &Suite, slot: &Slot) -> Result<Option<String>>` returns the branch id, mirroring `claim_external` and then `launch`.

- [ ] **Step 1: Write the failing tests** in `tests/connections.rs`:
  - `connections_are_saved_listed_and_redacted`;
  - `a_bad_connection_is_refused` (empty command; missing workdir);
  - `a_driver_that_exits_without_finishing_fails_the_run_scored`: the fixture registers an agent, makes one read tool call through MCP with plain `httpx` (no model; it is a crash test, not an agent), then `sys.exit(3)`. Asserts status `failed`, an error containing `code 3`, a `Check` event, and a `driver` world event;
  - `a_driver_that_outlives_its_run_is_killed`: the fixture finishes, then sleeps 120 s. The process is gone within about 35 s;
  - `live_an_experiment_setup_runs_a_code_connection`: the connection runs `examples/drivers/openai_agents_driver.py` in an experiment of 1 setup × 2 runs. Both runs end with a `check`, and each records `rules.connection`. It skips without the key or the package.

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test connections`. Expected: FAIL (routes missing).

- [ ] **Step 3: Implement** the store, routes, `Rules.connection`, `launch`/`start_slot`, the run and experiment wiring, and the docs.

- [ ] **Step 4: Run them and see them pass:** `cargo test -p shaked --test connections` (and `-- --ignored live_` for the live one). Then `cargo test -p shaked --test experiments --test suites --test external` to check nothing regressed.

- [ ] **Step 5: Commit** `git commit -m "Code connections: shaked launches a team's driver per run, in single runs and experiments"`

### Task 7: Forks relaunch the driver

**Files:**
- Modify: `core/crates/shaked/src/fork.rs:~605` (the external fork path)
- Modify: `ui/src/run/ForkPanel.tsx` (the fork answer's note), `docs/concepts.md` (Forking: Code runs)
- Test: `core/crates/shaked/tests/connections.rs`

**Interfaces:**
- Consumes: `Rules.connection` (Task 6), `driver::launch` with `fork_of`, and `DriverRun.history` / `.is_fork` (Task 3).
- Produces: a fork of a run whose `rules.connection` is set calls `launch` for the new branch with `fork_of = Some(parent)`. The fork answer gains `driver: "relaunched"`. If the agents registered on the new branch have no `history` call recorded, nothing extra happens. The UI marks the branch "Agents restarted" when the driver never read history; the SDK records that read as a `world_event` `{part: "driver", kind: "resumed"}` from `DriverRun.history`.

- [ ] **Step 1: Write the failing test** `live_a_fork_of_a_code_run_relaunches_the_driver`. Run the OpenAI Agents driver, fork at the step of its second tool call, then assert:
  - a new driver process ran (a `driver` exit event on the new branch);
  - the new branch has `tool_call` events after `start_seq`;
  - it ended with a `check`.

- [ ] **Step 2: Run it and see it fail.** Expected: FAIL (no relaunch; the branch stays `running`).

- [ ] **Step 3: Implement** the relaunch and the "Agents restarted" label (UI reads the fork answer's `driver` field and the `resumed` event).

- [ ] **Step 4: Run it and see it pass.**

- [ ] **Step 5: Commit** `git commit -m "Forks of Code runs relaunch the driver on the restored world"`

### Task 8: Code connections in the UI

**Files:**
- Modify:
  - `ui/src/connect/connect.ts:8` (`ConnectKind` gains `'code'`; `ConnectForm` gains `command: string`, `workdir: string` and `env: string` as `KEY=value` lines; `formProblem` and `checkBody` cover `code`);
  - `ui/src/connect/ConnectPage.tsx` (a "Code" choice beside HTTP and Bedrock: fields for name, command, folder and env, plus Test connection, which calls `POST /api/connections/{id}/test` and shows agents, tool calls, status and the stderr tail);
  - the team picker in New experiment and Run once (a saved Code connection is offered as a setup that posts `{runner: "external", connection: id, team: {agents: []}}`);
  - `ui/src/experiments/types.ts:42` (`NewSetup.connection?: string`).
- Create: `ui/src/connect/connections.ts` (`listConnections()`, `createConnection(body)`, `deleteConnection(id)`, `testConnection(id)` over the Task 6 routes)
- Test: `ui/src/connect/connect.test.ts`, `ui/src/connect/ConnectPage.test.tsx`

**Interfaces:**
- Consumes: Task 6 routes and `Connection`.
- Produces: `parseEnv(text: string): { env: Record<string,string> } | { error: string }` (`KEY=value` per line, `#` comments, an error names the bad line), and `codeSetup(c: Connection): NewSetup`.

- [ ] **Step 1: Write the failing tests:**
  - `parseEnv` parses, skips comments, and rejects `NOVALUE` naming line 2;
  - `formProblem` for `code` requires a command;
  - `codeSetup` builds `{name: c.name, runner: 'external', connection: c.id, team: {agents: []}}`;
  - ConnectPage test: choosing Code shows the command field, and Test connection renders `2 agents · 7 tool calls · passed` from a mocked answer.

- [ ] **Step 2: Run them and see them fail:** `cd ui && npx vitest run --maxWorkers=1 src/connect`. Expected: FAIL.

- [ ] **Step 3: Implement** the code. All copy says Shakedown, and the page explains in one line that the command runs on this computer.

- [ ] **Step 4: Run them and see them pass,** plus `npx tsc --noEmit`. Then check by hand with shaked running: save the LangGraph example as a connection, Test it, and run it in an experiment of 1 × 1 from the UI.

- [ ] **Step 5: Commit** `git commit -m "UI: Code connections — save, test, and pick a driver as an experiment setup"`

---

## Deferred to later plans (spec items not in Part 1)

- Reasoning from `http` agents: needs a reasoning URL and token in the `shakedown-agent/1` run request. → Part 2, with the OpenTelemetry intake, which gives every HTTP agent a reasoning path at once.
- Turning on thinking for API-mode agents (provider-specific request flags). → Part 2.
- Running the launched driver inside a sandbox whose network reaches only shaked. → Part 3 (sandboxes). Until then the launch is gated by the loopback-only bind.
- Per-agent limits set in a Code connection, keyed by agent id. → Part 3.
