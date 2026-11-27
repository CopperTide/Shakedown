// Explore (operations `explore.*`): a measures × dimensions builder, read-only SQL over the
// documented `explore` views, drill-through to the runs or steps behind a figure, and saved views.
import { del, get, path, post } from './client';

export type MeasureId = 'runs' | 'pass_rate' | 'cost' | 'tokens_in' | 'tokens_out' | 'tokens_cached' | 'model_calls'
  | 'tool_calls' | 'refusals' | 'errors' | 'duration' | 'thinking' | 'steps';
export type DimensionId = 'agent' | 'model' | 'setup' | 'scenario' | 'world' | 'experiment' | 'tool' | 'kind' | 'day';
export type Unit = 'count' | 'ratio' | 'usd' | 'seconds' | 'tokens';
export type Level = 'run' | 'step';

export interface Measure { id: MeasureId; label: string; unit: Unit; level: Level; description: string }
export interface Dimension { id: DimensionId; label: string; level: Level; description: string }

export interface SqlView { name: string; description: string; columns: { name: string; type: string; description: string }[] }

export interface ExploreSchema {
  measures: Measure[];
  dimensions: Dimension[];
  options: { experiments: { id: string; name: string }[]; setups: string[]; agents: string[] };
  sql: {
    schema: string;
    views: SqlView[];
    functions: string[];
    limits: { rows: number; bytes: number; timeout_seconds: number; columns: number };
    examples: { name: string; sql: string }[];
  };
}

/** RFC 3339 times; each list matches any of its values. */
export interface Filters { from?: string; to?: string; experiment?: string[]; setup?: string[]; agent?: string[] }

export interface Spec { measures: MeasureId[]; dimensions: DimensionId[]; filters?: Filters; limit?: number }

export type Cell = string | number | null;

export interface QueryResult {
  columns: { id: string; label: string; role: 'dimension' | 'measure'; unit?: Unit }[];
  rows: Cell[][];
  labels: { experiment?: Record<string, string> };
  truncated: boolean;
  ms: number;
}

export interface DrillInput { spec: Spec; cell: Partial<Record<DimensionId, string | null>>; measure?: MeasureId }

export interface DrillRun {
  run_id: string; branch_id: string; name: string; setup: string | null; scenario: string | null; world: string | null;
  status: string; at: string; seconds: number | null; cost_usd: number | null;
}
export interface DrillStep {
  run_id: string; branch_id: string; run_name: string; seq: number; at: string; agent: string | null;
  kind: string; tool: string | null; outcome: 'ok' | 'refused' | 'error' | null; model: string | null;
}
export type DrillResult =
  | { kind: 'runs'; capped: boolean; runs: DrillRun[] }
  | { kind: 'steps'; capped: boolean; steps: DrillStep[] };

export type ColumnKind = 'number' | 'time' | 'bool' | 'json' | 'text';
export interface SqlResult {
  sql: string;
  columns: { name: string; type: string; kind: ColumnKind }[];
  rows: unknown[][];
  truncated: 'rows' | 'bytes' | null;
  ms: number;
}

export type Chart = 'bar' | 'line' | 'stacked' | 'table';
export type Range = '7d' | '30d' | '90d' | 'all';
/** A builder view: its query, how it is drawn, and (UI only) the relative time range and charted measure. */
export interface BuilderSpec extends Spec { chart?: Chart; range?: Range; y?: MeasureId }
export interface SqlSpec { sql: string; chart?: { type: Chart; x?: string; y?: string[] } }

export type SavedView =
  | { id: string; name: string; kind: 'builder'; spec: BuilderSpec; created_at: string; updated_at: string }
  | { id: string; name: string; kind: 'sql'; spec: SqlSpec; created_at: string; updated_at: string };

export type NewView = { id?: string; name: string } & ({ kind: 'builder'; spec: BuilderSpec } | { kind: 'sql'; spec: SqlSpec });

export const exploreSchema = () => get<ExploreSchema>('/api/explore/schema');
export const exploreQuery = (spec: Spec) => post<QueryResult>('/api/explore/query', spec);
export const exploreDrill = (input: DrillInput) => post<DrillResult>('/api/explore/drill', input);
export const exploreSql = (sql: string) => post<SqlResult>('/api/explore/sql', { sql });
export const listViews = () => get<{ items: SavedView[] }>('/api/explore/views');
export const saveView = (view: NewView) => post<SavedView>('/api/explore/views', view);
export const deleteView = (id: string) => del<{ deleted: true }>(path`/api/explore/views/${id}`);
