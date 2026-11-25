import { describe, expect, it } from 'vitest';
import { worldNow } from './geo';
import { movingAt } from './mapLayers';
import { RATES, playAt, playClock, prefetchTargets, stopAtClock, stopToShow } from './playback';
import type { Stop } from './points';
import type { At } from './types';

const MIN = 60_000;
const EPOCH = Date.UTC(2026, 7, 13, 4);
const stops: Stop[] = [
  { seq: 2, at: '2', clockMs: 0 },
  { seq: 5, at: '5', clockMs: MIN },
  { seq: 6, at: '6', clockMs: MIN },
  { seq: 9, at: '9', clockMs: 10 * MIN },
  { seq: 12, at: 'final', clockMs: 10 * MIN },
];
const at: At = { seq: 5, point: 'step', clock_ms: MIN, world_ms: EPOCH + MIN, speed: 0, paused: false, utc_offset_min: -240 };

describe('recorded playback', () => {
  it('the_stop_on_screen_is_the_last_one_the_clock_has_reached', () => {
    expect(stopAtClock(stops, -1)).toBe(-1);
    expect(stopAtClock(stops, 0)).toBe(0);
    expect(stopAtClock(stops, MIN - 1)).toBe(0);
    expect(stopAtClock(stops, MIN)).toBe(2);
    expect(stopAtClock(stops, 1e12)).toBe(4);
  });

  it('the_virtual_clock_runs_at_the_rate_and_stops_at_the_end', () => {
    expect(RATES[0].worldMsPerMs).toBe(360); // an hour of world time in 10 s
    expect(playClock({ clockMs: 1000, perf: 50 }, 360, 150, 10 * MIN)).toBe(1000 + 36_000);
    expect(playClock({ clockMs: 1000, perf: 50 }, 360, 1e9, 10 * MIN)).toBe(10 * MIN);
    expect(playClock({ clockMs: 1000, perf: 50 }, 360, 0, 10 * MIN)).toBe(1000);
  });

  it('aircraft_glide_between_stops_on_the_virtual_clock', () => {
    const anchor = { clockMs: MIN, perf: 1000 };
    const shown = playAt(at, anchor, 360);
    expect(shown).toMatchObject({ clock_ms: MIN, world_ms: EPOCH + MIN, speed: 360, utc_offset_min: -240 });
    const flight = { key: 'f', label: 'F1', o: [-77.46, 38.94] as [number, number], d: [-87.9, 41.98] as [number, number],
      depart: EPOCH + MIN, arrive: EPOCH + 11 * MIN, tone: 'active' as const, chip: '' };
    const now = (perf: number) => worldNow(shown.world_ms, shown.speed, anchor.perf, perf)!;
    expect(movingAt(flight, now(1000)).t).toBe(0);
    expect(movingAt(flight, now(1000 + (5 * MIN) / 360)).t).toBeCloseTo(0.5);
    expect(movingAt(flight, now(1000 + (10 * MIN) / 360)).t).toBeCloseTo(1);
    expect(playAt(at, anchor, 0).speed).toBe(0);
  });

  it('a_newer_loaded_stop_replaces_the_one_on_screen_never_an_older_one', () => {
    expect(stopToShow(0, 3, (i) => i === 2)).toBe(2);
    expect(stopToShow(0, 3, (i) => i === 1 || i === 3)).toBe(3);
    expect(stopToShow(2, 3, () => false)).toBe(2);
    expect(stopToShow(3, 1, () => true)).toBe(3);
  });

  it('fetches_the_stops_the_clock_will_reach_while_they_load', () => {
    expect(prefetchTargets(stops, 0, 360, 200, 0)).toEqual([2]);
    expect(prefetchTargets(stops, 0, 360, 1000, 0)).toEqual([2, 4]);
    expect(prefetchTargets(stops, 10 * MIN, 360, 200, 4)).toEqual([]);
    expect(prefetchTargets(stops, 5 * MIN, 360, 10, 0)).toEqual([2]);
  });
});
