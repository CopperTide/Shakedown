// Comparing any number of runs: the address that names them, their letters, and each one against
// the first, composed from the pairwise compare (`GET /api/compare?a=&b=`). Pure.
import type { BranchDetail, CompareResponse, CompareSide } from '@/model/types';

/** The branches a `?branches=a,b,c` names, each once, in order. */
export const branchesIn = (param: string | null): string[] => [...new Set((param ?? '').split(',').map((s) => s.trim()).filter(Boolean))];

/** Where comparing `ids` happens: two on the pairwise page (it has the differing rows), more side by side. */
export const compareHref = (ids: string[]): string =>
  ids.length === 2 ? `/compare/${ids[0]}/${ids[1]}` : `/compare?branches=${ids.join(',')}`;

/** A, B, … Z, then AA, AB, …: the letter of the branch at `i`. */
export const letterOf = (i: number): string =>
  (i >= 26 ? letterOf(Math.floor(i / 26) - 1) : '') + String.fromCharCode(65 + (i % 26));

/** A branch's figures from its own record, when no compare gave them. */
export const ownSide = (d: BranchDetail): CompareSide =>
  ({ status: d.status, seconds: null, tokens: d.metrics?.tokens ?? null, cost_usd: null, cost_source: null, metrics: d.metrics });

/** How a branch differs from the first one: where they first diverge and how many final rows differ, or why that is unknown. */
export type AgainstFirst = { first: number | null; rows: number; notCompared: string[] } | { error: string };

export const againstFirst = (r: CompareResponse | { error: string }): AgainstFirst =>
  'error' in r ? r : { first: r.first_divergent_seq, rows: r.differing_rows.length, notCompared: r.not_compared ?? [] };

/** "first diverges at step 12 · 3 differing rows", or why it can't be said. */
export function againstLine(a: AgainstFirst): string {
  if ('error' in a) return `Not compared: ${a.error}`;
  const where = a.first == null ? 'never diverges' : `first diverges at step ${a.first}`;
  const rows = `${a.rows} differing row${a.rows === 1 ? '' : 's'} at the end`;
  return [where, rows, a.notCompared.length ? `${a.notCompared.join(', ')} not compared` : ''].filter(Boolean).join(' · ');
}

/** Each branch's figures: the first's from any compare that worked, the others' from their own compare, else their record. */
export function sidesOf(details: BranchDetail[], pairs: (CompareResponse | { error: string })[]): CompareSide[] {
  const ok = pairs.filter((p): p is CompareResponse => !('error' in p));
  return details.map((d, i) => {
    if (i === 0) return ok[0]?.a ?? ownSide(d);
    const p = pairs[i - 1];
    return p && !('error' in p) ? p.b : ownSide(d);
  });
}
