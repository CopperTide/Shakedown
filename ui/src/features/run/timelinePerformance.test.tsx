// @vitest-environment jsdom
import { cleanup, fireEvent, render } from '@testing-library/react';
import { useCallback } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { events, team } from '@/model/fixtures';
import { buildLanes, spanOf } from '@/model/lanes';
import ActionTimeline from './ActionTimeline';
import { useStepParam } from './useStepParam';
import { layoutTimeline, shapeOf } from './timelineLayout';
vi.mock('./timelineLayout', async (original) => {
  const actual = await original<typeof import('./timelineLayout')>();
  return { ...actual, layoutTimeline: vi.fn(actual.layoutTimeline), shapeOf: vi.fn(actual.shapeOf) };
});
class NoResize { observe() {} disconnect() {} unobserve() {} }
globalThis.ResizeObserver ??= NoResize as unknown as typeof ResizeObserver;
afterEach(cleanup);
it('step navigation reuses layout and renders only the two changed marks', () => {
  const lanes = buildLanes(events, team);
  const span = spanOf(lanes);
  const onModels = () => {};
  function Timeline() {
    const [selected, onSelect] = useStepParam();
    const clear = useCallback(() => onSelect(null), [onSelect]);
    return <ActionTimeline lanes={lanes} team={team} span={span} selected={selected} onSelect={onSelect} onClear={clear} models onModels={onModels} />;
  }
  const { container } = render(<MemoryRouter initialEntries={['/?step=6']}><Timeline /></MemoryRouter>);
  vi.mocked(layoutTimeline).mockClear();
  vi.mocked(shapeOf).mockClear();
  fireEvent.click(container.querySelector('[data-seq="9"]')!);
  expect(container.querySelector('[data-seq="9"]')?.getAttribute('aria-pressed')).toBe('true');
  expect(container.querySelector('[data-seq="6"]')?.getAttribute('aria-pressed')).toBe('false');
  expect(layoutTimeline).not.toHaveBeenCalled();
  expect(shapeOf).toHaveBeenCalledTimes(2);
  fireEvent.click(container.querySelector('[data-seq="6"]')!);
  expect(container.querySelector('[data-seq="6"]')?.getAttribute('aria-pressed')).toBe('true');
  expect(shapeOf).toHaveBeenCalledTimes(4);
  fireEvent.doubleClick(container.querySelector('[data-seq="6"]')!);
  expect(container.querySelector('[aria-pressed="true"]')).toBeNull();
});


it('renders every overlapping event at fit zoom and keeps them visible when selecting', () => {
  const mark = buildLanes(events, team)[0].marks[0];
  const lanes = [{ id: 'dense', name: 'Dense', marks: Array.from({ length: 100 }, (_, seq) => ({ ...mark, seq, at: seq })) }];
  const span = { start: 0, end: 10000 };
  const onSelect = vi.fn();
  const props = { lanes, team, span, onSelect, models: true, onModels: () => {} };
  const { container, rerender } = render(<ActionTimeline {...props} selected={null} />);
  expect(container.querySelectorAll('button[data-seq]')).toHaveLength(100);
  expect(container.querySelector('[data-event-count]')).toBeNull();
  rerender(<ActionTimeline {...props} selected={42} />);
  expect(container.querySelector('[data-seq="42"]')?.getAttribute('aria-pressed')).toBe('true');
  expect(container.querySelectorAll('button[data-seq]')).toHaveLength(100);
});
