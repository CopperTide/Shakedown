// @vitest-environment jsdom
// Settings' tabs (Advanced also covers the pages it lists) and the Advanced page's links.
import { cleanup, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it } from 'vitest';
import AdvancedPage from './AdvancedPage';
import { SettingsTabs } from './SettingsPage';

afterEach(cleanup);

const current = () => screen.getAllByRole('link').filter((a) => a.getAttribute('aria-current') === 'page').map((a) => a.textContent);

describe('Settings', () => {
  it.each([
    ['/settings/models', 'Connections & models'], ['/settings/project', 'Project settings'],
    ['/settings/advanced', 'Advanced'], ['/settings/observability', 'Advanced'], ['/settings/tokens', 'Advanced'],
  ])('on %s marks %s', (at, tab) => {
    render(<MemoryRouter initialEntries={[at]}><SettingsTabs /></MemoryRouter>);
    expect(current()).toEqual([tab]);
  });

  it('Advanced links runtimes, observability, API tokens and connecting an agent', () => {
    render(<MemoryRouter initialEntries={['/settings/advanced']}><AdvancedPage /></MemoryRouter>);
    const href = (name: RegExp) => screen.getByRole('link', { name }).getAttribute('href');
    expect(href(/^Runtimes/)).toBe('/runtimes');
    expect(href(/^Observability/)).toBe('/settings/observability');
    expect(href(/^API tokens/)).toBe('/settings/tokens');
    expect(href(/^Connect an agent/)).toBe('/connect');
  });
});
