import { renderToStaticMarkup } from 'react-dom/server';
import { createElement } from 'react';
import { describe, expect, it } from 'vitest';
import { viewStops } from '@/features/views/points';
import ViewScreen from '@/features/views/ViewScreen';
import { team } from '@/features/world/fixtures';
import { DEFS, WV_LIVE, WV_RECORDED, syntheticAt, syntheticScreen, viewBranches } from './worldViews';

describe('synthetic world views (dev fixtures)', () => {
  const branches = viewBranches(team);

  it('every_screen_draws_without_a_panel_error', () => {
    const view = DEFS.views[0];
    for (const screen of view.screens) {
      const point = syntheticAt('20', branches[WV_RECORDED]);
      const data = syntheticScreen(screen.id, point.clock, point.at);
      expect(data.screen).toBe(screen.id);
      const html = renderToStaticMarkup(createElement(ViewScreen, { view, screen, data, fetchedAt: 0 }));
      expect(html).not.toContain('Core sent');
      expect(html).not.toContain('no panel by this name');
    }
  });

  it('recorded_has_stops_and_a_final_and_live_runs', () => {
    const stops = viewStops(branches[WV_RECORDED].events);
    expect(stops.length).toBeGreaterThan(10);
    expect(stops.at(-1)?.at).toBe('final');
    expect(branches[WV_LIVE].status).toBe('running');
    expect(syntheticAt('live', branches[WV_LIVE]).at.speed).toBeGreaterThan(0);
    expect(syntheticAt('final', branches[WV_RECORDED]).at.point).toBe('final');
  });
});
