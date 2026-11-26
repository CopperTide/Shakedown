// Search over every run's steps: the words and the kind and agent picks, kept in the Analytics
// page's address (`?tab=search&q=crew&kind=reasoning&agent=ops`); the API query they make; the
// hits grouped by run; a snippet split at its marked matches; and where a hit opens. Pure functions only.
import type { SearchHit } from '@/api/analytics';
import { path } from '@/lib/url';

export interface StepSearch { q: string; kind: string[]; agent: string[] }
export const NO_SEARCH: StepSearch = { q: '', kind: [], agent: [] };

/** The kinds of step search finds, as the filter names them. */
export const KIND_LABEL: Record<string, string> = {
  reasoning: 'Reasoning', message: 'Message', tool_call: 'Tool call', delegation: 'Delegation', report: 'Report',
  handoff: 'Handoff', wake: 'Wake', idle: 'Turn end', finish: 'Finish', agent_end: 'Agent end', check: 'Check',
};
export const kindLabel = (k: string) => KIND_LABEL[k] ?? k.replace(/_/g, ' ');

const unique = (v: string[]) => [...new Set(v.filter(Boolean))];

/** The search in `sp`. */
export function parseSearch(sp: URLSearchParams): StepSearch {
  return { q: sp.get('q') ?? '', kind: unique(sp.getAll('kind')), agent: unique(sp.getAll('agent')) };
}

/** `prev` with `s` written in (empty parts left out, other keys kept). */
export function writeSearch(prev: URLSearchParams, s: StepSearch): URLSearchParams {
  const sp = new URLSearchParams(prev);
  for (const k of ['q', 'kind', 'agent']) sp.delete(k);
  if (s.q.trim()) sp.set('q', s.q);
  for (const k of s.kind) sp.append('kind', k);
  for (const a of s.agent) sp.append('agent', a);
  return sp;
}

/** The API's query string for `s` from `offset` (each list a JSON array); null with no words. */
export function searchQuery(s: StepSearch, offset = 0): string | null {
  const q = s.q.trim();
  if (!q) return null;
  const params = new URLSearchParams({ q });
  if (s.kind.length) params.set('kind', JSON.stringify(s.kind));
  if (s.agent.length) params.set('agent', JSON.stringify(s.agent));
  if (offset > 0) params.set('offset', String(offset));
  return `?${params.toString()}`;
}

/** `list` with `v` added, or taken out when it is there. */
export const toggled = (list: string[], v: string) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);

/** Where a match starts and stops in a snippet. */
export const HIT_START = '';
export const HIT_END = '';

export interface SnippetPart { text: string; hit: boolean }

/** A snippet as plain and matched runs of text, in order (an unclosed match runs to the end). */
export function snippetParts(snippet: string): SnippetPart[] {
  const out: SnippetPart[] = [];
  for (const [i, piece] of snippet.split(HIT_START).entries()) {
    const [hit, rest] = i === 0 ? [null, piece] : splitOnce(piece, HIT_END);
    if (hit) out.push({ text: hit, hit: true });
    if (rest) out.push({ text: rest, hit: false });
  }
  return out;
}

function splitOnce(s: string, sep: string): [string, string] {
  const at = s.indexOf(sep);
  return at < 0 ? [s, ''] : [s.slice(0, at), s.slice(at + sep.length)];
}

export interface RunGroup { runId: string; runName: string; hits: SearchHit[] }

/** Hits grouped by run, runs in the order their first hit came (newest first), hits kept in order. */
export function byRun(hits: SearchHit[]): RunGroup[] {
  const groups = new Map<string, RunGroup>();
  for (const h of hits) {
    const g = groups.get(h.run_id) ?? { runId: h.run_id, runName: h.run_name, hits: [] };
    g.hits.push(h);
    groups.set(h.run_id, g);
  }
  return [...groups.values()];
}

/** The run page at the hit's step. */
export const stepHref = (h: SearchHit) => `${path`/runs/${h.run_id}/${h.branch_id}`}?step=${h.seq}`;

/** The Analytics page's Search tab for `q` (the palette's way in). */
export const searchHref = (q: string) => `/analytics?${new URLSearchParams({ tab: 'search', q }).toString()}`;
