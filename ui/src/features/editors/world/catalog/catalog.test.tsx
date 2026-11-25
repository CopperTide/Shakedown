// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import ConnectorCatalog from './ConnectorCatalog';
import ConnectorsSection from '../ConnectorsSection';
import { EMPTY_FILTER, findConnectors } from './search';

afterEach(cleanup);
it('combines industry, company, interface and text search', () => {
  expect(findConnectors({ ...EMPTY_FILTER, industry: 'Manufacturing', vendor: 'Oracle', protocol: 'REST API', query: 'procurement' }).map((e) => e.id)).toEqual(['oracle-fusion']);
  expect(findConnectors({ ...EMPTY_FILTER, industry: 'Healthcare', vendor: 'Oracle' })).toEqual([]);
  expect(findConnectors({ ...EMPTY_FILTER, protocol: 'MCP' }).map((e) => e.id)).toContain('github');
});
it('inspects an Oracle template and adds a configuration without claiming connectivity', () => {
  const add = vi.fn();
  render(<ConnectorCatalog open onClose={() => {}} onAdd={add} />);
  fireEvent.change(screen.getByLabelText('Search connector catalog'), { target: { value: 'Oracle Fusion' } });
  expect(screen.getByRole('status').textContent).toBe('1 of 26 connectors');
  fireEvent.click(screen.getByRole('button', { name: /Oracle Fusion Cloud/ }));
  expect(screen.getByText('Planned adapter')).toBeTruthy();
  expect(screen.getByRole('link', { name: /Vendor API documentation/ }).getAttribute('href')).toContain('oracle.com');
  fireEvent.click(screen.getByRole('button', { name: 'Use configuration template' }));
  expect((screen.getByLabelText('Product') as HTMLInputElement).value).toBe('oracle-fusion');
  fireEvent.click(screen.getByRole('button', { name: 'Add connector' }));
  expect(add).toHaveBeenCalledWith({ id: 'oracle-fusion', label: 'Oracle Fusion Cloud', product: 'oracle-fusion', mode: 'real' });
});
it('shows saved MCP, OpenAPI and credential references in connector details', () => {
  render(<ConnectorsSection world={{ format: 'shakedown-world/3', label: 'Test', connectors: [{ id: 'custom', label: 'Custom system', mode: 'replay', mcp: 'https://example.test/mcp', openapi: 'api.yaml', auth: { key: 'key-name' } }] }} onChange={() => {}} />);
  fireEvent.click(screen.getByRole('button', { name: 'Inspect Custom system' }));
  const dialog = within(screen.getByRole('dialog'));
  expect(dialog.getByText('https://example.test/mcp')).toBeTruthy();
  expect(dialog.getByText('api.yaml')).toBeTruthy();
  expect(dialog.getByText('Key store: key-name')).toBeTruthy();
});
it('keeps the form open with an actionable duplicate-id error', () => {
  render(<ConnectorCatalog open onClose={() => {}} onAdd={() => 'Choose a unique id'} />);
  fireEvent.change(screen.getByLabelText('Search connector catalog'), { target: { value: 'Oracle Fusion' } });
  fireEvent.click(screen.getByRole('button', { name: /Oracle Fusion Cloud/ }));
  fireEvent.click(screen.getByRole('button', { name: 'Use configuration template' }));
  fireEvent.click(screen.getByRole('button', { name: 'Add connector' }));
  expect(screen.getByRole('alert').textContent).toBe('Choose a unique id');
  expect((screen.getByLabelText('Connector id') as HTMLInputElement).value).toBe('oracle-fusion');
});
