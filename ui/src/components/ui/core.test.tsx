// @vitest-environment jsdom
// The v7 core components: a switch that is green when on, a save bar that shows only after a
// change, a settings sub-nav that marks one section, and one primary button style.
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Button } from './button';
import { SaveBar } from './save-bar';
import { SubNav } from './subnav';
import { Toggle } from './toggle';

describe('core components', () => {
  it('toggle_is_green_when_on', () => {
    const onChange = vi.fn();
    const { rerender } = render(<Toggle checked={false} onChange={onChange} label="Track with git" />);
    const sw = screen.getByRole('switch', { name: 'Track with git' });
    expect(sw.getAttribute('aria-checked')).toBe('false');
    expect(sw.getAttribute('data-state')).toBe('unchecked');
    fireEvent.click(sw);
    expect(onChange).toHaveBeenCalledWith(true);
    rerender(<Toggle checked onChange={onChange} label="Track with git" />);
    expect(screen.getByRole('switch').getAttribute('data-state')).toBe('checked');
    expect(screen.getByRole('switch').className).toContain('bg-ok');
  });

  it('save_bar_hidden_until_dirty', () => {
    const props = { summary: '1 change', onSave: vi.fn(), onDiscard: vi.fn() };
    const { rerender } = render(<SaveBar dirty={false} {...props} />);
    expect(screen.queryByText('1 change')).toBeNull();
    rerender(<SaveBar dirty {...props} />);
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(props.onSave).toHaveBeenCalled();
  });

  it('subnav_marks_active', () => {
    const items = [{ id: 'overview', label: 'Overview' }, { id: 'data', label: 'Data', status: 'warn' as const }];
    const onSelect = vi.fn();
    render(<SubNav items={items} active="data" onSelect={onSelect} />);
    expect(screen.getByRole('button', { name: /Data/ }).getAttribute('aria-current')).toBe('page');
    expect(screen.getByRole('button', { name: 'Overview' }).getAttribute('aria-current')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Overview' }));
    expect(onSelect).toHaveBeenCalledWith('overview');
  });

  it('button_primary_uses_ink', () => {
    render(<Button variant="primary">Deploy</Button>);
    expect(screen.getByRole('button', { name: 'Deploy' }).className).toContain('bg-primary');
  });
});
