// What Home shows about runs: this week's numbers and what needs attention. Pure, so it is tested
// without a page.
import type { Branch, RunSummary } from '@/model/types';

const WEEK_MS = 7 * 86_400_000;

export interface WeekStats {
  runs: number;
  passed: number;
  failed: number;
  running: number;
  /** Passed over finished (passed + failed); null when none finished. */
  passRate: number | null;
}

/** The runs started in the 7 days before `now`, counted by their first branch's status. */
export function weekStats(runs: RunSummary[], now: number): WeekStats {
  const recent = runs.filter((r) => now - Date.parse(r.created_at) <= WEEK_MS);
  const count = (s: string) => recent.filter((r) => r.branches[0]?.status === s).length;
  const passed = count('passed');
  const failed = count('failed');
  const finished = passed + failed;
  return { runs: recent.length, passed, failed, running: count('running'), passRate: finished ? passed / finished : null };
}

/** Failed branches, newest first, at most five. */
export function attention(runs: RunSummary[]): Branch[] {
  return runs
    .flatMap((r) => r.branches.filter((b) => b.status === 'failed'))
    .sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at))
    .slice(0, 5);
}

/** "Good morning", "Good afternoon" or "Good evening" for `hour`. */
export function greeting(hour: number): string {
  if (hour < 12) return 'Good morning';
  return hour < 18 ? 'Good afternoon' : 'Good evening';
}
