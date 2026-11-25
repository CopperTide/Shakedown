// An experiment's per-run spend cap, as the form edits it: the typed text goes into the body as it
// is read (a bad entry included, so the form can refuse it), and comes out as text again.
import type { ExperimentBody } from '@/model/project';

/** The cap the text says: none when empty, else the number it reads as (NaN when it isn't one). */
export const capOf = (text: string): number | undefined => (text.trim() ? Number(text.trim()) : undefined);

/** The cap as the form shows it: empty for none. */
export const capText = (cap: number | undefined) => (cap == null || Number.isNaN(cap) ? '' : String(cap));

/** Two caps are the same, NaN included. */
export const sameCap = (a: number | undefined, b: number | undefined) => Object.is(a, b);

/** The body with its per-run cap set, or removed (and an emptied `budget` with it). */
export function withRunCap(body: ExperimentBody, cap: number | undefined): ExperimentBody {
  const budget = { ...body.budget, per_run_usd: cap };
  if (cap === undefined) delete budget.per_run_usd;
  const next: ExperimentBody = { ...body, budget };
  if (!Object.keys(budget).length) delete next.budget;
  return next;
}

/** Why the body's per-run cap can't be saved, if it can't: it must be a positive number of dollars. */
export function capProblem(body: ExperimentBody): string | null {
  const cap = body.budget?.per_run_usd ?? undefined;
  if (cap === undefined) return null;
  return Number.isFinite(cap) && cap > 0 ? null : 'The spend cap is a positive number of dollars, or empty for no cap';
}
