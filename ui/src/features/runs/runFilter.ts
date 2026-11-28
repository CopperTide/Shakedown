// The runs list narrowed by its address (`/runs?setup=…&scenario=…&model=…&from=…&to=…`), as a
// link (or the assistant) opens it. A run matches on its first branch: its setup's name, a model one
// of its agents uses, its scenario (task) and when it was made. Pure functions only.
import type { RunSummary } from '@/model/types';

export interface RunFilter { setup?: string; scenario?: string; model?: string; from?: string; to?: string }

const KEYS = ['setup', 'scenario', 'model', 'from', 'to'] as const;

export function parseRunFilter(sp: URLSearchParams): RunFilter {
  const f: RunFilter = {};
  for (const k of KEYS) {
    const v = sp.get(k);
    if (v) f[k] = v;
  }
  return f;
}

export const isFiltered = (f: RunFilter) => KEYS.some((k) => f[k]);

/** The runs that match every part of `f`. */
export function filterRuns(runs: RunSummary[], f: RunFilter): RunSummary[] {
  if (!isFiltered(f)) return runs;
  const from = f.from ? Date.parse(f.from) : -Infinity;
  const to = f.to ? Date.parse(f.to) : Infinity;
  return runs.filter((r) => {
    const first = r.branches.find((b) => !b.parent_id) ?? r.branches[0];
    const made = Date.parse(r.created_at);
    if (made < from || made >= to) return false;
    if (f.scenario && r.task_id !== f.scenario) return false;
    if (f.setup && first?.team.name !== f.setup) return false;
    if (f.model && !first?.team.agents.some((a) => a.model === f.model)) return false;
    return true;
  });
}

/** "Setup: Solo · Scenario: refund · Since Nov 3": what the list is narrowed to, in words. */
export function describeRunFilter(f: RunFilter): string {
  const day = (iso: string) => new Date(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric', timeZone: 'UTC' });
  const parts = [
    f.setup && `Setup: ${f.setup}`,
    f.scenario && `Scenario: ${f.scenario}`,
    f.model && `Model: ${f.model}`,
    f.from && `Since ${day(f.from)}`,
    f.to && `Before ${day(f.to)}`,
  ];
  return parts.filter(Boolean).join(' · ');
}
