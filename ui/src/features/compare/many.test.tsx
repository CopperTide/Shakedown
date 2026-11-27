// @vitest-environment jsdom
// Comparing three or more runs: the address, the letters, each run against the first, the best of
// each row, and the page composed from the pairwise compare.
import { cleanup, render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { V1, V2, comparison, v1Detail, v2Detail } from '@/features/world/fixtures';
import type { BranchDetail } from '@/model/types';
import CompareManyPage from './CompareManyPage';
import { againstFirst, againstLine, branchesIn, compareHref, letterOf, ownSide, sidesOf } from './many';
import { bestOf } from './outcomes';

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

const fig = (value: number | null) => ({ text: String(value), value, abnormal: false });

describe('many runs', () => {
  it('names_the_runs_in_the_address', () => {
    expect(branchesIn(' a,b ,,a,c')).toEqual(['a', 'b', 'c']);
    expect(branchesIn(null)).toEqual([]);
    expect(compareHref(['a', 'b'])).toBe('/compare/a/b');
    expect(compareHref(['a', 'b', 'c'])).toBe('/compare?branches=a,b,c');
    expect([0, 2, 25, 26, 27].map(letterOf)).toEqual(['A', 'C', 'Z', 'AA', 'AB']);
  });

  it('says_how_each_differs_from_the_first', () => {
    expect(againstLine(againstFirst({ ...comparison, first_divergent_seq: 12, differing_rows: [{ key: 'k', a: 1, b: 2 }] }))).toBe('first diverges at step 12 · 1 differing row at the end');
    expect(againstLine(againstFirst({ ...comparison, first_divergent_seq: null, differing_rows: [], not_compared: ['plant'] }))).toBe('never diverges · 0 differing rows at the end · plant not compared');
    expect(againstLine(againstFirst({ error: 'not ended' }))).toBe('Not compared: not ended');
  });

  it('takes_each_runs_figures_from_its_compare_else_its_record', () => {
    const third = { ...v2Detail, id: 'c' } as BranchDetail;
    const sides = sidesOf([v1Detail, v2Detail, third], [comparison, { error: 'no save' }]);
    expect(sides[0]).toBe(comparison.a);
    expect(sides[1]).toBe(comparison.b);
    expect(sides[2]).toEqual(ownSide(third));
  });

  it('bolds_one_best_and_none_on_a_tie', () => {
    expect(bestOf({ better: 'lower', values: [fig(3), fig(1), fig(2)] })).toBe(1);
    expect(bestOf({ better: 'higher', values: [fig(3), fig(null), fig(3)] })).toBeNull();
    expect(bestOf({ better: null, values: [fig(1), fig(2), fig(3)] })).toBeNull();
    expect(bestOf({ better: 'higher', values: [fig(1), fig(null), fig(null)] })).toBeNull();
  });
});

function Where() { return <output>{useLocation().pathname}</output>; }

function mockApi() {
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    const u = new URL(String(url), 'http://x');
    if (u.pathname === '/api/compare') return new Response(JSON.stringify(comparison));
    const id = /^\/api\/branches\/([^/]+)$/.exec(u.pathname)?.[1];
    if (id) return new Response(JSON.stringify(id === V1 ? v1Detail : { ...v2Detail, id }));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

const show = (q: string) => render(
  <MemoryRouter initialEntries={[`/compare${q}`]}><Routes><Route path="/compare" element={<CompareManyPage />} /><Route path="*" element={<Where />} /></Routes></MemoryRouter>,
);

describe('compare many page', () => {
  it('shows_three_runs_side_by_side', async () => {
    mockApi();
    show(`?branches=${V1},${V2},third`);
    expect(await screen.findByText('Checks passed')).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Compare 3 runs' })).toBeTruthy();
    expect(screen.getAllByRole('columnheader').map((h) => h.textContent?.slice(0, 1))).toEqual(['', 'A', 'B', 'C']);
    expect(screen.getByRole('link', { name: 'A against C in detail' }).getAttribute('href')).toBe(`/compare/${V1}/third`);
  });

  it('two_runs_go_to_the_pairwise_page', () => {
    show('?branches=a,b');
    expect(screen.getByRole('status').textContent).toBe('/compare/a/b');
  });
});
