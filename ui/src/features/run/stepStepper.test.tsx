// @vitest-environment jsdom
// The selected agent's previous and next step: buttons beside the step, and ←/→ unless a field or a tab has
// the keys; ↑/↓ move through every agent's steps.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, describe, expect, it } from 'vitest';
import StepStepper, { placeLine } from './StepStepper';
import { useStepKeys } from './useStepKeys';
import type { LaneRow, TimelineMark } from '@/model/lanes';

afterEach(cleanup);

const ORDER = [3, 4, 5, 9];
const mark = (seq: number) => ({ seq }) as TimelineMark;
/** The planner took steps 3, 5 and 9; the buyer step 4. */
const LANES: LaneRow[] = [
  { id: 'planner', name: 'Planner', marks: [3, 5, 9].map(mark) },
  { id: 'buyer', name: 'Buyer', marks: [mark(4)] },
] as LaneRow[];

function Steps({ start = null }: { start?: number | null }) {
  const [selected, setSelected] = useState<number | null>(start);
  useStepKeys(ORDER, LANES, setSelected);
  return (
    <>
      <StepStepper order={ORDER} lanes={LANES} selected={selected} onSelect={setSelected} />
      <output>{selected ?? 'none'}</output>
      <input aria-label="Search" />
      <div role="tablist"><button type="button" role="tab">Event log</button></div>
    </>
  );
}

const shown = () => screen.getByRole('status').textContent;

describe('step stepper', () => {
  it('says_where_the_selection_is', () => {
    expect(placeLine(ORDER, 5)).toBe('3 of 4');
    expect(placeLine(ORDER, null)).toBe('4 steps');
    expect(placeLine(ORDER, 6)).toBe('4 steps');
  });

  it('buttons_move_along_the_agents_steps_and_stop_at_its_ends', () => {
    render(<Steps start={5} />);
    expect(screen.getByText('Planner · 2 of 3')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Next step' }));
    expect(shown()).toBe('9');
    expect((screen.getByRole('button', { name: 'Next step' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Previous step' }));
    fireEvent.click(screen.getByRole('button', { name: 'Previous step' }));
    expect(shown()).toBe('3');
    expect((screen.getByRole('button', { name: 'Previous step' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('side_arrows_stay_on_the_agent_and_up_down_take_every_step', () => {
    render(<Steps start={3} />);
    fireEvent.keyDown(window, { key: 'ArrowRight' });
    expect(shown()).toBe('5');
    fireEvent.keyDown(window, { key: 'ArrowUp' });
    expect(shown()).toBe('4');
    fireEvent.keyDown(window, { key: 'ArrowRight' });
    expect(shown()).toBe('4');
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    expect(shown()).toBe('5');
    fireEvent.keyDown(screen.getByLabelText('Search'), { key: 'ArrowRight' });
    fireEvent.keyDown(screen.getByRole('tab'), { key: 'ArrowRight' });
    expect(shown()).toBe('5');
    fireEvent.keyDown(window, { key: 'ArrowLeft' });
    expect(shown()).toBe('3');
  });
});
