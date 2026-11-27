// @vitest-environment jsdom
// The Runtimes page: installs with their source, packages, check and the worlds they run; the
// install dialog's plan-then-install from a git folder or an archive; a refused removal said.
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { RuntimePlan, Runtimes } from '@/api/runtimes';
import RuntimesPage from './RuntimesPage';

const CHECK = { package: 'tau-airline-desk', passed: true, steps: [{ name: 'describe', ok: true, detail: 'tau-airline-desk 0.1.0' }], skipped: [] };

const LISTED: Runtimes = {
  hosted: false,
  installs: [{
    id: 'tau-world', source: 'git', url: 'git@github.com:CopperTide/tau-world.git', ref: 'HEAD', path: '', commit: '1572b020759645ca', installed_at: '2026-11-27T20:00:00Z',
    runtime: true, items: 0,
    packages: [
      { name: 'tau-airline', kind: 'benchmark', version: '0.1.0', used_by: ['airline'], registered: true, check: { ...CHECK, package: 'tau-airline' } },
      { name: 'tau-airline-desk', kind: 'integration', version: '0.1.0', used_by: [], registered: true, check: { ...CHECK, passed: false, steps: [{ name: 'start', ok: false, detail: 'no domain' }] } },
    ],
  }],
  builtin: [{ name: 'postgres', kind: 'integration', version: '0.1.0', used_by: [] }],
};

const PLAN: RuntimePlan = {
  proposed: true, id: 'tau-world', source: 'archive', packages_only: '', url: 'tau-world.tar.gz', ref: '', commit: 'ab'.repeat(32),
  project: null, packages: [{ name: 'tau-airline', kind: 'benchmark', dir: 'benchmark/airline', python: '.' }],
  items: [], views: [], compose: [], notes: [], problems: [], conflicts: [], previous_commit: null,
};

let posted: Record<string, unknown>[] = [];

function mockFetch(listed: Runtimes = LISTED) {
  posted = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    if (u.endsWith('/api/runtimes') && init?.method === 'POST') {
      const body = JSON.parse(String(init.body));
      posted.push(body);
      return new Response(JSON.stringify(body.confirm ? { ...PLAN, proposed: undefined, checks: [CHECK] } : PLAN));
    }
    if (u.endsWith('/api/runtimes/tau-world') && init?.method === 'DELETE') {
      return new Response(JSON.stringify({ error: 'tau-world runs the worlds airline: give them another runtime first' }), { status: 409 });
    }
    if (u.endsWith('/api/runtimes')) return new Response(JSON.stringify(listed));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

const show = () => render(<MemoryRouter><RuntimesPage /></MemoryRouter>);

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('runtimes page', () => {
  it('lists each install with its source, packages, checks and worlds, and warns that it is code', async () => {
    mockFetch();
    show();
    expect(await screen.findByText(/git@github.com:CopperTide\/tau-world.git · HEAD at 1572b0207596/)).toBeTruthy();
    expect(screen.getAllByText(/A runtime is code/).length).toBeGreaterThan(0);
    const table = screen.getAllByRole('table')[0];
    expect(within(table).getByRole('link', { name: 'airline' }).getAttribute('href')).toBe('/worlds/airline');
    expect(within(table).getByText('Check passed')).toBeTruthy();
    expect(within(table).getByText(/start: no domain/)).toBeTruthy();
    expect(screen.getByText('postgres')).toBeTruthy();
  });

  it('says why a runtime a world uses is not removed', async () => {
    mockFetch();
    show();
    fireEvent.click(await screen.findByRole('button', { name: 'Remove' }));
    expect((await screen.findByRole('alert')).textContent).toContain('runs the worlds airline');
  });

  it('installs from a git folder: Check shows the plan, Install confirms and shows the check', async () => {
    mockFetch();
    show();
    fireEvent.click((await screen.findAllByRole('button', { name: /Install a runtime/ }))[0]);
    fireEvent.change(screen.getByLabelText('Repository'), { target: { value: 'git@github.com:CopperTide/tau-world.git' } });
    fireEvent.change(screen.getByLabelText('Folder'), { target: { value: 'benchmark/airline' } });
    fireEvent.click(screen.getByRole('button', { name: 'Check' }));
    expect(await screen.findByText(/Python dependencies installed with uv/)).toBeTruthy();
    expect(posted[0]).toMatchObject({ url: 'git@github.com:CopperTide/tau-world.git', path: 'benchmark/airline', confirm: false });
    fireEvent.click(screen.getByRole('button', { name: 'Install' }));
    expect(await screen.findByText(/Installed: 1 packages registered/)).toBeTruthy();
    expect(posted[1]).toMatchObject({ confirm: true, id: 'tau-world' });
    expect(screen.getAllByText('Check passed').length).toBeGreaterThan(0);
  });

  it('uploads an archive as base64', async () => {
    mockFetch();
    show();
    fireEvent.click((await screen.findAllByRole('button', { name: /Install a runtime/ }))[0]);
    fireEvent.click(screen.getByRole('radio', { name: 'Archive' }));
    const file = new File(['packed'], 'tau-world.tar.gz', { type: 'application/gzip' });
    fireEvent.change(screen.getByLabelText('Archive'), { target: { files: [file] } });
    fireEvent.click(screen.getByRole('button', { name: 'Check' }));
    await waitFor(() => expect(posted.length).toBe(1));
    expect(posted[0]).toMatchObject({ archive: { name: 'tau-world.tar.gz', data: btoa('packed') }, confirm: false });
  });

  it('offers no install where hosted refuses it', async () => {
    mockFetch({ ...LISTED, hosted: true, installs: [] });
    show();
    expect(await screen.findByText(/Hosted Shakedown doesn't install runtimes yet/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Install a runtime/ })).toBeNull();
  });
});
