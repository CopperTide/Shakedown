// @vitest-environment jsdom
// The key promises say what the code does: sealed in the database, never shown again, sent only to an HTTP setup's endpoint when it names the key.
import { cleanup, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, describe, expect, it } from 'vitest';
import StepModel from './StepModel';

afterEach(cleanup);

describe('model step', () => {
  it('states_where_keys_live_without_a_keychain_or_machine_claim', () => {
    render(<MemoryRouter><StepModel ready={false} /></MemoryRouter>);
    expect(screen.getByText(/sealed with the vault key and stored in Shakedown's database/)).toBeTruthy();
    expect(screen.getByText(/never get a stored key.*sent to that endpoint on each request/)).toBeTruthy();
    expect(screen.queryByText(/agents never receive/)).toBeNull();
    expect(screen.queryByText(/keychain|never sent to CopperTide|stay on this machine/i)).toBeNull();
  });
});
