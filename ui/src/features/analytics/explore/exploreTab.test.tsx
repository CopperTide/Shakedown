// @vitest-environment jsdom
// The Explore tab: it opens on the default query (cost and runs by agent), a preset changes the
// query in the address and the API, a value drills through to its steps on the run page, and the
// SQL tab runs a query and shows its rows.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { DrillResult, ExploreSchema, QueryResult, SqlResult } from '@/api/explore';
import ExploreTab from './ExploreTab';

class NoResize { observe() {} unobserve() {} disconnect() {} }
globalThis.ResizeObserver ??= NoResize as unknown as typeof ResizeObserver;

const SCHEMA: ExploreSchema = {
  measures: [
    { id: 'cost', label: 'Cost', unit: 'usd', level: 'step', description: 'Model calls' },
    { id: 'runs', label: 'Runs', unit: 'count', level: 'run', description: 'Runs' },
    { id: 'refusals', label: 'Refusals', unit: 'count', level: 'step', description: 'Refused calls' },
    { id: 'tool_calls', label: 'Tool calls', unit: 'count', level: 'step', description: 'Tool calls' },
  ],
  dimensions: [{ id: 'agent', label: 'Agent', level: 'step', description: '' }, { id: 'tool', label: 'Tool', level: 'step', description: '' }],
  options: { experiments: [], setups: ['Solo'], agents: ['lead'] },
  sql: { schema: 'explore', views: [{ name: 'runs', description: 'One row per run', columns: [{ name: 'setup', type: 'text', description: '' }] }], functions: ['count'], limits: { rows: 5000, bytes: 8e6, timeout_seconds: 5, columns: 100 }, examples: [] },
};
const RESULT: QueryResult = {
  columns: [{ id: 'agent', label: 'Agent', role: 'dimension' }, { id: 'cost', label: 'Cost', role: 'measure', unit: 'usd' }, { id: 'runs', label: 'Runs', role: 'measure', unit: 'count' }],
  rows: [['lead', 1.25, 3], [null, 0.5, 2]], labels: {}, truncated: false, ms: 12,
};
const DRILL: DrillResult = { kind: 'steps', capped: false, steps: [{ run_id: 'r1', branch_id: 'b1', run_name: 'Hub day', seq: 42, at: '2026-11-25T10:00:00Z', agent: 'lead', kind: 'model_call', tool: null, outcome: null, model: 'm1' }] };
const SQL: SqlResult = { sql: 'SELECT 1', columns: [{ name: 'setup', type: 'text', kind: 'text' }, { name: 'runs', type: 'int8', kind: 'number' }], rows: [['Solo', 4]], truncated: null, ms: 3 };

const asked: { url: string; body: unknown }[] = [];
function serve() {
  asked.length = 0;
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    asked.push({ url: u, body: init?.body ? JSON.parse(String(init.body)) : null });
    const answer = u.endsWith('/schema') ? SCHEMA : u.endsWith('/query') ? RESULT : u.endsWith('/drill') ? DRILL : u.endsWith('/sql') ? SQL : { items: [] };
    return new Response(JSON.stringify(answer));
  }));
}

function Where() {
  const l = useLocation();
  return <output data-testid="where">{l.pathname + l.search}</output>;
}
const show = (at: string) => render(
  <MemoryRouter initialEntries={[at]}>
    <Routes><Route path="/analytics" element={<ExploreTab />} /><Route path="*" element={null} /></Routes>
    <Where />
  </MemoryRouter>,
);
const queries = () => asked.filter((a) => a.url.endsWith('/query')).map((a) => a.body as { measures: string[]; dimensions: string[] });

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('the Explore tab', () => {
  it('opens on cost and runs by agent, with a table and a chart', async () => {
    serve();
    show('/analytics?tab=explore');
    expect(await screen.findByText('$1.25')).toBeTruthy();
    expect(queries()[0]).toMatchObject({ measures: ['cost', 'runs'], dimensions: ['agent'] });
    expect(screen.getAllByText('(none)').length).toBeGreaterThan(0);
    expect(screen.getByRole('img', { name: /Cost/ })).toBeTruthy();
  });

  it('a preset changes the query, in the address and the API', async () => {
    serve();
    show('/analytics?tab=explore');
    fireEvent.click(await screen.findByRole('button', { name: 'Refusals by tool' }));
    await waitFor(() => expect(queries().some((q) => q.measures[0] === 'refusals' && q.dimensions[0] === 'tool')).toBe(true));
    expect(screen.getByTestId('where').textContent).toContain('m=refusals');
  });

  it('a value drills through to its steps, each linking to the run page at that step', async () => {
    serve();
    show('/analytics?tab=explore');
    fireEvent.click(await screen.findByText('$1.25'));
    const step = (await screen.findByText('step 42')).closest('a')!;
    expect(step.getAttribute('href')).toBe('/runs/r1/b1?step=42');
    const drill = asked.find((a) => a.url.endsWith('/drill'))!.body as { cell: unknown; measure: string };
    expect(drill).toMatchObject({ cell: { agent: 'lead' }, measure: 'cost' });
  });

  it('the SQL tab runs a query and shows its rows', async () => {
    serve();
    show('/analytics?tab=explore&mode=sql');
    const box = await screen.findByRole('textbox', { name: 'SQL query' });
    fireEvent.change(box, { target: { value: 'SELECT setup, count(*) AS runs FROM runs GROUP BY 1' } });
    fireEvent.click(screen.getByRole('button', { name: 'Run' }));
    expect(await screen.findByText('Solo')).toBeTruthy();
    expect(asked.find((a) => a.url.endsWith('/sql'))!.body).toEqual({ sql: 'SELECT setup, count(*) AS runs FROM runs GROUP BY 1' });
    expect(screen.getByTestId('where').textContent).toContain('mode=sql');
  });
});
