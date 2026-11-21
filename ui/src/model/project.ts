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
}
