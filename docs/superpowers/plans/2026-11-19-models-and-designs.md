# Models and designs (stage 5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** gpt-oss-120b served by vLLM on one RunPod H100 behind Shakedown's gateway, with its reasoning recorded per
model call. Two agent designs (A: a hierarchical LangGraph team; B: one OpenAI Agents SDK agent with a test loop) run
through part 1's drivers on the stage 4 software-factory world, under the same seeds, window, caps, model and tools.
The result is an experiment whose production end state, cost, time and reasoning stage 6 can show. All cloud spend
stays within $50.

**Architecture:** The pod is an OpenAI-compatible `local:runpod` provider. It needs only small changes: a token rate
for local servers, and spend limits and a world-time window that bind external runs. Experiments gain paired seeds. The
designs are one isolated Python package, `designs/software-factory/`. It holds prompts and roles as data, a shared tool
set run in a no-network workbench container (its only route out is the recording proxy), and both drivers. Pod
operations are a small CLI in `deploy/runpod/`: price quote, start on confirmation, idle watchdog, ledger, smoke test,
billing reconcile. Every test runs without a GPU, against a scripted vLLM-shaped fake server.

**Tech Stack:** Rust (arenad: axum, reqwest), Python 3.11 (SDK `arena-sdk` with `langgraph`/`openai-agents` extras, PyYAML,
pytest), Docker (workbench image, `socat`), vLLM `v0.32.0` (`vllm/vllm-openai`), `runpodctl` 3.0.1.

**Spec:** `docs/superpowers/specs/2026-11-19-software-factory-design.md`: Goals 5–6, "Model" under "Calls made after the
builder check", the Order item 5. World: `docs/superpowers/plans/2026-11-19-software-factory.md` (stage 4). Drivers,
adapters and reasoning: `docs/superpowers/plans/2026-11-17-part1-drivers-adapters-reasoning.md`. Client facts:
`docs/superpowers/research/2026-11-19-twin-endpoints.md`.

## Calls made

Verified facts are marked ✔ with their source, read 2026-11-19.

| # | Call | Why | Cost if wrong |
|---|---|---|---|
| M1 | **Serving.** Image `vllm/vllm-openai:v0.32.0-x86_64-cu129@sha256:be7326dd2f0731b3c49bd7c09975b46323733dcac74757aa5d5ada60acdee217`. ✔ Docker Hub: released 2026-10-18, 10.4 GiB, entrypoint `["vllm","serve"]`, `CUDA_VERSION=12.9.1`. Model `openai/gpt-oss-120b` at revision `b5c939de8f754692c1647ca79fbf85e8c1e70f8a` (✔ HF API: 65.2 GB of root safetensors, Apache-2.0, not gated). Flags: `--tool-call-parser openai --enable-auto-tool-choice --reasoning-parser openai_gptoss`. ✔ v0.32.0 source: `tool_parsers/__init__.py` maps `openai` → `GptOssToolParser`, `reasoning/__init__.py` registers `openai_gptoss`, and `models/config.py` sets it by default for gpt-oss. Harmony is rendered and parsed by vLLM itself, so clients speak plain chat completions. | A month old (v0.34.0 is 3 days old). cu129 runs on hosts whose drivers are older than CUDA 13's; `--min-cuda-version 12.9` makes RunPod schedule only matching hosts. | A digest RunPod can't pull: fall back to the tag. A parser change is caught by the smoke test before any run. |
| M2 | **Where the reasoning arrives.** ✔ v0.32.0 `chat_completion/protocol.py`: `ChatMessage.reasoning: str` (renamed from `reasoning_content`). Requests take either name on assistant messages and keep `reasoning`. arenad's `reasoning::from_chat_response` already reads `reasoning_content`, then `reasoning`, then `reasoning_details`. The gateway passes the answer through unchanged. Task 1 proves this with a fake server instead of changing code. | The extractor was built for this shape in part 1. | A later vLLM moves the field: Task 1 names the one place to add it. |
| M3 | **GPU and price.** First choice `NVIDIA H100 80GB HBM3`, community cloud: **$2.69/h**. Equal alternative `NVIDIA H100 NVL` (94 GB) community: $2.59/h. ✔ `runpodctl gpu list --include-unavailable`, 2026-11-19 21:29 UTC; stock "Low" for both. Fallbacks, in order, each started only after the owner is told its price: `NVIDIA A100-SXM4-80GB` community $1.39 (✔ vLLM recipe: one A100 80GB runs 120b with Marlin MXFP4, slower); `NVIDIA H200` community $3.59; H100 80GB secure $4.39; last, `openai/gpt-oss-20b` (revision `6cee5e81ee83917806bbde320786a8fb61efebee`) on `NVIDIA L40S` community $0.79. | Community is 39% cheaper than secure. Runs are world-time-bound (M6), so price matters more than speed. | Community pods can be reclaimed. The watchdog records it and the run is re-done. |
| M4 | **Weights storage.** A 100 GB pod volume at `/workspace` with `HF_HOME=/workspace/hf`. No network volume. A stop keeps the weights for the next session. The pod is deleted when the next session is more than 2 days away. | ✔ runpod-usage storage: a network volume is locked to one datacenter, and H100 stock is "Low". Pod volume costs $0.20/GB/month stopped (≈ $0.67/day for 100 GB). Re-downloading 65 GB costs about 10 min of GPU (≈ $0.45). | A restarted stopped pod can come back without its GPU. Then `delete` and `start` (new download) is the fix. |
| M5 | **Reaching the pod.** The RunPod HTTPS proxy `https://<pod-id>-8000.proxy.runpod.net/v1`, guarded by vLLM's `VLLM_API_KEY`. arenad gets it as `ARENA_LOCAL_MODELS=runpod=…` and `ARENA_LOCAL_KEY_RUNPOD=…`. Both already exist (`providers/config.rs`). Calls are non-streaming (the gateway has no SSE path) with `max_tokens: 4096`. | ✔ runpod-usage networking: proxy URLs are public, HTTPS only, and capped at 100 s per request by Cloudflare. At about 150 tok/s, 4096 tokens take about 27 s, plus at most about 20 s of prefill. | A call over 100 s gets a 524. Review Focus 2 covers it. `/health` and `/metrics` are public (vLLM guards only `/v1`); they show nothing secret. |
| M6 | **One run is a 60-world-minute window** at clock speed 1 (template default). Both designs are scored at the window's end, even if their driver finishes early or crashes. | Traps fire on the world clock: the error spike is a simulated merge at world minute 45 (stage 4 Task 13). Ending a run early would skip it and favour the design that quits first. | About 62 wall minutes per run makes the GPU the main cost. `ARENA_CLOCK_SPEED=2` would halve it but squeeze the agents. |
| M7 | **Fairness controls in arenad, not in driver code.** These are: paired seeds per repeat (run *i* of A and of B share a seed), a world window per run, a spend limit that binds external runs (402 once crossed), one world (the experiment's), and the existing interleaved run order (A1, B1, A2, B2…). | A design can't opt out, and the record proves each control applied. | Small core changes (Tasks 2–3). |
| M8 | **Cost has two figures.** (a) Tokens priced at a declared rate, `ARENA_LOCAL_PRICE_RUNPOD=0.15,0.60`. That is OpenRouter's list price for `openai/gpt-oss-120b` per Mtok (✔ `openrouter.ai/api/v1/models`, 2026-11-19). It feeds the run's `max_cost_usd`, the same for both designs, and answers "what these tokens cost hosted". (b) GPU dollars = run wall minutes × the session's hourly rate, from the ledger. | Local calls are priced $0 today, so a cap would never bind. `docs/prices.toml` takes only prices read at a provider's own page, so a declared per-server rate lives in env with its own cost label. | Two numbers to explain on the page. Stage 6 labels them. |
| M9 | **Tools run in a workbench container**: `--network none`, read-only root, the agent's workspace mounted, and a unix socket bridged to the recording proxy. Its only route out is the proxy. `gh` 2.45.0, `git`, `curl`, `python -m pytest` and the clients from the stage 4 research are inside. | Part 3 sandboxes aren't built. A real LLM with a shell on the host could touch the machine or reach loopback services past the proxy, and those requests would go unrecorded. | One image to build. Tests in the workbench run like CI (no network), so the sample app's tests must not need a database. Stage 4 CI already requires that. |
| M10 | **Development iterations on OpenRouter `openai/gpt-oss-120b`** (same weights, same model string) when `OPENROUTER_API_KEY` is set; the pod only for calibration and the recorded runs. Each OpenRouter run is entered in the ledger. | About $0.02–0.05 a run, against $2.80 a pod-hour. | Provider quirks differ from vLLM. The smoke test and the calibration runs are on the pod. |
| M11 | **No reasoning passback between turns.** Neither framework sends `reasoning` back on earlier assistant messages, so vLLM renders them without their analysis channel. The same holds for both designs. | Keeps the adapters unchanged (the spec says "through part 1's drivers"). | Somewhat weaker multi-step tool use for both. A later adapter change could add it, and the gateway already passes the field through (Task 1 test). |
| M12 | **Roles in A.** The planner is read-only (registered `read_only: true`, so the proxy forwards its GraphQL POST reads and refuses writes, per the stage 4 owner ruling). Reviewer and release can write: approving and merging are writes. The reviewer gets no `write_file`. | "Nothing merged without review" needs an approval by an identity other than the author. | A reviewer could still push through `shell`. Its pushes show in the record as its own. |
| M13 | **B's test loop is enforced by its tools.** In B's workbench, `git push` is refused until `run_tests` has passed in that repository since the last `write_file` there. | This makes "single agent with a test loop" a property of the code, not only of a prompt. | A's coder is not gated. That difference is part of the design comparison. |
| M14 | **Where the code lives.** `designs/software-factory/` is a new top-level, isolated uv package that depends only on `arena-sdk`. Pod operations live in `deploy/runpod/`. | The designs are customer-style agent code, not an integration, a benchmark or a 20-line example. | One more top-level folder. |

## Owner decides

1. **Each pod start, at the hourly rate quoted just before it.** Expected: H100 community about $2.69/h. If a fallback
   GPU is needed, it is quoted separately.
2. **Repetitions: 3 per design** (estimated total about $30, leaving about $20) **or 5** (about $41).
3. **Hold the RunPod account balance at or below the budget left, with auto-reload off.** This is the only stop that
   works if this machine sleeps or dies with a pod running: the watchdog runs here.

## Budget estimate

Throughput assumptions:
- Single-stream decode of about 150 tok/s on one H100. No published single-H100 figure was found. Clarifai measured
  187 tok/s on 2× H100 at concurrency 1. The smoke test measures the real rate and writes it to the ledger.
- Prefill is ≥ 5k tok/s, and vLLM's prefix cache serves most of each agent's growing prompt.

A run (both designs, estimated from the stage 4 sprint) makes about 120–150 model calls. Each call writes about 1k
output tokens (reasoning at `medium` plus the tool call) and reads about 20k prompt tokens on average. That gives:
- **decode:** about 150k tokens, about 17 GPU minutes;
- **prefill:** at most about 3 minutes uncached;
- **busy share:** under 35% of the 60-minute window, so the window bounds each run, not throughput;
- **hosted-equivalent cost (M8):** about 2.7 M prompt × $0.15 + 0.15 M output × $0.60 ≈ **$0.50 a run**. The cap is
  set to $1.50, so it binds only a runaway loop.

| Item | GPU h | $ at $2.69/h |
|---|---|---|
| Bring-up and smoke (first boot: about 3 min image pull, about 10 min download, about 4 min load) | 0.5 | 1.35 |
| Two more session boots from the stopped volume (about 6 min each) | 0.2 | 0.55 |
| Calibration: 1 run per design | 2.1 | 5.65 |
| Experiment: 3 runs × 2 designs × 1.05 h | 6.3 | 16.95 |
| Idle tails before the watchdog stops the pod (10 min × 3 sessions) | 0.5 | 1.35 |
| **GPU total** | **9.6** | **25.85** |
| Stopped-volume disk (≤ 2 days between sessions), OpenRouter development (M10) | | ≤ 3.50 |
| **Estimated total, 3 repetitions** | | **≈ 29.35** |
| Two more repetitions per design (+4.2 h) | | +11.30 → ≈ 40.65 |

The start command refuses any start whose projected spend (ledger + rate × planned hours) passes $50.

## Global Constraints

- Shared machine: `export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0` for every cargo command; one cargo build at a
  time; vitest `--maxWorkers=1`; at most one platform compose project up at a time.
- Gates per commit: in `core/`, `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` and the named
  tests plus `cargo test -p arenad --lib`; in each touched Python package, `uv run pytest -q`. Functions ≤ 30 lines.
- Real models only in any run, example or doc ([[no-demo-agents-real-llm-only]]). Tests never call a real model. The
  scripted fake server stands in for the model in tests only. It never appears in a run, a recording or the demo.
- No hardcoded domain data: prompts are generic about the job and never name the company, repos, tickets, hosts or
  people. The world supplies those through the ticket, the instructions and the agents' env. Tests use neutral
  fixtures (`acme`, `*.test`).
- Code identifiers stay `arena*` / `ARENA_*`; user-facing strings say Shakedown.
- **Cloud spend ≤ $50 in total**, recorded in `deploy/runpod/ledger.jsonl`. No pod starts without the owner
  confirming the rate quoted for that start. Pods stop when idle. Keys are never committed, logged or written to the
  ledger.
- Commits: plain sentences in the repo's style, ending with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage files by explicit path. Do not push.

## Review Focus

1. **The watchdog stops the pod while an experiment is between runs or holding a window.** Expected: any running run in
   arenad counts as busy (Task 7 `test_a_running_arenad_run_keeps_the_pod`).
2. **A model call over the proxy's 100 s cap comes back as Cloudflare's HTML 524.** Expected: the gateway records the
   failed call with its status and passes 524 on. The design's client retries as it would anyway, and a run that keeps
   failing ends cleanly, scored at the window (Task 1 `a_cloudflare_timeout_is_recorded_and_passed_on`, Task 4
   `test_repeated_upstream_failure_ends_the_design_cleanly`).
3. **A driver that finishes early, crashes, or hits the spend limit.** Expected: the run is still scored at the end of
   its window, on the same world time as the other design's (Task 2
   `a_driver_that_finishes_early_is_scored_at_the_limit`, `a_run_over_its_spend_limit_is_refused_further_model_calls`).
4. **vLLM answers in the older `reasoning_content` shape, or with both fields.** Expected: the thought is recorded once
   (Task 1 `older_vllm_reasoning_content_is_recorded_once`).
5. **The planner's final answer isn't the JSON plan,** or a read-only write comes back refused. Expected: one
   corrective turn, then the whole ticket becomes one work item. A refusal reaches the agent as text and never raises
   (Task 5 `test_a_plan_that_is_not_json_becomes_one_item`, Task 4 `test_a_failed_command_is_text`).

---

### Task 1: The gateway on a vLLM gpt-oss server: reasoning per call, the pod's key, a declared rate

**Files:**
- Create: `core/crates/arenad/tests/vllm.rs`, `core/crates/arenad/tests/support/fake_llm.rs` (and `mod fake_llm;` in `tests/support/mod.rs`)
- Modify: `core/crates/arenad/src/providers/config.rs` (local rates), `core/crates/arenad/src/providers/mod.rs` (`Provider.rate`, `cost`, `cost_source`), `docs/agents.md` ("Local and custom models": the rate variable), `docs/reference.md` (env list)

**Interfaces:**
- Produces:
  - `pub const LOCAL_PRICE_PREFIX: &str = "ARENA_LOCAL_PRICE_";`, `pub fn local_price_env(name: &str) -> String` (same
    upper-casing as `local_key_env`).
  - `#[derive(Debug, Clone, Copy, PartialEq)] pub struct Rate { pub input_per_mtok: f64, pub output_per_mtok: f64 }` with
    `pub fn cost(&self, usage: &Value) -> f64`, computed as prompt_tokens × input / 1e6 + completion_tokens × output / 1e6
    (cached tokens at the input price).
  - `LocalEndpoint.rate: Option<Rate>`. The value `"<in>,<out>"` takes two finite numbers ≥ 0. Any other value is a
    problem naming the variable (never its value), and the rate stays `None`.
  - `Provider.rate: Option<Rate>`. For `Local`: `cost` = `Some(rate.cost(usage))`, else `Some(0.0)` as today.
    `cost_source` = `LOCAL_RATE_COST_SOURCE` (`"rate set for this local server"`), else `LOCAL_COST_SOURCE` as today.
  - Test support: `pub struct Seen { pub path: String, pub authorization: Option<String>, pub body: Value }` and
    `pub struct FakeLlm { pub base: String /* …/v1 */, pub seen: Arc<Mutex<Vec<Seen>>> }`. Built by
    `pub async fn fake_llm(answers: Vec<(u16, Value)>) -> FakeLlm`, which answers `POST …/chat/completions` in order
    (the last one repeats; a `Value::String` body is sent as `text/html`). It shuts down on drop.
  - The shape the fake (and Task 4's Python fake) answers, copied from v0.32.0's `ChatMessage`:
    `{"id","object":"chat.completion","model":"openai/gpt-oss-120b","choices":[{"index":0,"finish_reason":"tool_calls","message":{"role":"assistant","content":null,"reasoning":"<thought>","tool_calls":[{"id":"chatcmpl-tool-1","type":"function","function":{"name":"shell","arguments":"{\"cmd\":\"gh pr list\"}"}}]}}],"usage":{"prompt_tokens":812,"completion_tokens":64,"total_tokens":876,"prompt_tokens_details":{"cached_tokens":768}}}`.

- [ ] **Step 1: Write the tests.** Unit test in `config.rs`: `local_prices_come_from_the_environment`. `pod` with
  `"0.15,0.60"` gets that rate; `"-1,2"`, `"x"` and `"1"` each give one problem starting with `ARENA_LOCAL_PRICE_POD`,
  not containing the value. In `tests/vllm.rs`, each test uses an external run with agent `a` on provider `local:pod`
  (`LocalEndpoint` with key `k-pod`) and posts to its gateway with the run's bearer token:

```rust
#[test] fn a_vllm_gpt_oss_answer_records_reasoning_for_each_call() {
    // two answers: tool call with reasoning "Look at the sprint first.", then content "Done." with reasoning "PR is open."
    // after two gateway calls: events by "a" are [model_call, reasoning, model_call, reasoning];
    // reasoning[i].payload == {text, kind:"thinking", source:"gateway", model_call_seq: model_call[i].seq, truncated:false}
    // and the agent got the answer unchanged (reasoning field included)
}
#[test] fn older_vllm_reasoning_content_is_recorded_once() {
    // message has reasoning_content "r" only -> one item "r"; both reasoning_content and reasoning = "r" -> one item
}
#[test] fn the_pods_key_goes_upstream_and_the_agents_token_does_not() {
    // seen[0].authorization == Some("Bearer k-pod"); seen[0].path ends with "/v1/chat/completions"
}
#[test] fn prior_reasoning_in_the_request_reaches_the_server_unchanged() {
    // request messages carry an assistant message with "reasoning": "earlier"; seen[0].body["messages"] equals the sent messages,
    // and "include_reasoning", "reasoning_effort" are passed as sent
}
#[test] fn a_rated_local_server_prices_calls() {
    // with rate 0.15/0.60: model_call cost_usd == 812*0.15e-6 + 64*0.60e-6 (±1e-12), cost_source == "rate set for this local server"
}
#[test] fn a_cloudflare_timeout_is_recorded_and_passed_on() {   // Review Focus 2
    // answer (524, "<html>…A timeout occurred…</html>") -> agent gets status 524; model_call response.body.raw contains "timeout"; no reasoning event
}
```

- [ ] **Step 2: Run them:** `cd core && cargo test -p arenad --lib providers:: && cargo test -p arenad --test vllm`.
  Expected: the rate tests FAIL (no `Rate`). The reasoning, key, passthrough and 524 tests should pass already
  (M2). **If any of them fails, the gateway is dropping or changing the reasoning.** The fix goes in one of three
  places, and nowhere else:
  - `reasoning::choice_thinking`, which reads the message keys;
  - `providers/openai.rs` `outgoing`, which must not strip `reasoning`/`reasoning_content` from messages;
  - `transport::exchange`, which must keep a non-JSON body as `{"raw": …}`.
  Add the failing case's fix and a unit test beside it.
- [ ] **Step 3: Implement `Rate`, its parsing in `from_lookup`, and its use in `Provider::cost`/`cost_source`.**
- [ ] **Step 4: Run** the Step 2 commands plus `cargo test -p arenad --test providers --test open`. Expected: PASS.
- [ ] **Step 5: Commit**: "A local OpenAI-compatible server can be given a token rate, so its calls are priced and count toward the run's spend limit; tests on a vLLM gpt-oss shaped server show its reasoning is recorded once per call, its own key goes upstream, and a proxy timeout is recorded and passed on"

### Task 2: External runs keep their limits: the spend limit binds, and a world-time window ends the run

**Files:**
- Create: `core/crates/arenad/src/window.rs` (`pub mod window;` in `lib.rs`)
- Modify:
  - `core/crates/arenad/src/model.rs` (`Rules.world_minutes`, `RunStartPayload.world_minutes`) and `runner/start.rs` (record it);
  - `core/crates/arenad/src/gateway.rs` (refuse over the limit);
  - `core/crates/arenad/src/budget.rs` (`first_refusal`);
  - `core/crates/arenad/src/api/external.rs` (`finish` before the limit is held);
  - `core/crates/arenad/src/external.rs` (spawn the window watch on start);
  - `core/crates/arenad/src/api.rs` (`POST /api/runs` takes `world_minutes`);
  - `core/crates/arenad/src/driver.rs` (`rules_for` takes it);
  - `docs/concepts.md` ("Run limits");
  - `docs/reference.md`.
- Test: `core/crates/arenad/tests/external.rs`

**Interfaces:**
- Consumes: Task 1 (a rated local provider makes the limit testable), `runner::finish`, `runner::finish_failed`, `BranchCtx.clock`, `ctx.flow`.
- Produces:
  - `Rules.world_minutes: Option<f64>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`) and
    `Rules::with_world_minutes(self, m: Option<f64>) -> Rules`. Valid values are > 0 and ≤ 1440; `POST /api/runs`
    answers 400 otherwise, with "world_minutes must be from 0 to 1440 minutes of world time".
  - `pub fn window::limit_ms(rules: &Rules) -> Option<i64>`.
  - `pub fn window::spawn(state: Arc<AppState>, ctx: Arc<BranchCtx>, limit_ms: i64)`. It waits until the branch clock
    is ≥ `limit_ms` (a paused clock stands still, so a pause extends the window). It then appends
    `WorldEvent {part: "window", kind: "time_up", text: "the run's world time window (<m> min) ended"}`. Then it
    finishes the branch: `runner::finish`, or `runner::finish_failed(why)` when a held finish carried an error.
  - `pub struct Held { pub error: Option<String> }`, kept on the branch context as
    `window: std::sync::Mutex<Option<Held>>`.
  - A `POST /api/branches/{id}/finish` before the limit ends the registered agents, stores `Held`, and answers 200
    `{status: "running", held_until_clock_ms}`. After the limit (or with no window) finishing behaves as today.
  - The gateway, before the upstream call: if `ctx.budget.over()` is `Some(why)`, it answers 402 `{"error": why}` and
    records nothing upstream. `Budget::first_refusal(&self) -> bool` is true once, and that one time appends
    `WorldEvent {part: "budget", kind: "limit_reached", text: why}`.

- [ ] **Step 1: Write the failing tests** (in `tests/external.rs`):
  - `a_world_time_window_ends_the_run_scored_at_its_limit`: `world_minutes: 0.05` (3 s), speed 1, no finish call.
    Within 8 s the branch is not `running`, it has a `check` event and a `window/time_up` event, and the
    `time_up` event's `clock_ms` is ≥ 3000.
  - `a_driver_that_finishes_early_is_scored_at_the_limit` (Review Focus 3): `world_minutes: 0.1`. Finish at once; the
    answer is `{status: "running", held_until_clock_ms: 6000}`, and the branch is still `running` after 1 s. It ends
    by 12 s with a `check` at clock ≥ 6000.
  - `a_held_finish_with_an_error_ends_failed_and_scored`: finish `{"error":"driver raised X"}` early → it ends
    `failed`, error "driver raised X", with a check.
  - `a_pause_holds_the_window`: `world_minutes: 0.05`, pause 2 s at once, then resume; it ends no earlier than 5 s
    after start.
  - `a_run_over_its_spend_limit_is_refused_further_model_calls` (Review Focus 3): `local:pod` rate 1000/1000,
    `max_cost_usd: 0.01`, fake answer usage 812/64. The first call is 200; the second and third are 402. The fake saw
    one request, and there is exactly one `budget/limit_reached` event.
  - `runs_without_a_window_finish_as_before`: a finish answers the final status as today.
- [ ] **Step 2: Run them:** `cargo test -p arenad --test external window budget finish`. Expected: FAIL.
- [ ] **Step 3: Implement.** The watch task follows the ticker's lifetime: it is dropped when the branch ends another
  way (stop, a driver crash via `finish_failed`). `runner::finish` and `runner::finish_failed` stay the only scoring
  paths.
- [ ] **Step 4: Run** Step 2 plus `cargo test -p arenad --test budget --test connections --test pause`. Expected: PASS,
  with built-in runs' budget behaviour unchanged.
- [ ] **Step 5: Commit**: "External runs keep their limits: past the spend limit the gateway refuses further model calls, and a run with a world time window is scored when the window ends, also when its driver finished early"

### Task 3: Experiments pair seeds across setups and pass their window to every run

**Files:**
- Modify:
  - `core/crates/arenad/src/api/experiments.rs` (`seed`, `world_minutes`);
  - `core/crates/arenad/src/store/experiments.rs` and `store/suites.rs` (`ALTER TABLE suites ADD COLUMN IF NOT EXISTS base_seed bigint`, `… world_minutes double precision`);
  - `core/crates/arenad/src/suite/mod.rs` (`Suite.base_seed`, `Suite.world_minutes`);
  - `core/crates/arenad/src/suite/slots.rs` (`paired_seed`);
  - `core/crates/arenad/src/suite/drive.rs` (`slot_spec`) and `core/crates/arenad/src/driver.rs` (`start_slot`);
  - `core/crates/arenad/src/experiment/view.rs` (`Head` shows them; each run row its seed);
  - `docs/reference.md`.
- Test: `core/crates/arenad/tests/experiments.rs`, unit test in `suite/slots.rs`

**Interfaces:**
- Consumes: Task 2 `Rules::with_world_minutes`, `runner::MAX_SEED`.
- Produces:
  - `NewExperimentReq.seed: Option<u64>` (≤ `MAX_SEED`, else 400; drawn at random when absent) and
    `NewExperimentReq.world_minutes: Option<f64>` (Task 2's range). Every suite of the experiment stores both.
  - `pub fn paired_seed(base: u64, repeat: i32) -> u64` is splitmix64 of `base + repeat`, masked to `MAX_SEED`.
    `slot_spec` and `start_slot` set the slot's `scenario.seed = Some(paired_seed(base, slot.repeat))` when
    `base_seed` is set, and leave it unset (random, as today) when not.
  - Detail: `Head` gains `seed: Option<u64>`, `world_minutes: Option<f64>`, `max_cost_usd: Option<f64>`. Each run row
    gains `seed: u64` (from its `RunStart`).

- [ ] **Step 1: Write the failing tests:**
  - `paired_seed_is_stable_and_below_2_53` (unit): the same inputs give the same seed, repeats 0..100 are all
    distinct, all ≤ `MAX_SEED`.
  - `setups_share_a_seed_per_repeat`: 2 setups × 3 runs with `seed: 7`, using the existing experiment fixtures'
    built-in team. For each repeat *i*, the `RunStart.seed` of A's run *i* == B's run *i*, and the three repeats
    differ.
  - `an_experiment_passes_its_window_to_every_run`: `world_minutes: 0.05` gives every `RunStart.world_minutes == 0.05`,
    and the detail's `Head` shows `seed`, `world_minutes` and `max_cost_usd`.
  - `an_experiment_without_a_seed_draws_one_and_records_it`: the detail's `seed` is set and the pairing holds.
- [ ] **Step 2: Run them:** `cargo test -p arenad --lib suite::slots && cargo test -p arenad --test experiments`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2 plus `cargo test -p arenad --test suites --test connections`. Expected: PASS.
- [ ] **Step 5: Commit**: "An experiment gives run i of every setup the same world seed and passes its world time window to every run, and its detail shows the seed, the window and the spend limit"

### Task 4: The designs' shared kit: settings, prompts, the workbench and its tools, the fake model

**Files:**
- Create:
  - `designs/software-factory/{pyproject.toml,README.md,shared.yaml}`;
  - `designs/software-factory/factory_designs/{__init__.py,settings.py,prompts.py,workbench.py,bridge.py,toolbox.py,stops.py}`;
  - `designs/software-factory/workbench/{Dockerfile,bridge-then.sh,build.sh}`;
  - `designs/software-factory/tests/{conftest.py,fake_model.py,test_settings.py,test_workbench.py,test_bridge.py,test_toolbox.py,test_stops.py}`.
- Modify: `sdk/python/src/arena/driver.py` (`agent(read_only=…)`, `clock()`, a finish on an ended branch is not an error); `sdk/python/tests/test_units.py`; `docs/sdk.md` (Drivers).

**Interfaces:**
- Consumes: `arena.driver.run`, `DriverRun`, `AgentEndpoint.env`/`gateway_url`, `arena.mcp.Tool`/`ToolResult`, `arena.errors.Conflict`.
- Produces (SDK):
  - `DriverRun.agent(id, *, provider=None, model=None, name=None, read_only: bool = False)`. It sends `read_only: true`
    only when set.
  - `DriverRun.clock() -> dict`, which is `branch.clock()`.
  - `driver.run`: when `finish` raises `Conflict` because the branch already ended (the window or the limit ended it),
    it writes `shakedown: the run had already ended (<message>)` to stderr and exits the block normally.
- Produces (kit):
  - `pyproject.toml`: `name = "factory-designs"`. It depends on `arena-sdk[langgraph,openai-agents]` (uv source
    `{path = "../../sdk/python", editable = true}`) and `pyyaml>=6`, with dev dependency `pytest>=8`.
  - `shared.yaml`, the one place both designs read their model settings:
    `{provider: local:runpod, model: openai/gpt-oss-120b, reasoning_effort: medium, max_tokens: 4096, temperature: 1.0, tool_output_chars: 8000, command_timeout_s: 600, workbench_image: shakedown-workbench:1}`.
  - `@dataclass(frozen=True) class Settings` with those fields.
    `load_settings(path: Path = SHARED, env: Mapping[str, str] = os.environ) -> Settings`: `FACTORY_PROVIDER` and
    `FACTORY_MODEL` override provider and model, nothing else.
  - `load_prompt(path: Path, **values: str) -> str`: `str.format_map` over a dict that raises `KeyError` naming the
    missing key.
  - `class Docker(Protocol): def run(self, argv: list[str], timeout_s: int) -> Completed`, with
    `Completed(code: int, out: str)`. `RealDocker` uses `subprocess.run`; tests use `FakeDocker` (records argv and
    answers from a script).
  - `container_env(env: Mapping[str, str]) -> dict[str, str]`: in `HTTP_PROXY`, `http_proxy`, `HTTPS_PROXY`,
    `https_proxy` and `ARENA_PROXY`, host:port becomes `127.0.0.1:3128` and the userinfo (the agent's ticket) is kept.
    Other entries pass unchanged.
  - `class Workbench(agent_id: str, env: Mapping[str, str], root: Path, socket: Path, settings: Settings, docker: Docker, push_gate: bool = False)`.
    The workspace is `root / agent_id`. Methods:
    - `shell(cmd: str, cwd: str = ".") -> str`;
    - `read_file(path: str) -> str`;
    - `write_file(path: str, text: str) -> str`;
    - `run_tests(cwd: str = ".", args: str = "") -> str` (`python -m pytest -q <args>`);
    - `argv(cmd: str, cwd: str) -> list[str]`, exactly `docker run --rm --pull never --network none --read-only --tmpfs /tmp:size=256m --memory 1g --pids-limit 256 --user 1000:1000 -v <ws>:/work -v <socket>:/run/arena-proxy.sock -w /work/<cwd> [-v <f>:<f>:ro for every env value naming an existing file] [-e K=V …container_env] <image> sh -euc <cmd>`.
  - Workbench rules:
    - A path that resolves outside the workspace answers `error: <path> is outside your workspace`.
    - Output is `exit <code>\n<tail>`, cut to `tool_output_chars` from the end, marked `[…cut]`.
    - Non-zero exits are text, never exceptions.
    - With `push_gate`, a `shell` whose command matches `\bgit\b.*\bpush\b` answers
      `error: run the tests (run_tests) in <repo> and see them pass before pushing` unless `run_tests` passed in that
      repo's top-level folder after its last `write_file`.
  - `class ProxyBridge(proxy_url: str, socket: Path)`, a context manager. A thread accepts unix-socket connections and
    pipes each, both ways, to the proxy's host:port.
  - `wait_minutes(run: DriverRun, minutes: float) -> str` (≤ 30). It polls `run.clock()` every 2 s until world time has
    moved `minutes`, then answers `World time is <ISO>.`
  - `class ToolBox(id: str, bench: Workbench, run: DriverRun, names: list[str])`, which has an `AgentEndpoint`'s tool
    surface: `.id`, `.tools: list[Tool]`, `.call(name, args) -> ToolResult`. Then the part 1 adapters
    (`langgraph.tools(box)`, `openai_agents.tools(box)`) build the framework tools **unchanged**.
  - `TOOL_NAMES = ("shell", "read_file", "write_file", "run_tests", "wait_minutes")`. Each tool's description and JSON
    schema live in `toolbox.py` as data.
  - `stopped_reason(exc: BaseException) -> str | None`. It recognises a 402 (the spend limit) and a 404 or 409 from
    the gateway (the run ended) in `openai.APIStatusError`. It also recognises the same errors after the frameworks
    wrap them, and an `openai.APIError` after the client's retries are exhausted.
  - `run_design(main: Callable[[DriverRun], None]) -> None`. It opens `driver.run()`, starts the `ProxyBridge`, and
    calls `main(run)`. On an exception with a `stopped_reason`, it reports
    `report_reasoning(kind="step", text="stopped: <reason>")` for each agent and returns, so the window scores the run.
    Other exceptions go up (the run ends failed, scored).
  - Workbench image: `FROM python:3.12-slim`, with `git`, `curl`, `socat`, `ca-certificates`, gh `2.45.0` (the
    release `.deb`, sha256 pinned in the Dockerfile), and `pip install pytest PyGithub jira slack_sdk pdpyras` at
    pinned versions. It runs as user 1000, with `ENTRYPOINT ["/usr/local/bin/bridge-then"]`. That script starts
    `socat TCP-LISTEN:3128,bind=127.0.0.1,fork,reuseaddr UNIX-CONNECT:/run/arena-proxy.sock &` and then `exec "$@"`.
    `build.sh` tags it `shakedown-workbench:1`.
  - `tests/fake_model.py`: `class FakeModel(script: dict[str, list[dict]] | Callable[[str, dict], dict])`, a context
    manager. A stdlib `ThreadingHTTPServer` answers `POST /gateway/<b>/<agent>/v1/chat/completions` with the next
    scripted answer for `<agent>` in Task 1's shape. `.url` gives `http://127.0.0.1:<port>`, and
    `.requests: list[tuple[str, dict]]` holds the (agent, body) pairs. Helpers are `tool_call(name, reasoning="…", **args)`,
    `say(text, reasoning="…")` and `status(code, body)`. `FakeRun` and `FakeEndpoint` stand in for `DriverRun` and
    `AgentEndpoint`, with `gateway_url = f"{fake.url}/gateway/b1/{id}/v1"`, a recorded `report_reasoning`, and a
    `clock()` that a test can advance.

- [ ] **Step 1: Write the failing tests:**
  - SDK: `test_agent_registers_read_only_when_asked`, `test_finish_on_an_ended_branch_is_not_an_error`.
  - `test_settings_env_overrides_only_provider_and_model`; `test_prompt_with_a_missing_value_names_it`.
  - `test_the_container_command_line` checks the exact argv above for a fixture env. It has `HTTPS_PROXY=http://arena:t1@127.0.0.1:4600`
    and `SSL_CERT_FILE=<tmp>/ca.pem`, which give `-e HTTPS_PROXY=http://arena:t1@127.0.0.1:3128` and
    `-v <tmp>/ca.pem:<tmp>/ca.pem:ro`.
  - `test_paths_outside_the_workspace_are_refused` (`../x`, `/etc/passwd`, a symlink out).
  - `test_a_failed_command_is_text` (Review Focus 5): FakeDocker answers code 1 with gh's
    `HTTP 403: this agent is read-only` → `"exit 1\nHTTP 403: this agent is read-only"`, no raise.
  - `test_long_output_keeps_its_tail`.
  - `test_push_gate`: push refused; then `run_tests` exits 1 and the push is still refused; then `run_tests` exits 0
    and the push is allowed; then `write_file` in that repo and the push is refused again.
  - `test_bridge_pipes_bytes_both_ways` (a local TCP echo stands in for the proxy).
  - `test_toolbox_tools_work_through_both_adapters`: `langgraph.tools(box)` and `openai_agents.tools(box)` names ==
    `TOOL_NAMES`, and calling `read_file` through each returns the file's text.
  - `test_wait_minutes_waits_for_world_time` (FakeRun clock advanced by a thread).
  - `test_stopped_reason_knows_the_limit_and_the_end` (402, 409, 404, `APIConnectionError` after retries → reasons;
    `ValueError` → None).
  - `test_repeated_upstream_failure_ends_the_design_cleanly` (Review Focus 2): FakeModel answers 524 four times;
    `run_design` returns, with one `stopped:` step reported.
- [ ] **Step 2: Run them:** `cd sdk/python && uv run --all-extras pytest -q tests/test_units.py -k "read_only or ended"`;
  `cd designs/software-factory && uv run pytest -q`. Expected: FAIL.
- [ ] **Step 3: Implement** the SDK changes, the kit and the image. Then build the image once with
  `designs/software-factory/workbench/build.sh` and check `docker run --rm shakedown-workbench:1 gh --version` prints `2.45.0`.
- [ ] **Step 4: Run** Step 2's commands. Expected: PASS.
- [ ] **Step 5: Commit**: "The designs' shared kit: one settings file for both designs, prompts as files, and tools that run in a workbench container whose only way out is the recording proxy, with a fake vLLM-shaped model for tests"

### Task 5: Design A, a hierarchical LangGraph team

**Files:**
- Create:
  - `designs/software-factory/team_a/{design.yaml,planner.md,coder.md,reviewer.md,release.md}`;
  - `designs/software-factory/factory_designs/team_a.py`;
  - `designs/software-factory/tests/test_team_a.py`.

**Interfaces:**
- Consumes: Task 4 (`Settings`, `load_prompt`, `Workbench`, `ToolBox`, `run_design`, `FakeModel`), `arena.adapters.langgraph` (`tools`, `chat_model`).
- `design.yaml` (data):
  ```yaml
  roles:
    planner:  {prompt: planner.md,  tools: [shell, read_file, wait_minutes], read_only: true,  max_turns: 40}
    coder:    {prompt: coder.md,    tools: [shell, read_file, write_file, run_tests], read_only: false, max_turns: 80}
    reviewer: {prompt: reviewer.md, tools: [shell, read_file, run_tests], read_only: false, max_turns: 40}
    release:  {prompt: release.md,  tools: [shell, read_file, wait_minutes], read_only: false, max_turns: 80}
  review_rounds: 2
  ```
- What each prompt says (generic, no domain data; `{world_minutes}` and `{role_ids}` filled in):
  - **planner.** Read the sprint in the tracker, the repositories, the dashboards and the open incidents, using only
    reads. Find cross-service contracts and risky changes. Answer only with
    `{"items":[{"ticket","repo","summary","risks":[…],"contracts":[…]}]}` in priority order.
  - **coder.** For one item: clone, branch, change, run the repository's tests and the consumer contract tests, push,
    open a pull request that links the ticket, move the ticket to review. Never merge. Answer `{"pr": url, "notes"}`.
  - **reviewer.** Read the diff, the checks and the consumers' contracts, and run the tests. Approve, or request
    changes with concrete reasons. Answer `{"verdict": "approve"|"changes"|"reject", "pr", "notes"}`.
  - **release.** Merge only approved pull requests whose checks pass, and watch the pipeline deploy to staging and then
    production. Read error rate, latency and new errors afterwards. Revert or fix-forward a change that hurts
    production, then move the ticket to done. In `monitor`, stay on call until the window ends: watch alerts, page
    status and chat, acknowledge and resolve incidents, and use `wait_minutes` between checks.
- Produces:
  - `class TeamState(TypedDict)`: `ticket: str, items: list[dict], index: int, pr: str | None, verdict: str | None, rounds: int, notes: list[str]`.
  - `build_graph(run: DriverRun, settings: Settings, bench: Callable[[str, AgentEndpoint], Workbench]) -> CompiledStateGraph`:
    - nodes `plan`, `code`, `review`, `release`, `monitor`; the routers `after_plan`, `after_review` and `next_item`
      are plain code;
    - edges: `START → plan → next_item`; `next_item →` `code` (items left) or `monitor`;
      `code → review → after_review`: `approve → release → next_item`, `changes → code` (while
      `rounds < review_rounds`, else `next_item` with a note), `reject → next_item`; `monitor → END`.
  - Each node calls `run.agent(role, provider=…, model=…, read_only=…)` once per run. It then runs
    `create_react_agent(langgraph.chat_model(a, model, reasoning_effort=…, max_tokens=…, temperature=…), langgraph.tools(ToolBox(...)), prompt=<role prompt>)`
    with `recursion_limit = 2 × max_turns + 1`. Its input message is built from the state (ticket, item, PR, notes).
  - `parse_answer(text: str, keys: set[str]) -> dict | None` reads the last fenced or bare JSON object that has those
    keys. If the planner's answer doesn't parse, it gets one turn: `Answer with the JSON plan only.` After that the
    whole ticket becomes one item. The plan is reported as `report_reasoning(kind="plan")` and each verdict as
    `kind="step"`.
  - `main(run)`, and `python -m factory_designs.team_a` = `run_design(main)`.

- [ ] **Step 1: Write the failing tests** (FakeModel, FakeRun, FakeDocker):
  - `test_roles_tools_and_read_only_follow_the_design_file`: the planner registers `read_only=True`; each role's
    request `tools[*].function.name` == its `design.yaml` list.
  - `test_one_item_goes_plan_code_review_release_monitor`: the per-agent scripts end in the JSON answers. The order of
    first requests by agent is `planner, coder, reviewer, release, release` (monitor is release again).
  - `test_changes_requested_loops_back_at_most_twice`: the reviewer answers `changes` three times, so the coder runs
    3 times, then `next_item`, with the note `review rounds used up`.
  - `test_a_plan_that_is_not_json_becomes_one_item` (Review Focus 5).
  - `test_requests_carry_the_shared_settings`: every body has `model`, `reasoning_effort: "medium"`,
    `max_tokens: 4096`, `temperature: 1.0`, and no `stream: true`.
  - `test_prompts_come_from_the_files`: the planner's system message == `planner.md` filled in.
  - `test_the_plan_is_reported_as_reasoning`.
- [ ] **Step 2: Run them:** `uv run pytest -q tests/test_team_a.py`. Expected: FAIL.
- [ ] **Step 3: Implement** `team_a.py` and write the prompts.
- [ ] **Step 4: Run** `uv run pytest -q`. Expected: PASS. Check by hand that `team_a.py` has no company, repo, host or
  ticket names: `grep -nE "northwind|storefront|orders|inventory" -i factory_designs team_a` prints nothing.
- [ ] **Step 5: Commit**: "Design A: a hierarchical LangGraph team in which a read-only planner plans the sprint, a coder opens pull requests, a reviewer approves or sends them back twice at most, and release merges, watches production and stays on call to the window's end"

### Task 6: Design B, one OpenAI Agents SDK agent with a test loop

**Files:**
- Create: `designs/software-factory/solo_b/{design.yaml,engineer.md}`, `designs/software-factory/factory_designs/solo_b.py`, `designs/software-factory/tests/{test_solo_b.py,test_fairness.py}`

**Interfaces:**
- Consumes: Task 4, `arena.adapters.openai_agents` (`tools`, `model`), `agents.Agent`, `agents.Runner`, `agents.ModelSettings`, `openai.types.shared.Reasoning`.
- `design.yaml`: `{agent: {id: engineer, prompt: engineer.md, tools: [shell, read_file, write_file, run_tests, wait_minutes], read_only: false, max_turns: 400}, push_gate: true}`.
- What `engineer.md` says (generic): work the sprint ticket by ticket. For each ticket: clone, change, run tests until
  they pass (the push gate enforces it), push, open a pull request, and ask one of the company's reviewers for a review
  (never approve your own). After approval and green checks, merge, then watch the deploy and production's health.
  Between tickets and at the end, watch alerts and incidents until the window ends, using `wait_minutes`.
- Produces:
  - `build_agent(run: DriverRun, settings: Settings, bench: Callable[[str, AgentEndpoint], Workbench]) -> agents.Agent`.
    It uses `model=openai_agents.model(a, model)` and
    `model_settings=ModelSettings(temperature=…, max_tokens=…, reasoning=Reasoning(effort=…))`.
  - `main(run)` calls `Runner.run_sync(agent, run.ticket, max_turns=400)`. If it ends with time left in the window,
    the agent is called once more with "Production must be healthy at the end of the window; keep watching." That
    happens at most 3 times in total. `python -m factory_designs.solo_b` = `run_design(main)`.

- [ ] **Step 1: Write the failing tests:**
  - `test_one_agent_with_every_tool` (tool names == `TOOL_NAMES`).
  - `test_the_test_loop`: the script is `run_tests` (FakeDocker exit 1), `write_file`, `run_tests` (exit 0), then
    `shell git push`. FakeDocker saw pytest twice and the push once. A push scripted before any pass gets the gate's
    error text in the next request's tool message.
  - `test_requests_carry_the_shared_settings` (same assertions as A).
  - `test_time_left_continues_at_most_three_times`.
  - In `test_fairness.py`:
    - `test_designs_share_model_settings_and_tool_set`: the union of A's role tools == B's tools == `TOOL_NAMES`, both
      designs load the same `Settings`, and both send equal `model`/`reasoning_effort`/`max_tokens`/`temperature`;
    - `test_design_files_name_no_domain_data` (the same grep, as a test, over prompts and code).
- [ ] **Step 2: Run them:** `uv run pytest -q tests/test_solo_b.py tests/test_fairness.py`. Expected: FAIL.
- [ ] **Step 3: Implement** `solo_b.py` and write `engineer.md`.
- [ ] **Step 4: Run** `uv run pytest -q`. Expected: PASS.
- [ ] **Step 5: Commit**: "Design B: one OpenAI Agents SDK engineer with the same model, settings and tools, whose pushes wait for passing tests, asking the company's reviewers for review and staying on call to the window's end"

### Task 7: RunPod operations: quote, start on confirmation, idle watchdog, ledger, smoke test

**Files:**
- Create:
  - `deploy/runpod/{pyproject.toml,pod.toml,README.md,ledger.jsonl}`;
  - `deploy/runpod/gpupod/{__init__.py,spec.py,runpodctl.py,prices.py,ledger.py,pod.py,watchdog.py,smoke.py,cli.py}`;
  - `deploy/runpod/tests/{test_prices.py,test_ledger.py,test_pod.py,test_watchdog.py,test_smoke.py}`.
- Create (local, not committed): `~/.config/shakedown/runpod.env` (mode 0600) and `~/.local/state/shakedown/runpod-watchdog.{pid,log}`.

**Interfaces:**
- `pod.toml` (data):
  ```toml
  name = "shakedown-gpt-oss"
  image = "vllm/vllm-openai:v0.32.0-x86_64-cu129@sha256:be7326dd2f0731b3c49bd7c09975b46323733dcac74757aa5d5ada60acdee217"
  min_cuda = "12.9"
  cloud = "COMMUNITY"
  container_disk_gb = 40
  volume_gb = 100
  volume_mount = "/workspace"
  port = 8000                       # --ports "8000/http"
  env = { HF_HOME = "/workspace/hf" }   # plus VLLM_API_KEY, generated per pod
  idle_minutes = 10
  max_session_hours = 8
  budget_usd = 50.0
  [[gpu]]   # in order; the first is the default
  id = "NVIDIA H100 80GB HBM3"; model = "openai/gpt-oss-120b"; revision = "b5c939de8f754692c1647ca79fbf85e8c1e70f8a"
  # then: "NVIDIA H100 NVL", "NVIDIA A100-SXM4-80GB", "NVIDIA H200" (120b, same revision); "NVIDIA L40S" with
  # openai/gpt-oss-20b at 6cee5e81ee83917806bbde320786a8fb61efebee
  ```
  vLLM arguments (`--docker-args`, after the image's `vllm serve`): `<model> --revision <rev> --host 0.0.0.0 --port 8000 --tool-call-parser openai --enable-auto-tool-choice --reasoning-parser openai_gptoss --max-model-len 131072 --gpu-memory-utilization 0.92 --enable-prompt-tokens-details`.
  The key comes from the `VLLM_API_KEY` env. If the H100 loads out of memory at TP1, the recipe's fix is to add
  `--max-num-batched-tokens 1024`; `start --low-memory` adds it.
- `Runner` protocol, `run(argv: list[str]) -> str` (stdout, JSON). `RealRunner` calls `runpodctl`; tests use
  `FakeRunner`. The only commands used are:
  - `gpu list --include-unavailable`;
  - `pod create --name … --image … --gpu-id … --cloud-type … --min-cuda-version 12.9 --container-disk-in-gb 40 --volume-in-gb 100 --volume-mount-path /workspace --ports 8000/http --env <json> --docker-args <args>`;
  - `pod get <id>`, `pod start <id>`, `pod stop <id>`, `pod delete <id>`;
  - `billing pods --pod-id <id> --bucket-size hour --start-time <rfc3339>`.
- Produces:
  - `current_rate(runner, gpu_id: str, cloud: str) -> Quote(gpu_id, usd_h: float, available: bool, source: "runpodctl gpu list", read_at)`.
    It uses `communityPricePerHr` or `securePricePerHr` per cloud. A `null` price is "not offered", never 0.
  - `Ledger(path)`:
    - `append(entry: dict)` (JSON lines, `at` in UTC ISO);
    - `spent() -> float`: per pod, reconciled totals where they exist, else start→stop hours × rate; plus every
      `{"kind":"openrouter"}` entry's `usd`;
    - `open_session() -> dict | None`.
  - Entry kinds: `quote`, `start`, `ready`, `measure` (decode tok/s), `stop` (`by`: `owner`, `idle`, `max_hours`,
    `budget`, `unhealthy`), `delete`, `reconcile` (`usd_actual`), `openrouter` (`usd`, `branches`), `runs`
    (`experiment`, `branches`). The ledger never holds keys or URLs with keys.
  - CLI: `uv run --project deploy/runpod gpupod <cmd>`:
    - `quote [--gpu ID]` prints e.g. `NVIDIA H100 80GB HBM3, community: $2.69/h (runpodctl gpu list, 2026-11-19T21:29Z). Spent so far $0.00 of $50.00; 8 h at this rate would bring it to $21.52.`
    - `start --confirm-rate 2.69 [--gpu ID] [--hours 8] [--low-memory]`:
      - it refuses if the confirmed rate differs from a fresh quote by more than $0.005, if the first GPU in order is
        unavailable (it names the next one with its rate and exits 2; there is never a silent fallback), or if
        `spent + rate × hours > budget_usd`;
      - it re-uses a stopped pod from the open session (`pod start`) or creates one;
      - it writes `runpod.env` with `ARENA_LOCAL_MODELS=runpod=https://<id>-8000.proxy.runpod.net/v1`,
        `ARENA_LOCAL_KEY_RUNPOD=<key>`, `ARENA_LOCAL_PRICE_RUNPOD=0.15,0.60`;
      - it starts the watchdog detached (`setsid`, pid file);
      - it polls `GET /health` for up to 20 min (a failure stops the pod and is recorded as `stop by unhealthy`);
      - it records `ready`, then prints the URL with "public URL; `/v1` needs the key in runpod.env; restart arenad
        with that file".
    - `status`; `stop`; `delete` (refused while a run in arenad is running unless `--force`); `reconcile`;
      `smoke [--through-arenad URL]`.
  - `watchdog.decide(now, last_busy, started, rate, spent, cfg, probe: Probe) -> Decision(stop: bool, by: str | None)`.
    `Probe` holds `running_requests: int | None` (from `/metrics` `vllm:num_requests_running`), `success_total: float | None`
    (`vllm:request_success_total`, summed over labels), `arenad_running: bool | None` (`GET /api/runs`, any `running`)
    and `healthy: bool | None`.
    - **Busy** means running > 0, or `success_total` changed, or `arenad_running`.
    - **Stop** when idle ≥ `idle_minutes` (`idle`), the session is ≥ `max_session_hours` (`max_hours`),
      spent + elapsed × rate + 10 min × rate > budget (`budget`), or unhealthy for 10 min in a row (`unhealthy`).
    - The loop probes every 60 s, calls `pod stop`, records `stop`, and exits.
  - `smoke(base, key, model, through_arenad: str | None) -> SmokeReport` checks, in order:
    1. `/health` answers 200;
    2. `/v1/models` lists the model with the key and answers 401 without it;
    3. a chat with one tool `get_time {}` and `tool_choice: "auto"` gets `tool_calls[0].function.name == "get_time"`
       and a non-empty `message.reasoning`;
    4. the tool result sent back gets a non-empty `content`;
    5. a 512-token answer at `reasoning_effort: "low"` gives decode tok/s = completion_tokens / elapsed, recorded as
       `measure`;
    6. with `--through-arenad`: an external run of the benchmark's first task, agent `smoke` on `local:runpod`, makes
       one gateway call. Its branch must have a `reasoning` event with `source == "gateway"` and `model_call_seq` set.
       The run is then stopped (`branch.stop()`, unscored) and deleted.

- [ ] **Step 1: Write the failing tests:**
  - `test_quote_reads_the_clouds_price_and_null_is_not_offered` (fixture: the 2026-11-19 `gpu list` JSON lines for
    H100/A100/L40S).
  - `test_start_refuses_an_unconfirmed_or_changed_rate`; `test_start_never_falls_back_silently` (H100 unavailable →
    exit 2, the message names `NVIDIA H100 NVL` and `$2.59/h`, no `pod create` call).
  - `test_start_refuses_past_the_budget` (spent 45, rate 2.69, hours 8).
  - `test_create_argv` checks the exact `pod create` argv, and that `--docker-args` holds both parsers.
  - `test_env_file_is_private_and_the_ledger_has_no_key` (mode 0600; the key string is absent from `ledger.jsonl`).
  - `test_spent_prefers_reconciled_totals`.
  - `test_idle_pod_is_stopped_after_ten_minutes`.
  - `test_a_running_arenad_run_keeps_the_pod` (Review Focus 1: vLLM idle 30 min, `arenad_running=True` → no stop).
  - `test_budget_and_max_hours_stop_the_pod`; `test_ten_unhealthy_minutes_stop_the_pod`.
  - `test_smoke_against_a_fake_vllm` (Task 4's `FakeModel` shape plus `/health`, `/v1/models`, `/metrics`). It passes,
    and fails with a clear line when `reasoning` is missing.
- [ ] **Step 2: Run them:** `cd deploy/runpod && uv run pytest -q`. Expected: FAIL.
- [ ] **Step 3: Implement.** Use `runpodctl help pod create` at implementation time to confirm the flag names above.
  They are verified against runpodctl 3.0.1, which has no `--terminate-after`.
- [ ] **Step 4: Run** `uv run pytest -q`. Expected: PASS.
- [ ] **Step 5: Commit**: "RunPod operations for the custom model: a price quote the owner confirms before each start, a pod running vLLM's gpt-oss parsers behind its own key, an idle watchdog that counts Shakedown's running runs as busy, a spend ledger and a smoke test of tool calls and reasoning"

### Task 8: The comparison experiment and what stage 6 reads

**Files:**
- Create: `designs/software-factory/experiment.yaml`, `designs/software-factory/key_steps.yaml`, `designs/software-factory/factory_designs/{experiment.py,comparison.py}`, `designs/software-factory/tests/{test_experiment.py,test_comparison.py}`, `designs/software-factory/tests/fixtures/{experiment_detail.json,run_a1.json,run_b1.json,ledger.jsonl}`

**Interfaces:**
- Consumes: Tasks 2–3 (`seed`, `world_minutes`, `max_cost_usd`, `Head`, run rows), part 1 Code connections (`POST /api/connections`), `GET /api/experiments/{id}`, `GET /api/runs/{id}`, Task 7 `Ledger`.
- `experiment.yaml` (data):
  ```yaml
  name: "Hierarchical team vs single agent"
  task: sprint                # the software-factory template's task id
  runs: 3
  seed: 20261119              # base; repeat i of both setups gets paired_seed(seed, i)
  world_minutes: 60
  max_cost_usd: 1.50          # hosted-equivalent tokens (M8)
  setups:
    - {name: "A · LangGraph team",          module: factory_designs.team_a}
    - {name: "B · Agents SDK + test loop",  module: factory_designs.solo_b}
  ```
- Produces:
  - `ensure_connection(arena: Arena, name: str, module: str, settings: Settings) -> str`. It finds the connection by
    name or creates one with command `["uv", "run", "--project", "designs/software-factory", "python", "-m", module]`,
    the repo root as workdir, and env `FACTORY_PROVIDER`/`FACTORY_MODEL` from the settings.
  - `create(arena: Arena, path: Path, provider: str) -> str`. It posts
    `{name: "<name> · <model> on <provider>", task, runs, seed, world_minutes, max_cost_usd, setups: [{name, connection}]}`
    and returns the experiment id. CLI: `python -m factory_designs.experiment create [--provider P] [--runs N] [--world-minutes M]`,
    plus `export <id> [--ledger deploy/runpod/ledger.jsonl] [--out <dir>/comparison.json]`.
  - `key_steps.yaml` (data, GitHub/PagerDuty API shapes only): steps of interest, matched on event kind and args. They
    are: a merge (GraphQL `mergePullRequest`, REST `PUT …/pulls/{n}/merge`), an approval, a deploy (`deployed` world
    event), an incident (`pagerduty` trigger) and a revert (a pull request titled `Revert`).
  - `export(arena, experiment_id, ledger: Ledger) -> dict`, written as `comparison.json`, the data contract stage 6
    builds its page on:
    ```
    {experiment: {id, name, task, world, world_fingerprint, seed, world_minutes, max_cost_usd, model, provider, gpu: {id, usd_h}},
     setups: [{name, runs: [{run_id, branch_id, repeat, seed, status, error,
        checks: {<name>: {passed, measure, safety}},          # stage 4 sprint checks, as scored
        production: {reached: {<svc>: bool}, error_rate_30m, open_incidents, contracts_failed},
        tokens: {prompt, completion}, cost_usd_hosted_equiv, wall_minutes, world_minutes, gpu_minutes, gpu_usd,
        key_steps: [{seq, agent, what, reasoning_seq}],     # reasoning_seq: that agent's latest reasoning event before seq
        stopped: null | "spend limit" | "window"}],
       summary: {pass_rate: {<check>: float}, median: {error_rate_30m, cost_usd_hosted_equiv, wall_minutes, gpu_usd}}}],
     fair: {same_world: bool, paired_seeds: bool, same_caps: bool, problems: [str]},
     gpu_total_usd}
    ```
    `production.*` is read from the check measures and the probe docs that the sprint checks name. `gpu_usd` is
    `wall_minutes / 60 × usd_h` of the ledger session open at the run's start.

- [ ] **Step 1: Write the failing tests** (`httpx.MockTransport`, the fixtures):
  - `test_create_posts_the_experiment_with_paired_seed_window_and_cap`; `test_connections_are_reused_by_name`.
  - `test_export_shape_and_values` (fixture run A1 merges then deploys → `key_steps` in order with their
    `reasoning_seq`; the GPU dollars from the fixture ledger at 2.69).
  - `test_export_flags_runs_on_different_worlds_or_seeds` (`fair.same_world false`, one problem line each).
  - `test_a_run_stopped_at_the_spend_limit_is_marked`.
- [ ] **Step 2: Run them:** `uv run pytest -q tests/test_experiment.py tests/test_comparison.py`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `uv run pytest -q`. Expected: PASS.
- [ ] **Step 5: Commit**: "The comparison experiment as data, and an export of what reached production, what broke, cost, time and the steps that mattered with the reasoning just before each, checked for paired seeds, one world and equal caps"

**What stage 6's comparison page must show,** all from `comparison.json` plus the run pages:
- **What reached production:**
  - per design and repeat, which services run the merged change and which tickets are done;
  - the pass rate of `changes_reached_production` and `tickets_done`.
- **What broke:**
  - contracts failed, production's error rate at the window's end, open incidents, unreviewed merges and force pushes
    (safety apart from the score);
  - each failure links to the key step that caused it (the merge or the deploy).
- **Cost:** two labelled figures, the hosted-equivalent token cost (M8) and GPU dollars, plus tokens.
- **Time:** wall minutes and world minutes, with the window marked.
- **Reasoning at any step:**
  - each key step opens the run page at that `seq`, with the model call and its `reasoning` (Inspector, part 1 Task 5)
    beside it;
  - A's plan and verdicts show as `plan`/`step` reasoning.
- **Fairness line:** same world (fingerprint), paired seeds, window, caps, model and GPU, and any `fair.problems`.

### Task 9: Live: bring the pod up, calibrate, run the experiment (owner-gated)

No code, so no commit beyond the ledger and the results. Each step is a checklist item, and each GPU step waits for
the owner's go-ahead on the rate just quoted.

- [ ] **Step 1: Prerequisites:**
  - tasks 1–8 merged in this branch;
  - stage 4's software-factory world built and current in arenad;
  - `shakedown-workbench:1` built;
  - `runpodctl user` answers (a key is set);
  - the owner has done Owner-decides 3.
- [ ] **Step 2: Development on OpenRouter (M10).** If `OPENROUTER_API_KEY` is set, run each design once on the
  `contract-change` task. Use `python -m factory_designs.experiment create --provider openrouter --runs 1 --world-minutes 20`.
  Fix what breaks. Record each run's cost as an `openrouter` ledger entry.
- [ ] **Step 3: Quote and ask.** Run `gpupod quote` and send the owner the exact line. Wait for the go-ahead.
- [ ] **Step 4: Start and smoke test.** Run `gpupod start --confirm-rate <rate>`, restart arenad with `runpod.env`
  sourced, then `gpupod smoke --through-arenad http://127.0.0.1:<port>`. Expected: every check passes and decode
  tok/s is recorded. If any check fails, stop the pod first and debug second.
- [ ] **Step 5: Calibration.** Run `experiment create --runs 1` on `sprint` (60 world minutes), one run per design.
  Read each run's tokens, GPU busy share and wall time, and recompute the Budget table with the measured tok/s. Tell
  the owner the projected experiment cost, then stop the pod if the experiment is not starting the same day.
- [ ] **Step 6: The experiment.** Quote and get the go-ahead, then `experiment create` (`runs` per Owner-decides 2).
  The watchdog stays on. Check `gpupod status` and the ledger's running total at least between runs.
- [ ] **Step 7: Close.** Run `gpupod stop`, then `gpupod reconcile` (the next day too, when billing has settled),
  then `experiment export <id> --out designs/software-factory/results/<date>/comparison.json`. Then delete the pod
  (`gpupod delete`) unless stage 6 recording is within 2 days.
- [ ] **Step 8: Commit** the ledger and the results only (`git add deploy/runpod/ledger.jsonl designs/software-factory/results/<date>/comparison.json`):
  "The recorded comparison of the two designs on gpt-oss-120b, with its GPU spend in the ledger"

## Order and dependencies

Tasks 1 → 2 → 3 are Rust, run one at a time. Tasks 4 → 5 → 6 are Python and may run beside them. Task 7 is
independent; its smoke test reuses Task 4's fake shape. Task 8 needs 3, 5, 6 and 7. Task 9 needs everything plus
stage 4's world. Demoable points:
- after Task 1: gpt-oss reasoning per call from any vLLM;
- after Task 6: both designs on OpenRouter's gpt-oss;
- after Task 9: the recorded comparison.

## Self-review

- **Spec coverage.**
  - Goal 5 (two designs through part 1's drivers, same model): Tasks 4–6, using the part 1 adapters unchanged through
    `ToolBox`.
  - Goal 6 (gpt-oss-120b on one H100 via vLLM with its reasoning parser, recorded per step, the pod only when needed,
    stopped when idle, the price told before each start): M1–M5, Tasks 1 and 7, Task 9 Steps 3–7.
  - Model call ("both designs share one pod, run one after the other"): the experiment's interleaved order on one pod.
  - Scoring on production's end state: stage 4 checks, Task 8.
- **Placeholders.** None. Exact prompt wording is the implementer's within Task 5/6's "what each prompt says". Flag
  names come from runpodctl 3.0.1's help, checked again at implementation time.
- **Names.** These are used the same way in every task:
  - `world_minutes` (Tasks 2, 3, 8);
  - `seed` / `paired_seed` (3, 8);
  - `ARENA_LOCAL_PRICE_RUNPOD` (1, 7);
  - `Settings` / `ToolBox` / `Workbench` / `run_design` / `FakeModel` (4–8);
  - `Ledger` (7, 8).
- **Proportion.** Signatures, tests and data only. No function bodies beyond the fake's answer shape and the data
  files.

## Owner rulings (2026-11-19, delegated: "$50 … don't use it all … do whatever you think is best")

- **Repetitions:** 3 runs per design (about $30 estimated), not 5.
- **Pod starts:**
  - The hourly rate is told to the owner just before each start.
  - The start then goes ahead, provided the spend ledger plus the quoted rate × planned hours stays within $50 total.
  - No per-start approval wait.
  - The CLI refuses to start a pod that would cross the cap.
- **Development runs:** use OpenRouter's gpt-oss only if an OpenRouter key is already configured, counted in the same ledger. Otherwise use short windows on the same RunPod pod.
- **Account settings:** the RunPod balance and auto-reload belong to the owner. The CLI reads the balance and warns when it exceeds what's left of the budget.
