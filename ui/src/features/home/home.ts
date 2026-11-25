// What Home shows about runs: this week's numbers and what needs attention. Pure, so it is tested
// without a page.
import type { Branch, RunSummary } from '@/model/types';

const WEEK_MS = 7 * 86_400_000;

/**
 * How a run ends when it did not pass: its checks failed, it went over its spend cap, or it ended
 * with an error, interrupted or stopped. `budget_exceeded` is shaked's too, though not yet a
 * `BranchStatus` here.
 */
const NOT_PASSED: ReadonlySet<string> = new Set(['failed', 'budget_exceeded', 'infra_error', 'interrupted', 'stopped']);

/** The branch ended without passing (see `NOT_PASSED`). */
export const failedRun = (b: Pick<Branch, 'status'>) => NOT_PASSED.has(b.status);

export interface WeekStats {
  runs: number;
  passed: number;
  /** Runs that ended without passing: checks failed, over budget, an error, interrupted or stopped. */
  failed: number;
  running: number;
  /** Passed over finished (passed + failed); null when none finished. */
  passRate: number | null;
}

/** The runs started in the 7 days before `now`, counted by their first branch's status. */
export function weekStats(runs: RunSummary[], now: number): WeekStats {
  const recent = runs.filter((r) => now - Date.parse(r.created_at) <= WEEK_MS);
  const count = (is: (b: Branch) => boolean) => recent.filter((r) => r.branches[0] && is(r.branches[0])).length;
  const passed = count((b) => b.status === 'passed');
  const failed = count(failedRun);
  const finished = passed + failed;
  return { runs: recent.length, passed, failed, running: count((b) => b.status === 'running'), passRate: finished ? passed / finished : null };
}

/** Branches that ended without passing, newest first, at most five. */
export function attention(runs: RunSummary[]): Branch[] {
  return runs
    .flatMap((r) => r.branches.filter(failedRun))
    .sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at))
    .slice(0, 5);
}

/** "Good morning", "Good afternoon" or "Good evening" for `hour`. */
export function greeting(hour: number): string {
  if (hour < 12) return 'Good morning';
  return hour < 18 ? 'Good afternoon' : 'Good evening';
}
