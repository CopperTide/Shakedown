// @vitest-environment jsdom
// The key promises say what the code does: sealed in the database, never shown again, sent only to an HTTP setup's endpoint when it names the key.
// On the page they show on hovering "Your keys stay yours", not inline.
import { cleanup, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it } from 'vitest';
import StepModel, { PROMISES } from './StepModel';

afterEach(cleanup);

describe('model step', () => {
  it('states_where_keys_live_without_a_keychain_or_machine_claim', () => {
    const all = PROMISES.join(' ');
    expect(all).toMatch(/sealed with the vault key and stored in Shakedown's database/);
    expect(all).toMatch(/never get a stored key.*sent to that endpoint on each request/);
    expect(all).not.toMatch(/agents never receive/);
    expect(all).not.toMatch(/keychain|never sent to CopperTide|stay on this machine/i);
  });

  it('shows_the_promise_title_with_the_promises_on_hover_only', () => {
    render(<MemoryRouter><StepModel ready={false} /></MemoryRouter>);
    expect(screen.getByText('Your keys stay yours')).toBeTruthy();
    expect(screen.getByText('Add a key')).toBeTruthy();
    expect(screen.queryByText(/sealed with the vault key/)).toBeNull();
    expect(screen.queryByText(/Every run calls a real model/)).toBeNull();
  });
});
