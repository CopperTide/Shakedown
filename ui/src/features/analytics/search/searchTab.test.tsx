// @vitest-environment jsdom
// The Search tab: the words in the address reach the API, hits show grouped by run with the
// match marked, a kind chip narrows the search, and a hit links to its run page step.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { SearchResults } from '@/api/analytics';
import SearchTab from './SearchTab';
import { HIT_END, HIT_START } from './searchModel';

const RESULTS: SearchResults = {
  hits: [
    { run_id: 'r1', run_name: 'Hub day', branch_id: 'b1', seq: 42, kind: 'reasoning', agent: 'ops-controller', at: '2026-11-25T10:00:00Z', snippet: `short of ${HIT_START}crew${HIT_END} at AUS` },
    { run_id: 'r2', run_name: 'Storm', branch_id: 'b2', seq: 7, kind: 'tool_call', agent: 'crew-controller', at: '2026-11-25T09:00:00Z', snippet: `${HIT_START}crew${HIT_END}_status` },
    { run_id: 'r1', run_name: 'Hub day', branch_id: 'b1', seq: 3, kind: 'message', agent: 'duty-manager', at: '2026-11-25T08:00:00Z', snippet: 'find a crew' },
  ],
  total: 3, total_capped: false, next_offset: null,
  kinds: [{ name: 'reasoning', count: 1 }, { name: 'tool_call', count: 1 }, { name: 'message', count: 1 }],
  agents: [{ name: 'ops-controller', count: 1 }],
};

const asked: URLSearchParams[] = [];
function serve() {
  asked.length = 0;
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    asked.push(new URL(String(url), 'http://x').searchParams);
    return new Response(JSON.stringify(RESULTS));
  }));
}

function Where() {
  const l = useLocation();
  return <output data-testid="where">{l.pathname + l.search}</output>;
}

const show = (at: string) => render(
  <MemoryRouter initialEntries={[at]}>
    <Routes><Route path="/analytics" element={<SearchTab />} /><Route path="*" element={null} /></Routes>
    <Where />
  </MemoryRouter>,
);

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('the Search tab', () => {
  it('with no words, says what it finds and asks nothing', () => {
    serve();
    show('/analytics?tab=search');
    expect(screen.getByText('Find any step in any run')).toBeTruthy();
    expect(asked).toHaveLength(0);
  });

  it('shows hits grouped by run, marks the match, and links each to its step', async () => {
    serve();
    show('/analytics?tab=search&q=crew');
    expect(await screen.findByRole('heading', { name: 'Hub day' })).toBeTruthy();
    expect(asked[0].get('q')).toBe('crew');
    expect(screen.getAllByRole('heading', { level: 3 }).map((h) => h.textContent)).toEqual(['Hub day', 'Storm']);
    expect(screen.getAllByText('crew', { selector: 'mark' })).toHaveLength(2);
    const step = screen.getByText('step 42').closest('a')!;
    expect(step.getAttribute('href')).toBe('/runs/r1/b1?step=42');
    expect(screen.getByText(/matching steps/).textContent).toMatch(/3 matching steps/);
  });

  it('a kind chip narrows the search, in the address and the API', async () => {
    serve();
    show('/analytics?tab=search&q=crew');
    const chip = await screen.findByRole('button', { name: /Tool call/ });
    fireEvent.click(chip);
    await waitFor(() => expect(asked.some((a) => a.get('kind') === '["tool_call"]')).toBe(true));
    expect(screen.getByTestId('where').textContent).toContain('kind=tool_call');
  });
});
