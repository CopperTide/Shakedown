// The experiments list's rows: ordering sub-experiments under their parent, sorting by whether
// they ran, and filtering. Pure.
import type { Problem } from '@/model/project';
import { RUN_STATUSES, type RunStatus, type RunStatusInfo } from './runStatus';

export interface Row {
  id: string;
  label: string;
  scenario: string;
  setups: number;
  problems: Problem[];
  parent?: string;
  /** Whether it ran; absent until its deployments are read. */
  run?: RunStatusInfo;
}

export interface OrderedRow extends Row {
  /** 0 for a top-level experiment, 1 for a sub-experiment, and so on. */
  depth: number;
}

/** Top-level experiments in id order, each followed by its sub-experiments (depth first). */
export function ordered(rows: Row[]): OrderedRow[] {
  const byParent = new Map<string | undefined, Row[]>();
  const ids = new Set(rows.map((r) => r.id));
  for (const r of [...rows].sort((a, b) => a.id.localeCompare(b.id))) {
    const parent = r.parent && ids.has(r.parent) ? r.parent : undefined;
    byParent.set(parent, [...(byParent.get(parent) ?? []), r]);
  }
  const out: OrderedRow[] = [];
  const visit = (parent: string | undefined, depth: number) => {
    for (const r of byParent.get(parent) ?? []) {
      out.push({ ...r, depth });
      visit(r.id, depth + 1);
    }
  };
  visit(undefined, 0);
  return out;
}

/** `tree`: sub-experiments under their parent; `status`: has results first; `recent`: newest deployment first. */
export type Sort = 'tree' | 'status' | 'recent';

const rank = (r: Row) => (r.run ? RUN_STATUSES.indexOf(r.run.status) : RUN_STATUSES.length);
const byLabel = (a: Row, b: Row) => a.label.localeCompare(b.label) || a.id.localeCompare(b.id);
const FLAT: Record<Exclude<Sort, 'tree'>, (a: Row, b: Row) => number> = {
  status: (a, b) => rank(a) - rank(b) || byLabel(a, b),
  recent: (a, b) => (b.run?.last ?? '').localeCompare(a.run?.last ?? '') || byLabel(a, b),
};

/** The rows in `sort` order; the flat sorts list every experiment at depth 0. */
export function arrange(rows: Row[], sort: Sort): OrderedRow[] {
  if (sort === 'tree') return ordered(rows);
  return [...rows].sort(FLAT[sort]).map((r) => ({ ...r, depth: 0 }));
}

export interface Filter {
  query: string;
  problemsOnly: boolean;
  /** Only the experiments with this status; `all` (the default) keeps every one. */
  status?: RunStatus | 'all';
}

/** The rows whose label or scenario contains `query` (any case), with problems or a status when asked. */
export function filterExperiments<R extends Row>(rows: R[], { query, problemsOnly, status = 'all' }: Filter): R[] {
  const q = query.trim().toLowerCase();
  return rows.filter((r) =>
    (!problemsOnly || r.problems.length > 0) &&
    (status === 'all' || r.run?.status === status) &&
    (!q || r.label.toLowerCase().includes(q) || r.scenario.toLowerCase().includes(q)));
}
