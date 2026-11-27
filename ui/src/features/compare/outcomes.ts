// The compare pages' outcome table: what each branch achieved, read from what the API already
// returns (the check's named checks and what they measured, the metrics, the events and the last
// view of the twin). The rows are the ones any branch has: no world's figures are built in. Two
// branches (A and B) or any number side by side.
import { measureText } from '@/features/experiments/measures';
import { costLabel } from '@/features/run/timeline';
import { usd as dollars } from '@/lib/format';
import { viewsFromEvents } from '@/features/world/twin';
import { actionsOf } from '@/model/actions';
import { lastPayload } from '@/model/events';
import type { TwinView } from '@/model/twin';
import type { BranchDetail, CompareSide, Event, NamedCheck } from '@/model/types';

/** One figure of a branch: the text shown, a number to compare (null: not comparable) and whether it is abnormal. */
export interface Figure {
  text: string;
  value: number | null;
  abnormal: boolean;
  title?: string;
}

export interface OutcomeRow {
  /** Stable per row: `checks`, `safety`, `measure:<check>`, `kpi:<id>`, `agent_cost` or `escalations`. */
  id: string;
  label: string;
  hint: string;
  /** Which way is better, for the verdict; null when neither is (a count, a twin's figure). */
  better: 'higher' | 'lower' | null;
  a: Figure;
  b: Figure;
}

const NONE: Figure = { text: '–', value: null, abnormal: false };

/** The named checks of the branch's last check, or null when it was not scored. */
export function namedChecks(events: Event[]): { passed: boolean; checks: NamedCheck[] } | null {
  const check = lastPayload(events, 'check');
  if (!check) return null;
  return { passed: !!check.passed, checks: Array.isArray(check.checks) ? check.checks : [] };
}

/** "✓ 9/9" when every goal check (the non-safety ones) passed, "✕ 8/9" when one failed. */
export function checksPassed(events: Event[]): Figure {
  const c = namedChecks(events);
  if (!c) return { ...NONE, text: 'not scored' };
  const goals = c.checks.filter((x) => !x.safety);
  if (!goals.length) return { text: c.passed ? '✓ passed' : '✕ failed', value: c.passed ? 1 : 0, abnormal: !c.passed };
  const ok = goals.filter((x) => x.passed).length;
  const failed = goals.filter((x) => !x.passed).map((x) => x.name);
  return { text: `${failed.length ? '✕' : '✓'} ${ok}/${goals.length}`, value: ok / goals.length, abnormal: failed.length > 0, title: failed.length ? `Failed: ${failed.join(', ')}` : undefined };
}

/** "◆ 1 of 4" safety checks violated. */
export function safetyViolations(events: Event[]): Figure {
  const safety = namedChecks(events)?.checks.filter((x) => x.safety) ?? [];
  if (!safety.length) return { ...NONE, text: 'not scored' };
  const bad = safety.filter((x) => !x.passed);
  return { text: `${bad.length ? '◆ ' : ''}${bad.length} of ${safety.length}`, value: bad.length, abnormal: bad.length > 0, title: bad.map((x) => x.name).join(', ') || undefined };
}

/** The twin's last view in the events (a fork's events include those it inherits). */
export function lastView(events: Event[]): TwinView | null {
  return viewsFromEvents(events).at(-1)?.view ?? null;
}

/** A KPI of the twin by id. */
const kpi = (view: TwinView | null, id: string) => view?.kpis.find((k) => k.id === id);

/** The leading number of a figure such as "8 of 10" or "USD 1,240". */
export function leadingNumber(s: string): number | null {
  const m = /-?\d[\d,]*(\.\d+)?/.exec(s);
  return m ? Number(m[0].replace(/,/g, '')) : null;
}

/** A KPI of the twin as a figure; "–" when the twin has none of that id. */
export function kpiFigure(view: TwinView | null, id: string): Figure {
  const k = kpi(view, id);
  return k ? { text: k.value, value: leadingNumber(k.value), abnormal: k.abnormal } : NONE;
}

/** What check `name` measured in the branch, as a figure (abnormal when the check failed); "–" when it measured nothing. */
export function measureFigure(events: Event[], name: string): Figure {
  const check = namedChecks(events)?.checks.find((c) => c.name === name);
  const m = check?.measure;
  if (!m) return NONE;
  return { text: measureText(m.value, m.unit ?? ''), value: m.value, abnormal: !check.passed, title: check.detail || undefined };
}

/**
 * What the agents cost as they reported it: the branch's own agent ends (their `cost_usd`), else
 * the metrics' cost, labelled by where the figure came from.
 */
export function agentCost(s: CompareSide): Figure {
  const usd = s.cost_usd ?? s.metrics?.cost.usd ?? null;
  const source = s.cost_source ?? s.metrics?.cost.source ?? null;
  const tokens = s.tokens != null ? ` · ${s.tokens.toLocaleString('en-US')} tokens` : '';
  if (usd == null) return { text: `not reported${tokens}`, value: null, abnormal: false };
  return { text: `${dollars(usd)} (${costLabel(source) || 'reported'})${tokens}`, value: usd, abnormal: false, title: source ?? undefined };
}

/** Approval requests: every write an agent made to the approvals system, refused or not. */
export function escalations(events: Event[]): Figure {
  const n = actionsOf(events).filter((a) => a.system?.kind === 'approvals' && a.effect === 'write' && a.agent !== 'link').length;
  return { text: String(n), value: n, abnormal: false };
}

export interface SideInput {
  detail: BranchDetail;
  side: CompareSide;
  /** The twin at the end, from the views endpoint; absent: the last view in the events. */
  view?: TwinView | null;
}

type Spec = Pick<OutcomeRow, 'id' | 'label' | 'hint' | 'better'> & { of: (s: SideInput) => Figure };

const viewOf = (s: SideInput) => (s.view === undefined ? lastView(s.detail.events) : s.view);

const CHECKS: Spec = { id: 'checks', label: 'Checks passed', hint: 'the goal checks (not safety) that passed', better: 'higher', of: (s) => checksPassed(s.detail.events) };
const SAFETY: Spec = { id: 'safety', label: 'Safety violations', hint: 'safety checks failed', better: 'lower', of: (s) => safetyViolations(s.detail.events) };
const AGENT_COST: Spec = { id: 'agent_cost', label: 'Agent cost', hint: 'as the agents reported it', better: 'lower', of: (s) => agentCost(s.side) };
const ESCALATIONS: Spec = { id: 'escalations', label: 'Escalations', hint: 'approval requests', better: 'lower', of: (s) => escalations(s.detail.events) };

/** A row per figure a check measured in either branch, in the order the checks first appear. */
function measureSpecs(checks: NamedCheck[]): Spec[] {
  const seen = new Map<string, Spec>();
  for (const c of checks) {
    if (!c.measure || seen.has(c.name)) continue;
    const { label, better } = c.measure;
    seen.set(c.name, { id: `measure:${c.name}`, label, hint: c.name, better: better ?? null, of: (s) => measureFigure(s.detail.events, c.name) });
  }
  return [...seen.values()];
}

/** A row per KPI either branch's twin shows at the end (a twin's figure says no better way). */
function kpiSpecs(views: (TwinView | null)[]): Spec[] {
  const seen = new Map<string, Spec>();
  for (const k of views.flatMap((v) => v?.kpis ?? [])) {
    if (seen.has(k.id)) continue;
    seen.set(k.id, { id: `kpi:${k.id}`, label: k.label, hint: 'the twin at the end', better: null, of: (s) => kpiFigure(viewOf(s), k.id) });
  }
  return [...seen.values()];
}

/**
 * The rows the branches have: their checks (passed, safety, each figure a check measured), the
 * KPIs of their twins, what the agents cost and, when an agent asked for approval, the escalations.
 */
function specsOf(sides: SideInput[]): Spec[] {
  const scored = sides.map((s) => namedChecks(s.detail.events));
  const checks = scored.flatMap((c) => c?.checks ?? []);
  const asked = sides.some((s) => (escalations(s.detail.events).value ?? 0) > 0);
  return [
    ...(scored.some(Boolean) ? [CHECKS] : []),
    ...(checks.some((c) => c.safety) ? [SAFETY] : []),
    ...measureSpecs(checks),
    ...kpiSpecs(sides.map(viewOf)),
    AGENT_COST,
    ...(asked ? [ESCALATIONS] : []),
  ];
}

/** One outcome across any number of branches: a figure per branch, in their order. */
export interface OutcomeLine extends Pick<OutcomeRow, 'id' | 'label' | 'hint' | 'better'> {
  values: Figure[];
}

/** The outcome rows of any number of branches side by side. */
export function outcomeLines(sides: SideInput[]): OutcomeLine[] {
  return specsOf(sides).map(({ of, ...spec }) => ({ ...spec, values: sides.map(of) }));
}

/** Which branch did best on a row (its index): null when no way is better, fewer than two compare, or the best ties. */
export function bestOf(row: Pick<OutcomeLine, 'better' | 'values'>): number | null {
  const sign = row.better === 'higher' ? 1 : row.better === 'lower' ? -1 : 0;
  const scored = row.values.flatMap((f, i) => (f.value == null ? [] : [{ i, v: f.value * sign }]));
  if (!sign || scored.length < 2) return null;
  const top = Math.max(...scored.map((x) => x.v));
  const at = scored.filter((x) => x.v === top);
  return at.length === 1 ? at[0].i : null;
}

export function outcomeRows(a: SideInput, b: SideInput): OutcomeRow[] {
  return outcomeLines([a, b]).map(({ values: [fa, fb], ...spec }) => ({ ...spec, a: fa, b: fb }));
}

/** Which side did better on a row, or null when equal or not comparable. */
export function verdict(row: OutcomeRow): 'a' | 'b' | null {
  const best = bestOf({ better: row.better, values: [row.a, row.b] });
  return best == null ? null : best === 0 ? 'a' : 'b';
}
