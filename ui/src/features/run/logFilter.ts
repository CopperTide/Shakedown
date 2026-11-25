// Filtering a run's steps: by action, agent and outcome (any of the picked values of each), and by
// text in the detail. Kept in the address (`?action=…&agent=…&outcome=…&q=…`) so a filtered view
// can be shared; the event log shows only the matching steps and the timeline shows the same matching steps.
import type { LaneRow, TimelineMark } from '@/model/lanes';

/** One step as the event log lists it: its mark and the lane (agent) it is on. */
export interface LogRow { lane: string; m: TimelineMark }

export type Facet = 'action' | 'agent' | 'outcome';
export const FACETS: Facet[] = ['action', 'agent', 'outcome'];

export interface LogFilter {
  action: string[];
  agent: string[];
  outcome: string[];
  q: string;
}

export const NO_FILTER: LogFilter = { action: [], agent: [], outcome: [], q: '' };

/** Every step of `lanes` in step order (model calls only with `models`). */
export function logRows(lanes: LaneRow[], models: boolean): LogRow[] {
  const rows = lanes.flatMap((l) => l.marks.filter((m) => models || m.kind !== 'model').map((m) => ({ lane: l.name, m })));
  return rows.sort((a, b) => a.m.seq - b.m.seq);
}

/** The kind of action a step is: its label without whom a message went to ("Message → ops" is "Message"). */
export const actionOf = (m: TimelineMark) => m.label.replace(/\s*→.*$/, '');

/** A row's value for `facet`. */
export function valueOf(r: LogRow, facet: Facet): string {
  if (facet === 'action') return actionOf(r.m);
  if (facet === 'agent') return r.lane;
  return r.m.outcome;
}

const SEP = ',';
const listOf = (v: string | null) => (v ? v.split(SEP).map((s) => s.trim()).filter(Boolean) : []);

/** The filter in an address's query. */
export function parseFilter(sp: URLSearchParams): LogFilter {
  return { action: listOf(sp.get('action')), agent: listOf(sp.get('agent')), outcome: listOf(sp.get('outcome')), q: sp.get('q') ?? '' };
}

/** `prev` with the filter `f` in it (an empty part leaves the address). */
export function writeFilter(prev: URLSearchParams, f: LogFilter): URLSearchParams {
  const out = new URLSearchParams(prev);
  for (const k of FACETS) {
    if (f[k].length) out.set(k, f[k].join(SEP));
    else out.delete(k);
  }
  if (f.q.trim()) out.set('q', f.q);
  else out.delete('q');
  return out;
}

export const isActive = (f: LogFilter) => FACETS.some((k) => f[k].length > 0) || f.q.trim() !== '';

/** Whether `r` passes `f`, leaving out facet `skip` (for that facet's own counts). */
export function passes(r: LogRow, f: LogFilter, skip?: Facet): boolean {
  for (const k of FACETS) {
    if (k !== skip && f[k].length && !f[k].includes(valueOf(r, k))) return false;
  }
  const q = f.q.trim().toLowerCase();
  return !q || r.m.detail.toLowerCase().includes(q);
}

/** `value` picked or unpicked in `facet`. */
export function toggled(f: LogFilter, facet: Facet, value: string): LogFilter {
  const on = f[facet].includes(value);
  return { ...f, [facet]: on ? f[facet].filter((v) => v !== value) : [...f[facet], value] };
}

export interface Chip { value: string; count: number; on: boolean }

/**
 * Each facet's values with how many steps each would show given the rest of the filter, in
 * first-seen order. A picked value stays listed even when no step has it now.
 */
export function chipsOf(rows: LogRow[], f: LogFilter): Record<Facet, Chip[]> {
  const out = {} as Record<Facet, Chip[]>;
  for (const k of FACETS) {
    const counts = new Map<string, number>();
    for (const r of rows) {
      const v = valueOf(r, k);
      counts.set(v, (counts.get(v) ?? 0) + (passes(r, f, k) ? 1 : 0));
    }
    for (const v of f[k]) if (!counts.has(v)) counts.set(v, 0);
    out[k] = [...counts].map(([value, count]) => ({ value, count, on: f[k].includes(value) }));
  }
  return out;
}

/** The steps `f` keeps, or null when nothing is filtered. */
export function matching(rows: LogRow[], f: LogFilter): Set<number> | null {
  return isActive(f) ? new Set(rows.filter((r) => passes(r, f)).map((r) => r.m.seq)) : null;
}
