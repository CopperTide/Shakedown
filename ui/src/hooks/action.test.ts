import { describe, expect, it, vi } from 'vitest';
import { runAction } from './action';

describe('runAction', () => {
  it('is busy while the request runs, then done, and gives back its answer', async () => {
    const set = vi.fn();
    expect(await runAction(async () => 'ok', set)).toBe('ok');
    expect(set.mock.calls).toEqual([[{ busy: true, error: null }], [{ busy: false, error: null }]]);
  });
  it('stays busy after success when the page moves on', async () => {
    const set = vi.fn();
    await runAction(async () => 1, set, { stayBusy: true });
    expect(set.mock.calls).toEqual([[{ busy: true, error: null }]]);
  });
  it('a failure says why in words and gives back nothing', async () => {
    const set = vi.fn();
    expect(await runAction(async () => { throw new Error('the disk is full'); }, set, { stayBusy: true })).toBeUndefined();
    expect(set.mock.calls).toEqual([[{ busy: true, error: null }], [{ busy: false, error: 'the disk is full' }]]);
  });
});
