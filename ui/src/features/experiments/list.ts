// The experiments list's rows: ordering sub-experiments under their parent, and filtering. Pure.
import type { Problem } from '@/model/project';

export interface Row {
  id: string;
  label: string;
  scenario: string;
  setups: number;
  problems: Problem[];
  parent?: string;
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

export interface Filter {
  query: string;
  problemsOnly: boolean;
}

/** The rows whose label or scenario contains `query` (any case), with problems when asked. */
export function filterExperiments<R extends Row>(rows: R[], { query, problemsOnly }: Filter): R[] {
  const q = query.trim().toLowerCase();
  return rows.filter((r) =>
    (!problemsOnly || r.problems.length > 0) &&
    (!q || r.label.toLowerCase().includes(q) || r.scenario.toLowerCase().includes(q)));
}
