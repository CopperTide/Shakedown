// @vitest-environment jsdom
// The world editor: connectors with key-store auth, flows one step per line, databases with green
// switches, and an honest Overview when no runtime is set.
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';
import WorldEditor from './WorldEditor';

const BODY = { format: 'shakedown-world/3', label: 'Larkspur Valve Works' };
let puts: { body: Record<string, unknown> }[] = [];

function mockFetch(body: Record<string, unknown> = BODY) {
  puts = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const u = String(url);
    if (init?.method === 'PUT') {
      const sent = JSON.parse(String(init.body));
      puts.push(sent);
      return new Response(JSON.stringify({ kind: 'world', id: 'plant', label: 'L', hash: 'h2', problems: [], body: sent.body }));
    }
    if (u.endsWith('/api/project/worlds/plant')) return new Response(JSON.stringify({ kind: 'world', id: 'plant', label: 'L', hash: 'h', problems: [], body }));
    if (u.endsWith('/api/worlds')) return new Response(JSON.stringify([{ name: 'toybench', title: 'Toy bench', built: false }]));
    return new Response(JSON.stringify({ error: 'not found' }), { status: 404 });
  }));
}

const show = () => render(
  <MemoryRouter initialEntries={['/worlds/plant']}><Routes><Route path="/worlds/:id" element={<WorldEditor />} /></Routes></MemoryRouter>,
);

const section = async (name: string) => fireEvent.click(within(await screen.findByRole('navigation', { name: 'Sections' })).getByRole('button', { name: new RegExp(`^${name}`) }));
const type = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } });

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('world editor', () => {
  it('world_without_package_overview', async () => {
    mockFetch();
    show();
    expect(await screen.findByText('No runtime yet')).toBeTruthy();
  });

  it('add_connector_then_save', async () => {
    mockFetch();
    show();
    await section('Connectors');
    type('Connector id', 'mes');
    type('Connector name', 'Larkspur MES');
    type('MCP server URL', 'https://mcp.larkspur.internal/mes');
    type('Key-store name', 'larkspur-mes');
    fireEvent.click(screen.getByRole('button', { name: 'Add connector' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Save' }));
    await waitFor(() => expect(puts.length).toBe(1));
    expect(puts[0].body.connectors).toEqual([{ id: 'mes', label: 'Larkspur MES', mode: 'real', mcp: 'https://mcp.larkspur.internal/mes', auth: { key: 'larkspur-mes' } }]);
  });

  it('flows_steps_one_per_line', async () => {
    mockFetch();
    show();
    await section('Software flows');
    type('Flow id', 'order-to-job');
    type('Trigger', 'crm.order.created');
    type('Steps', 'erp.create_sales_order\nmes.create_job\n');
    fireEvent.click(screen.getByRole('button', { name: 'Add flow' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Save' }));
    await waitFor(() => expect(puts.length).toBe(1));
    expect(puts[0].body.flows).toEqual([{ id: 'order-to-job', trigger: 'crm.order.created', steps: ['erp.create_sales_order', 'mes.create_job'] }]);
  });

  it('database_toggles_are_green_when_on', async () => {
    mockFetch({ ...BODY, data: [{ database: 'larkspur-prod', engine: 'postgres', version: '15.6' }] });
    show();
    await section('Data');
    const masked = await screen.findByRole('switch', { name: 'Masked' });
    expect(masked.hasAttribute('data-on')).toBe(false);
    fireEvent.click(masked);
    expect(screen.getByRole('switch', { name: 'Masked' }).hasAttribute('data-on')).toBe(true);
  });

  it('runtime_lists_packages', async () => {
    mockFetch();
    show();
    await section('Runtime');
    fireEvent.change(await screen.findByLabelText('Runtime package'), { target: { value: 'toybench' } });
    fireEvent.click(await screen.findByRole('button', { name: 'Save' }));
    await waitFor(() => expect(puts.length).toBe(1));
    expect(puts[0].body.package).toBe('toybench');
  });
});

describe('observability and on-call', () => {
  it('add_operator_never_answers', async () => {
    mockFetch();
    show();
    await section('Observability');
    type('Operator id', 'it');
    type('Operator name', 'IT on call');
    fireEvent.change(screen.getByLabelText('Behavior'), { target: { value: 'never_answers' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add operator' }));
    expect((await save()).observability).toEqual({ operators: [{ id: 'it', label: 'IT on call', behavior: 'never_answers' }] });
  });

  it('human_rule_choices', async () => {
    mockFetch();
    show();
    await section('Observability');
    fireEvent.change(await screen.findByLabelText('When an agent is stuck'), { target: { value: 'fail' } });
    expect((await save()).observability).toEqual({ human: [{ moment: 'agent_stuck', then: 'fail' }] });
  });
});

async function save() {
  fireEvent.click(await screen.findByRole('button', { name: 'Save' }));
  await waitFor(() => expect(puts.length).toBe(1));
  return puts[0].body;
}
