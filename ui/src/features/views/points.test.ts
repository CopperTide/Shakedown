import { describe, expect, it } from 'vitest';
import type { Event, EventKind } from '@/model/types';
import { areaNames, gridStyle, isRectangular, templateAreas } from './layout';
import { atFor, isLive, viewStops } from './points';

const ev = (seq: number, kind: EventKind, payload: Record<string, unknown> = {}): Event =>
  ({ branch_id: 'b1', seq, at: '2026-01-01T00:00:00Z', agent: null, kind, payload } as unknown as Event);

const events: Event[] = [
  ev(1, 'run_start'),
  ev(2, 'tool_call', { name: 't1', clock_ms: 0 }),
  ev(3, 'model_call'),
  ev(4, 'world_event', { kind: 'k1', clock_ms: 60_000 }),
  ev(5, 'tool_call', { name: 't2' }),
  ev(6, 'tool_call', { name: 't3', clock_ms: 120_000 }),
  ev(7, 'check', { passed: true }),
];

describe('view stops', () => {
  it('a_stop_per_clocked_event_then_final_at_the_check', () => {
    expect(viewStops(events)).toEqual([
      { seq: 2, at: '2', clockMs: 0 },
      { seq: 4, at: '4', clockMs: 60_000 },
      { seq: 6, at: '6', clockMs: 120_000 },
      { seq: 7, at: 'final', clockMs: 120_000 },
    ]);
  });

  it('no_final_without_a_check_and_one_final_with_several', () => {
    expect(viewStops(events.slice(0, 6)).map((s) => s.at)).toEqual(['2', '4', '6']);
    expect(viewStops([...events, ev(9, 'check', {})]).filter((s) => s.at === 'final').map((s) => s.seq)).toEqual([9]);
  });

  it('the_step_in_the_address_snaps_to_a_stop', () => {
    const stops = viewStops(events);
    expect(atFor(stops, null)).toEqual({ at: 'final', seq: 7 });
    expect(atFor(stops, 5)).toEqual({ at: '4', seq: 4 });
    expect(atFor(stops, 1)).toEqual({ at: '2', seq: 2 });
    expect(atFor([], 3)).toEqual({ at: null, seq: null });
  });

  it('live_while_running', () => {
    expect(isLive('running')).toBe(true);
    expect(isLive('passed')).toBe(false);
    expect(isLive(undefined)).toBe(false);
  });
});

describe('screen layout', () => {
  const screen = { id: 's1', label: 'S1', areas: ['k1  k1', 'm1 f1', 'm1 .'], columns: '3fr 1fr', rows: 'auto 1fr 1fr' };

  it('names_each_panel_once_skipping_empty_cells', () => {
    expect(areaNames(screen)).toEqual(['k1', 'm1', 'f1']);
  });

  it('builds_the_css_grid_from_the_file', () => {
    expect(templateAreas(screen.areas)).toBe('"k1 k1" "m1 f1" "m1 ."');
    expect(gridStyle(screen)).toEqual({ gridTemplateAreas: '"k1 k1" "m1 f1" "m1 ."', gridTemplateColumns: '3fr 1fr', gridTemplateRows: 'auto 1fr 1fr' });
    expect(gridStyle({ id: 's2', label: 'S2', areas: ['a b'] })).toMatchObject({ gridTemplateColumns: 'repeat(2, minmax(0, 1fr))', gridTemplateRows: 'repeat(1, minmax(0, 1fr))' });
  });

  it('stacks_the_panels_when_the_areas_are_ragged', () => {
    expect(isRectangular(['a b', 'c'])).toBe(false);
    expect(gridStyle({ id: 's3', label: 'S3', areas: ['a b', 'c'] })).toMatchObject({ gridTemplateAreas: '"a" "b" "c"', gridTemplateColumns: '1fr' });
  });
});
