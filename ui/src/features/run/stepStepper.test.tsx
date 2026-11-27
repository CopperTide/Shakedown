// @vitest-environment jsdom
// Previous and next step: buttons beside the step, and ←/→ unless a field or a tab has the keys.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, describe, expect, it } from 'vitest';
import StepStepper, { placeLine } from './StepStepper';
import { useStepKeys } from './useStepKeys';

afterEach(cleanup);

const ORDER = [3, 5, 9];

function Steps({ start = null }: { start?: number | null }) {
  const [selected, setSelected] = useState<number | null>(start);
  useStepKeys(ORDER, setSelected);
  return (
    <>
      <StepStepper order={ORDER} selected={selected} onSelect={setSelected} />
      <output>{selected ?? 'none'}</output>
      <input aria-label="Search" />
      <div role="tablist"><button type="button" role="tab">Event log</button></div>
    </>
  );
}

const shown = () => screen.getByRole('status').textContent;

describe('step stepper', () => {
  it('says_where_the_selection_is', () => {
    expect(placeLine(ORDER, 5)).toBe('2 of 3');
    expect(placeLine(ORDER, null)).toBe('3 steps');
    expect(placeLine(ORDER, 4)).toBe('3 steps');
  });

  it('buttons_move_and_stop_at_the_ends', () => {
    render(<Steps start={5} />);
    fireEvent.click(screen.getByRole('button', { name: 'Next step' }));
    expect(shown()).toBe('9');
    expect((screen.getByRole('button', { name: 'Next step' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Previous step' }));
    fireEvent.click(screen.getByRole('button', { name: 'Previous step' }));
    expect(shown()).toBe('3');
    expect((screen.getByRole('button', { name: 'Previous step' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('arrow_keys_move_unless_typing_or_on_a_tab', () => {
    render(<Steps start={3} />);
    fireEvent.keyDown(window, { key: 'ArrowRight' });
    expect(shown()).toBe('5');
    fireEvent.keyDown(screen.getByLabelText('Search'), { key: 'ArrowRight' });
    fireEvent.keyDown(screen.getByRole('tab'), { key: 'ArrowRight' });
    expect(shown()).toBe('5');
    fireEvent.keyDown(window, { key: 'ArrowLeft' });
    expect(shown()).toBe('3');
  });
});
