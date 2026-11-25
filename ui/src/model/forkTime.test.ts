import { describe, expect, it } from 'vitest';
import type { Event } from '@/model/types';
import { FORK_GAP_MS, forkShiftMs, onForkClock } from './forkTime';

const ev = (seq: number, at: string): Event => ({ seq, at, kind: 'tool_call', agent: 'a', payload: {} } as unknown as Event);

describe('a fork on its own clock', () => {
  // Inherited steps recorded on 22 Nov, the fork's own steps three days later.
  const events = [ev(1, '2026-11-22T21:00:00Z'), ev(2, '2026-11-22T21:05:00Z'), ev(3, '2026-11-25T07:50:40Z'), ev(4, '2026-11-25T07:51:40Z')];

  it('closes the gap between the fork point and its first own step', () => {
    expect(forkShiftMs(events, 3)).toBe(Date.parse('2026-11-25T07:50:40Z') - Date.parse('2026-11-22T21:05:00Z') - FORK_GAP_MS);
    const out = onForkClock(events, 3);
    expect(Date.parse(out[2].at) - Date.parse(out[1].at)).toBe(FORK_GAP_MS);
    expect(Date.parse(out[3].at) - Date.parse(out[2].at)).toBe(60_000); // spacing within the fork is kept
    expect(out[0].at).toBe(events[0].at); // inherited steps keep their times
  });

  it('leaves a run that is not a fork, or a fork that started right away, as it is', () => {
    expect(onForkClock(events, null)).toBe(events);
    const quick = [ev(1, '2026-11-22T21:00:00Z'), ev(2, '2026-11-22T21:00:00.500Z')];
    expect(forkShiftMs(quick, 2)).toBe(0);
    expect(onForkClock(quick, 2)).toBe(quick);
  });

  it('needs steps on both sides of the fork point', () => {
    expect(forkShiftMs(events.slice(2), 3)).toBe(0);
    expect(forkShiftMs(events.slice(0, 2), 3)).toBe(0);
  });
});
