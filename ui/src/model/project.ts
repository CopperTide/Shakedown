// The project folder's items as the project API returns them (plan 1): ids and names, never paths.

export type ItemKind = 'worlds' | 'scenarios' | 'setups' | 'experiments';

/** Something wrong with a file or between files; never stops the rest of the project from loading. */
export interface Problem {
  kind?: string | null;
  id?: string | null;
  pointer: string;
  message: string;
}

export interface ProjectStatus {
  name: string;
  counts: Record<ItemKind, number>;
  problems: Problem[];
  settings?: ProjectSettings;
}

export interface ItemSummary {
  kind: string;
  id: string;
  label: string;
  hash: string;
  problems: Problem[];
}

/** An item with its file's contents. */
export interface Item<B = Record<string, unknown>> extends ItemSummary {
  body: B;
  diff?: string;
}

/** A put that the project's permissions turned into a proposal: nothing was written. */
export interface Proposed {
  proposed: true;
  diff: string;
}

/** An experiment file's contents (shakedown-experiment/1). */
export interface ExperimentBody {
  format: string;
  label: string;
  world: string;
  scenario: string;
  setups: string[];
  repeats: { per_setup: number; paired_seeds?: boolean };
  budget?: { per_run_usd?: number; per_deployment_usd?: number };
  parent?: string;
}

/** A credential reference: a key-store name or an environment variable, never the value. */
export interface AuthRef { key?: string; env?: string }

export interface Connector {
  id: string;
  label: string;
  mode: 'stand-in' | 'real' | 'replay';
  product?: string;
  mcp?: string;
  openapi?: string;
  auth?: AuthRef;
}

export interface Flow { id: string; trigger: string; steps: string[] }

/** One data source: exactly one of file, database or import. */
export interface DataSource {
  file?: string;
  database?: string;
  import?: string;
  seeds?: string;
  from?: string;
  engine?: string;
  version?: string;
  snapshot?: string;
  masked?: boolean;
  default_state?: boolean;
}

export interface Person { id: string; label: string; role?: string }

/** A world file's contents (shakedown-world/3). */
export interface WorldBody {
  format: string;
  label: string;
  package?: string;
  connectors?: Connector[];
  flows?: Flow[];
  data?: DataSource[];
  people?: Person[];
  observability?: Observability;
}

export interface Change { record: string; value: unknown }

/** A failure or disruption injected during the run: at a world time or after N agent calls. */
export interface ScenarioEvent {
  name: string;
  at?: string;
  after_calls?: number;
  effects?: unknown[];
  db?: string;
  fault?: string;
  params?: Record<string, unknown>;
}

export interface Gate { rule: string; scope: string; on_breach: 'block' | 'flag' }

export interface Check { text: string; kind: 'goal' | 'safety'; query?: string; ends_run?: boolean }

/** A scenario file's contents (shakedown-scenario/1). */
export interface ScenarioBody {
  format: string;
  label: string;
  world: string;
  task: string;
  preset?: string;
  changes?: Change[];
  events?: ScenarioEvent[];
  gates?: Gate[];
  checks?: Check[];
}

export interface BuiltinAgent { id: string; role: string; model: string; key: string; tools?: string[] }

/** Where a team comes from. A team described in MADL gets the full graph and checks. */
export type TeamSpec =
  | { kind: 'builtin'; agents: BuiltinAgent[] }
  | { kind: 'code'; command: string }
  | { kind: 'http'; url: string; auth?: AuthRef }
  | { kind: 'claude-code' }
  | { kind: 'codex' }
  | { kind: 'madl'; file: string };

/** An agent setup file's contents (shakedown-setup/1). */
export interface SetupBody { format: string; label: string; team: TeamSpec }

export type EditPolicy = 'diff_first' | 'apply_then_tell';
export type SpendPolicy = 'always_ask' | { under_cap: { usd_per_day: number } };

/** `shakedown.yaml`: the project's name, the assistant's model and what agents may do without asking. */
export interface ProjectSettings {
  format: string;
  name: string;
  defaults?: { world?: string; cost_cap_usd?: number };
  permissions?: { edit?: EditPolicy; spend?: SpendPolicy };
  assistant?: { model: string; provider?: string };
}

export type OperatorBehavior = { answers_after: { minutes: number } } | 'approves' | 'refuses' | 'never_answers';
export type HumanMoment = 'paged' | 'approval' | 'gate_needs_person' | 'agent_stuck' | 'message_to_person';

/** A world's monitoring, alerting and on-call. */
export interface Observability {
  tools?: { id: string; product: string; mode: 'real' | 'stand-in'; seeded_from?: string; auth?: AuthRef }[];
  alerts?: { name: string; when: string; routes_to?: string }[];
  operators?: { id: string; label: string; role?: string; behavior: OperatorBehavior }[];
  human?: { moment: HumanMoment; then: 'continue' | 'pause' | 'fail' }[];
}
