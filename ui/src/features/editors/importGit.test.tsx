// @vitest-environment jsdom
// Import from git on the Worlds page: Check shows what the repo holds and what happens to each
// piece without writing; Import confirms; a package name that is taken blocks the import.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { GitImport } from '@/api/project';
import ItemListPage from './ItemListPage';

const PLAN: GitImport = {
  proposed: true,
  id: 'airline-world',
  url: '/repos/airline-world',
  ref: 'main',
  commit: 'a813415851dded5582af9bd4f47ab74336f7810d',
  project: { name: 'Airline world', problems: [] },
  packages: [
    { name: 'airline-hub-day', kind: 'benchmark', dir: 'benchmark/hub-day', python: '.' },
    { name: 'airline-driver', kind: 'integration', dir: 'parts/driver', python: '.' },
  ],
  items: [
    { kind: 'world', id: 'hub-day', label: 'United at IAD, one hub day', action: 'write', reason: null },
    { kind: 'setup', id: 'occ-team', label: 'OCC team', action: 'keep', reason: 'the project has its own setup occ-team: kept' },
  ],
  views: [{ world: 'hub-day', file: 'views/occ.yaml' }],
  compose: [{ file: 'ops/compose.yaml', services: ['cache'], volumes: ['cache-data'] }],
  notes: [],
  problems: [],
  conflicts: [],
  previous_commit: null,
};

const posted: { url: string; ref?: string; confirm: boolean }[] = [];

function mockFetch(plan: GitImport = PLAN) {
  posted.length = 0;
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    if (u.endsWith('/api/project/worlds')) return new Response(JSON.stringify({ items: [] }));
    if (u.endsWith('/api/project/import-git')) {
      const body = JSON.parse(String(init?.body)) as { url: string; ref?: string; confirm: boolean };
      posted.push(body);
      const out = body.confirm ? { ...plan, proposed: undefined, registered: true } : plan;
      return new Response(JSON.stringify(out));
    }
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

async function check(url: string) {
  render(<MemoryRouter><ItemListPage kind="worlds" /></MemoryRouter>);
  fireEvent.click(await screen.findByRole('button', { name: 'Import from git' }));
  fireEvent.change(screen.getByLabelText('Repository'), { target: { value: url } });
  fireEvent.change(screen.getByLabelText('Ref'), { target: { value: 'main' } });
  fireEvent.click(screen.getByRole('button', { name: 'Check' }));
}

describe('import from git', () => {
  it('check_shows_the_plan_then_import_confirms', async () => {
    mockFetch();
    await check('/repos/airline-world');
    expect(await screen.findByText('airline-driver')).toBeTruthy();
    expect(screen.getByText('Packages (2)')).toBeTruthy();
    expect(screen.getByTitle('the project has its own setup occ-team: kept').textContent).toContain('kept');
    expect(screen.getByText('views/occ.yaml')).toBeTruthy();
    expect(screen.getByText('ops/compose.yaml')).toBeTruthy();
    expect(posted).toEqual([{ url: '/repos/airline-world', ref: 'main', confirm: false }]);
    fireEvent.click(screen.getByRole('button', { name: 'Import' }));
    expect((await screen.findByRole('status')).textContent).toContain('2 packages registered');
    expect(posted[1]).toEqual({ url: '/repos/airline-world', ref: 'main', confirm: true });
  });

  it('says_what_a_reimport_removes_because_the_repo_no_longer_has_it', async () => {
    const gone = { kind: 'experiment', id: 'old-try', label: 'Old try', action: 'remove' as const, reason: null };
    mockFetch({ ...PLAN, items: [...PLAN.items, gone], removed: [gone] });
    await check('/repos/airline-world');
    expect((await screen.findByText(/Old try/)).textContent).toContain('removed (to the trash)');
    fireEvent.click(screen.getByRole('button', { name: 'Import' }));
    expect((await screen.findByRole('status')).textContent).toContain('1 removed');
  });

  it('a_taken_package_name_blocks_the_import', async () => {
    mockFetch({ ...PLAN, conflicts: ['a package named airline-driver is installed already, from /x'] });
    await check('/repos/airline-world');
    expect((await screen.findByRole('alert')).textContent).toContain('installed already');
    expect((screen.getByRole('button', { name: 'Import' }) as HTMLButtonElement).disabled).toBe(true);
  });
});
