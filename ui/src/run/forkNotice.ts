// What the run page says once a rerun starts: where it really started, and whether Shakedown
// started the team's own driver again (a run a Code connection drove).
import type { ForkResponse } from '../types';

export function forkNotice(r: ForkResponse): string | null {
  const parts: string[] = [];
  if (r.moved_from != null) parts.push(`Restarted from step ${r.start_seq} instead of ${r.moved_from}: ${r.reason ?? ''}`);
  if (r.driver === 'relaunched') parts.push(`Agents restarted: Shakedown started your driver again on the world as it was at step ${r.start_seq}.`);
  if (r.driver === 'failed') parts.push("Your driver couldn't be started again, so the rerun ended failed (see its error).");
  return parts.length ? parts.join(' ') : null;
}
