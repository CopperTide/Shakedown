// @vitest-environment jsdom
// The step panel: a step's prose as Markdown and data as JSON, and full width and back.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Split } from '@/components/ui/split';
import type { Event } from '@/model/types';
import Inspector, { jsonIn } from './Inspector';
import { StepSidePanel, useExpanded } from './RunShell';

vi.mock('@/api/branches', () => ({ branchEvent: vi.fn(), rows: vi.fn() }));
afterEach(cleanup);

const step = (kind: Event['kind'], payload: unknown): Event => ({ branch_id: 'b', seq: 7, at: '2026-11-25T12:00:00Z', agent: 'lead', kind, payload } as Event);

describe('step text', () => {
  it('renders_a_message_as_markdown', () => {
    const { container } = render(<Inspector branchId="b" event={step('message', { to: 'ops', text: 'Ship **SO-1001** first.\n\n1. Call the carrier' })} />);
    expect(container.querySelector('strong')?.textContent).toBe('SO-1001');
    expect(container.querySelector('ol li')?.textContent).toBe('Call the carrier');
  });

  it('keeps_a_json_text_as_json', () => {
    expect(jsonIn(' {"a": 1} ')).toEqual({ a: 1 });
    expect(jsonIn('[not json')).toBeUndefined();
    expect(jsonIn('Plain words')).toBeUndefined();
    render(<Inspector branchId="b" event={step('report', { to: 'lead', text: '{"done": true}' })} />);
    expect(screen.getByText(/"done": true/)).toBeTruthy();
  });
});

function Page() {
  const [expanded, setExpanded] = useExpanded();
  return <Split storageKey="test.side" expanded={expanded} main={<p>the timeline</p>} side={<StepSidePanel expanded={expanded} onExpand={setExpanded}>the step</StepSidePanel>} />;
}

describe('full width', () => {
  it('expands_and_comes_back', () => {
    render(<Page />);
    expect(screen.getByText('the timeline').parentElement?.className).not.toContain('hidden');
    fireEvent.click(screen.getByRole('button', { name: 'Full width' }));
    expect(screen.getByText('the timeline').parentElement?.className).toContain('hidden');
    fireEvent.click(screen.getByRole('button', { name: 'Side panel' }));
    expect(screen.getByText('the timeline').parentElement?.className).not.toContain('hidden');
  });

  it('escape_gives_the_width_back', () => {
    render(<Page />);
    fireEvent.click(screen.getByRole('button', { name: 'Full width' }));
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.getByRole('button', { name: 'Full width' })).toBeTruthy();
  });
});
