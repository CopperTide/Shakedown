// Mirrors shaked's model.rs / fork.rs / compare.rs / recorded.rs (serde snake_case).
import type { Board } from '@/features/team/board';
import type { SystemInfo, ToolInfo } from './report';
import type { RecordedScenario, ScenarioOptions } from './scenario';
import type { WorldViewPayload } from './twin';

/** `http`: an agent someone deployed, reached at its URL (`shakedown-agent/1`); shaked doesn't run it, it calls it. */
export type AgentKind = 'claude_code' | 'codex' | 'open' | 'external' | 'http';

/** A header value as shaked answers it: never the secret itself, a reference to it (valid until shaked restarts). */
export interface SecretRef {
  secret: string;
}

/**
 * Who drives a run's agents (`model.rs` `Runner`). `dispatch`: a lead delegates to the team, whose
 * agents wake for work and stay available; `external`: the design's own system, over the API.
 */
export type Runner = 'chain' | 'concurrent' | 'dispatch' | 'external';

/** Model shaked uses for an OpenRouter agent with no model set (`providers/mod.rs`). */
export const DEFAULT_OPENROUTER_MODEL = 'deepseek/deepseek-chat-v3.1';

/** The provider of an open agent that names none (and of calls recorded before providers). */
export const DEFAULT_PROVIDER = 'openrouter';

/** `providers/mod.rs` `ProviderKind`. */
export type ProviderKind = 'openrouter' | 'openai' | 'anthropic' | 'local';

/** One entry of `GET /api/providers`, or of `/api/health`'s `providers` (`providers/status.rs`). */
export interface ProviderStatus {
  /** `openrouter`, `openai`, `anthropic` or `local:<name>`. */
  id: string;
  kind: ProviderKind;
  label: string;
  available: boolean;
  /** True when the provider itself was asked (its model list), not only the configuration. */
  checked: boolean;
  why?: string;
  models?: string[];
}
/** `stopped`: someone stopped the run before it ended (`POST /api/branches/{id}/stop`); unscored. */
export type BranchStatus = 'running' | 'queued' | 'passed' | 'failed' | 'infra_error' | 'interrupted' | 'stopped';
export type EventKind =
  | 'run_start'
  | 'agent_start'
  | 'model_call'
  | 'tool_call'
  | 'handoff'
  | 'message'
  | 'agent_end'
  | 'check'
  | 'world_event'
  /** The digital twin after a step (`payload: {seq_of, view}`); not a step of its own. */
  | 'world_view'
  /** Someone paused the branch: its calls wait and its clock stands still until `resume`. */
  | 'pause'
  /** The branch goes on after a pause (`payload: {paused_ms}`). */
  | 'resume'
  /** A dispatch run's lead gives an agent work (`payload: {to, task}`). */
  | 'delegation'
  /** An agent of a dispatch run reports to its lead (`payload: {to, text, turn_end?}`). */
  | 'report'
  /** shaked starts an agent's turn in a dispatch run (`payload: {reason, mail, idle_wake?, input}`). */
  | 'wake'
  /** An agent of a dispatch run ended its turn and waits (`payload: {ok, error?, text}`). */
  | 'idle'
  /** The lead of a dispatch run ends it (`payload: {summary}`). */
  | 'finish'
  /** What an agent thought or said it would do (`payload: {text, kind: 'thinking'|'step'|'plan', source:
   *  'agent'|'gateway'|'claude_code', model_call_seq?, truncated?}`); not a step of its own. */
  | 'reasoning'
  /** shaked saved the parts that save at points (`payload: {reason, saves, error?, paused_ms}`); not a step. */
  | 'save_point';

export type RestoreMode = 'exact' | 'approximate';
/** How one part of a rerun's world is (or was) restored (`model.rs` `RestoredHow`). */
export type RestoredHow = 'save' | 'replay' | 'approximate';

export interface RestoredPart {
  part: string;
  how: RestoredHow;
}

/** One part of `GET /api/branches/{id}/restore-plan` (`fork/rerun.rs` `PartPlan`). */
export interface PartPlan extends RestoredPart {
  /** For a replay: a rough estimate of how long it takes. */
  estimate_seconds?: number;
}

export interface RestorePlan {
  parts: PartPlan[];
}

/** `{part: save}`. */
export type Saves = Record<string, string>;

/** A part of a benchmark's world. */
export interface WorldPart {
  part: string;
  integration: string;
  config?: unknown;
}

/** Payload of `run_start` (`runner.rs`); runs recorded before integrations have only `task_id` and `team`. */
export interface RunStartPayload {
  task_id: string;
  team?: Team;
  benchmark?: string;
  world?: WorldPart[];
  seed?: number;
  base_seed?: number;
  /** Each part's integration fingerprint at the run's start. */
  fingerprints?: Record<string, unknown>;
  saves?: Saves;
  /** The run's rules; absent in runs recorded before runners (a chain with a handoff required). */
  runner?: Runner;
  handoff?: 'required' | 'optional';
  /** The scenario the run was started with, as shaked expanded it (absent: none chosen). */
  scenario?: RecordedScenario;
  /** Each part's kind of system, and each tool's effect and part (absent in runs recorded before they were kept). */
  systems?: Record<string, SystemInfo>;
  tools?: Record<string, ToolInfo>;
  /** The run's cost cap, when it has one. */
  max_cost_usd?: number;
  /** The Code connection whose driver drives the run. */
  connection?: string;
}

export interface AgentSpec {
  id: string;
  name: string;
  kind: AgentKind;
  model: string | null;
  /** Open agents only: `openrouter` (the default when absent), `openai`, `anthropic`, `local:<name>` or `bedrock:<region>`. */
  provider?: string | null;
  /** HTTP agents only: where shaked calls the agent. */
  url?: string;
  /** HTTP agents only: headers sent with every call; shaked answers each value as a `SecretRef`. */
  headers?: Record<string, string | SecretRef>;
  prompt: string;
  /** True when the team keeps this agent to read tools; absent or false, it may use every tool. */
  read_only?: boolean;
  /** The agent that leads a dispatch run (absent everywhere: the first agent leads). */
  lead?: boolean;
}

export interface Team {
  id: string;
  name: string;
  agents: AgentSpec[];
  /** A CopperTide design's board (groups, places, arrows), kept in this browser; never sent to shaked. */
  board?: Board;
  /** The runner a suggested team's prompts are written for (`GET /api/teams`); kept in this browser. */
  runner?: Runner;
  /** Picked from the benchmark's suggestions on purpose (kept in this browser): stays as it is. */
  picked?: boolean;
  /** A Code connection's id: the team is that connection's driver (its agents register as it runs). */
  connection?: string;
}

export interface Change {
  prompt?: string | null;
  model?: string | null;
  kind?: AgentKind | null;
  /** A new provider; a model change for the fork rules. Sent with `model`. */
  provider?: string | null;
  read_only?: boolean | null;
}

interface EventBase {
  branch_id: string;
  seq: number;
  at: string;
  agent: string | null;
}

/** An event whose payload has the shape shaked writes for its kind (`Payloads`). */
export type TypedEvent = { [K in EventKind]: EventBase & { kind: K; payload: Payloads[K] } }[EventKind];

/** A recorded event: `kind` says the shape of its payload. */
export type Event = TypedEvent;

export interface ToolCallPayload {
  name: string;
  args: unknown;
  result: unknown;
  error: string | null;
  reads: unknown;
  writes: unknown;
  /** `saves.retail` when that part was saved; runs recorded before integrations have only this. */
  snapshot: string | null;
  snapshot_error?: string;
  tool_call_id: string | null;
  /** The world part the tool ran on. */
  part?: string;
  /** The parts saved after this call (absent when nothing was saved). */
  saves?: Saves;
  /** The world clock when the call ran. */
  clock_ms?: number;
  paused_ms?: number;
  /** Keys the call changed for its part's own bookkeeping: saved like a write, but not one. */
  internal?: string[];
  /** How long the tool ran in the world (absent when it reached no part, and in older runs). */
  duration_ms?: number;
}

export interface ModelCallPayload {
  request: unknown;
  response: unknown;
  usage: unknown;
  cost_usd: number | null;
  cost_source: string | null;
  /** The provider that answered (absent in calls recorded before providers: OpenRouter). */
  provider?: string;
  /** The provider's own response, when it differs from the canonical `response`. */
  provider_response?: unknown;
  /** Set on a call that compacted the conversation (Claude Code). */
  compaction?: unknown;
}

export interface AgentEndPayload {
  ok: boolean;
  error: string | null;
  tokens: number | null;
  /** The input and output tokens of `tokens` (absent in runs recorded before they were kept). */
  prompt_tokens?: number | null;
  completion_tokens?: number | null;
  cost_usd: number | null;
  cost_source: string | null;
  seconds: number | null;
}

export interface HandoffPayload {
  note: string;
  /** The agent the note is for; absent: for whoever comes next. */
  to?: string;
}

/** `runner.rs` / `external.rs` / `dispatch` `AgentStart`. */
export interface AgentStartPayload {
  name: string;
  kind: AgentKind;
  model: string | null;
  provider?: string;
  /** An agent an external system started (joined over the API). */
  external?: boolean;
}

/** One agent's message to another, or to everyone (`to` = `*`). */
export interface MessagePayload {
  to: string;
  text: string;
}

/** Something a part reported as its clock advanced (a disruption, a decision, the driver's exit). */
export interface WorldEventPayload {
  part: string;
  kind: string;
  text: string;
  data: unknown;
  /** Absent on the driver's exit. */
  clock_ms?: number;
}

/** A dispatch run's lead gives agent `to` its `task`. */
export interface DelegationPayload {
  to: string;
  task: string;
}

/** An agent's report to the lead; `turn_end` when shaked passed on what it said at the end of its turn. */
export interface ReportPayload {
  to: string;
  text: string;
  turn_end?: boolean;
}

/** The lead's summary of the run. */
export interface FinishPayload {
  summary: string;
}

/** Why a dispatch run woke an agent, the steps whose mail it got, and its input. */
export interface WakePayload {
  reason: 'start' | 'mail' | 'idle' | 'resume';
  mail: number[];
  input: string;
  /** Which wake of a waiting team this is (`reason: idle`). */
  idle_wake?: number;
}

/** An agent of a dispatch run ended its turn and waits. */
export interface IdlePayload {
  ok: boolean;
  text: string;
  error?: string;
}

/** The parts that save at points, saved before a model call, when a driver asked or after the task's setup. */
export interface SavePointPayload {
  reason: 'model_call' | 'asked' | 'setup';
  saves: Saves;
  error?: string;
  paused_ms: number;
}

/** The branch goes on after a pause. */
export interface ResumePayload {
  paused_ms: number;
}

/** What an agent thought or said it would do (`reasoning.rs`). */
export interface ReasoningPayload {
  text: string;
  kind: 'thinking' | 'step' | 'plan';
  source: 'agent' | 'gateway' | 'claude_code';
  model_call_seq?: number;
  /** Cut at 32,768 characters. */
  truncated?: boolean;
}

/** Each event kind's payload as shaked writes it. */
export interface Payloads {
  run_start: RunStartPayload;
  agent_start: AgentStartPayload;
  model_call: ModelCallPayload;
  tool_call: ToolCallPayload;
  handoff: HandoffPayload;
  message: MessagePayload;
  agent_end: AgentEndPayload;
  check: CheckPayload;
  world_event: WorldEventPayload;
  world_view: WorldViewPayload;
  pause: Record<string, never>;
  resume: ResumePayload;
  delegation: DelegationPayload;
  report: ReportPayload;
  wake: WakePayload;
  idle: IdlePayload;
  finish: FinishPayload;
  reasoning: ReasoningPayload;
  save_point: SavePointPayload;
}

export interface CheckPayload {
  passed: boolean;
  gold_db: string | null;
  differing_rows: string[] | null;
  /** Every part's final state. */
  final_saves?: Saves;
  /** Parts with no expected result to compare with (parts with replay). */
  unchecked?: string[];
  /** A benchmark that scores itself: each named check (`safety`: an unsafe action, not a missed goal). */
  checks?: NamedCheck[];
}

export interface NamedCheck {
  name: string;
  passed: boolean;
  safety?: boolean;
  /** Why it failed, in words (empty when it passed). */
  detail?: string;
  /** The figure the check measured (an optimisation's verified objective, simulations used). */
  measure?: Measure;
}

/** A number a check measured. `better` absent: neither way is better (a count). `reference`: a value no run made. */
export interface Measure {
  label: string;
  value: number;
  unit?: string;
  better?: 'higher' | 'lower';
  target?: number;
  reference?: { label: string; value: number };
}

export interface Branch {
  id: string;
  run_id: string;
  parent_id: string | null;
  start_seq: number | null;
  team: Team;
  changes: Record<string, Change> | null;
  fork_note: string | null;
  status: BranchStatus;
  error: string | null;
  created_at: string;
  ended_at: string | null;
  /** The restore a fork asked for (null for a run's first branch; absent from older shaked versions). */
  restore_mode?: RestoreMode | null;
  /** How each part of a fork's world was restored (empty for a run's first branch). */
  restored?: RestoredPart[];
}

/** Shakedown's own time per kind, in ms (`overhead.rs`); a kind with none is absent. */
export type Overhead = Partial<Record<'save' | 'restore' | 'record' | 'gateway' | 'gate' | 'link' | 'mcp' | 'wait' | 'score' | 'pause', number>>;

/** A cost and where it comes from (`metrics.rs`). */
export interface Cost {
  usd: number | null;
  /** `reported`, `list-price estimate`, `no API charge`, `unknown` or `mixed`. */
  label: string;
  source: string | null;
  as_of: string | null;
}

export interface AgentUse {
  agent: string;
  kind: AgentKind | null;
  model: string | null;
  provider: string | null;
  tokens: number;
  prompt_tokens: number | null;
  completion_tokens: number | null;
  cost: Cost;
}

/** A branch's numbers for deployment estimates (`metrics.rs`). */
export interface Metrics {
  measured_s: number;
  overhead_ms: Overhead;
  overhead_s: number;
  overhead_recorded: boolean;
  estimated_s: number | null;
  tokens: number;
  cost: Cost;
  agents: AgentUse[];
}

/** GET /api/branches/{id}: the branch plus its events and metrics. */
export interface BranchDetail extends Branch {
  events: Event[];
  metrics?: Metrics;
  /** A queued cloud run's place in line, from 1. */
  queue_position?: number | null;
}

export interface RunSummary {
  id: string;
  name?: string;
  task_id: string;
  created_at: string;
  branches: Branch[];
}

export interface ForkResponse {
  branch_id: string;
  start_seq: number;
  moved_from: number | null;
  reason: string | null;
  /** A run a Code connection drove: Shakedown started its driver again for the rerun, or couldn't. */
  driver?: 'relaunched' | 'failed';
}

export interface Health {
  /** Some provider can take API-mode calls, judged from the configuration. */
  open_available: boolean;
  /** Each provider from the configuration alone: local servers are `checked: false` (not asked). */
  providers?: ProviderStatus[];
  /** Whether the Claude Code and Codex CLIs are on the shaked machine's PATH. */
  claude: boolean;
  codex: boolean;
  /** Packages and agents run only in sandboxes (hosted): the CLIs run there, whatever this machine's PATH says. */
  sandboxes?: boolean;
  disk?: DiskUsage;
  benchmark?: BenchmarkInfo;
}

/** Space used by every save the shakedown keeps (the hub's and each integration's), and the limit. */
export interface DiskUsage {
  bytes: number;
  limit_bytes: number;
}

/** The benchmark new runs use; `error` when it can't be described. */
export interface BenchmarkInfo {
  name: string;
  version?: string;
  tasks?: number;
  world?: WorldPart[];
  error?: string;
}

/** `GET /api/benchmark`: what the scenario builder and the world view need of the benchmark. */
export interface BenchmarkDescription {
  name: string;
  version: string;
  tasks: { id: string; reason: string }[];
  /** The disruptions a run can be started with (null when the benchmark offers none). */
  scenario_options: ScenarioOptions | null;
  /** True when the benchmark draws the world's view. */
  view: boolean;
}

export interface TaskSummary {
  id: string;
  reason: string;
}

/** One of `GET /api/tasks` (`api.rs` `tasks`): a task with its ticket and the benchmark's instructions. */
export interface Task extends TaskSummary {
  ticket: string;
  instructions: string;
  /** The world's date and time at clock 0 (`YYYY-MM-DDTHH:MM:SS`), for a world whose clock is not today's. */
  clock_start?: string;
}

export interface NewRunResponse {
  run_id: string;
  branch_id: string;
}

/** SSE `status` event payload. */
export interface StatusEvent {
  status: BranchStatus;
  error: string | null;
}

export interface RowsResponse {
  seq: number;
  /** Key `<table>/<id>` to the document at that step, or null if absent. */
  rows: Record<string, unknown>;
}

export interface CompareSide {
  status: BranchStatus;
  restore_mode?: RestoreMode | null;
  restored?: RestoredPart[];
  seconds: number | null;
  tokens: number | null;
  cost_usd: number | null;
  cost_source: string | null;
  /** The branch's own metrics, cost per agent included (absent in older answers). */
  metrics?: Metrics;
}

export interface RowDiff {
  key: string;
  a: unknown;
  b: unknown;
}

export interface CompareResponse {
  a: CompareSide;
  b: CompareSide;
  first_divergent_seq: number | null;
  differing_rows: RowDiff[];
  /** Parts that keep no saves, so their final states could not be compared. */
  not_compared?: string[];
}
