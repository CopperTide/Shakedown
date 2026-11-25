import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { fillOf } from './BarsPanel';
import { feedTime } from './FeedPanel';
import { zoneOf } from './format';
import type { ScreenData, ViewDef } from './types';
import ViewScreen, { slotOf } from './ViewScreen';

const H = 3_600_000;
const T0 = Date.UTC(2026, 0, 1);

const view: ViewDef = {
  id: 'v1', label: 'View one',
  panels: {
    k1: { kind: 'kpis', items: [{ label: 'Rate', format: 'percent' }, { label: 'State', format: 'chip' }, { label: 'Spend', format: 'usd' }] },
    b1: { kind: 'board', title: 'Items', columns: [{ field: 'id', label: 'Item' }, { field: 'status', label: 'Status', format: 'chip' }], filters: [{ id: 'f1', label: 'Odd' }] },
    t1: { kind: 'timeline', title: 'Lanes' },
    r1: { kind: 'bars', title: 'Limits', unit: 'h' },
    f1: { kind: 'feed', title: 'Events' },
    e1: { kind: 'board', title: 'Broken', columns: [] },
  },
  screens: [{ id: 's1', label: 'S1', areas: ['k1 k1', 'b1 t1', 'r1 f1', 'e1 x1'] }],
};

const data: ScreenData = {
  view: 'v1', screen: 's1',
  at: { seq: 12, point: 'step', clock_ms: 2 * H, world_ms: T0 + 2 * H, speed: 0, paused: false },
  panels: {
    k1: { kind: 'kpis', items: [{ value: 81.25, tone: 'warn' }, { value: 'x', tone: 'critical', chip: 'Bad' }, { value: 1234, tone: 'neutral' }] },
    b1: { kind: 'board', columns: ['key', 'status', 'id', 'tone', 'chip', 'filters'], rows: [['a1', 'ok', 'a1', 'neutral', null, 1], ['a2', 'held', 'a2', 'critical', 'Held', 0]], total: 5 },
    t1: { kind: 'timeline', lanes: ['u1'], columns: ['lane', 'key', 'label', 'start', 'end', 'tone', 'chip'], rows: [[0, 'k1', 'Bar one', T0 + H, T0 + 3 * H, 'active', null]], range: [T0, T0 + 4 * H] },
    r1: { kind: 'bars', columns: ['key', 'label', 'value', 'limit', 'ratio', 'tone', 'chip'], rows: [['r1', 'R1', 9, 10, 0.9, 'warn', null]] },
    f1: { kind: 'feed', columns: ['seq', 'clock_ms', 'part', 'kind', 'text', 'tone', 'chip'], rows: [[11, 90 * 60_000, 'p1', 'k1', 'Something happened', 'critical', null]] },
    e1: { kind: 'board', error: 'part p9 has no doc d1' },
  },
};

describe('view screen', () => {
  const html = renderToStaticMarkup(<ViewScreen view={view} screen={view.screens[0]} data={data} fetchedAt={0} />);

  it('places_each_panel_in_its_grid_area', () => {
    for (const id of ['k1', 'b1', 't1', 'r1', 'f1', 'e1', 'x1']) expect(html).toContain(`data-panel="${id}"`);
    expect(html).toContain('grid-area:k1');
    expect(html).toContain('grid-template-areas:&quot;k1 k1&quot; &quot;b1 t1&quot; &quot;r1 f1&quot; &quot;e1 x1&quot;');
  });

  it('draws_kpis_in_file_order_with_formats_and_chips', () => {
    expect(html).toContain('81.3%');
    expect(html).toContain('Bad');
    expect(html).toContain('$1,234');
    expect(html.indexOf('Rate')).toBeLessThan(html.indexOf('State'));
  });

  it('draws_the_board_its_count_filters_and_chips', () => {
    expect(html).toContain('2 of 5');
    expect(html).toContain('Odd');
    expect(html).toContain('Held');
  });

  it('draws_timeline_bars_feed_and_their_times', () => {
    expect(html).toContain('u1');
    expect(html).toContain('01:00Z – 03:00Z');
    expect(html).toContain('9 h');
    expect(html).toContain('90%');
    expect(html).toContain('01:30Z');
    expect(html).toContain('Something happened');
  });

  it('an_error_stays_in_its_panel_and_a_missing_panel_says_so', () => {
    expect(html).toContain('part p9 has no doc d1');
    expect(html).toContain('The view has no panel by this name.');
  });

  it('panels_wait_until_the_first_screen_comes', () => {
    const waiting = renderToStaticMarkup(<ViewScreen view={view} screen={view.screens[0]} data={null} fetchedAt={0} />);
    expect(waiting).toContain('Loading…');
    expect(waiting).not.toContain('part p9');
  });
});

describe('panel slots and helpers', () => {
  it('slot_of', () => {
    expect(slotOf(undefined, undefined, true)).toEqual({ error: 'The view has no panel by this name.' });
    expect(slotOf(view.panels.b1, undefined, false)).toBeNull();
    expect(slotOf(view.panels.b1, undefined, true)).toEqual({ error: 'Core sent nothing for this panel.' });
    expect(slotOf(view.panels.b1, { kind: 'feed', columns: [], rows: [] }, true)).toEqual({ error: 'Core sent a feed for a board panel.' });
  });

  it('bars_fill_with_the_limit_at_the_same_place', () => {
    expect(fillOf(1)).toBeCloseTo(0.8);
    expect(fillOf(0.5)).toBeCloseTo(0.4);
    expect(fillOf(3)).toBe(1);
    expect(fillOf(null)).toBe(0);
  });

  it('feed_time_from_the_world_epoch', () => {
    expect(feedTime(90 * 60_000, { world_ms: T0 + 2 * H, clock_ms: 2 * H })).toBe('01:30Z');
    expect(feedTime(90 * 60_000, null)).toBe('+1:30');
    expect(feedTime(null, null)).toBe('');
    expect(feedTime(90 * 60_000, { world_ms: T0 + 2 * H, clock_ms: 2 * H }, zoneOf(-240))).toBe('21:30');
  });
});
