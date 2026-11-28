// What the run page says once a rerun starts: where it really started, and whether Shakedown
// started the team's own driver again (a run a Code connection drove).
import type { ForkResponse } from '@/model/types';

export function forkNotice(r: ForkResponse): string | null {
  const parts: string[] = [];
  if (r.moved_from != null) parts.push(`Restarted from step ${r.start_seq} instead of ${r.moved_from}: ${r.reason ?? ''}`);
  if (r.driver === 'relaunched') parts.push(`Driver restarted at step ${r.start_seq}.`);
  if (r.driver === 'failed') parts.push("Driver didn't restart: the rerun failed.");
  return parts.length ? parts.join(' ') : null;
}
