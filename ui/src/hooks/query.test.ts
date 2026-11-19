import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Query, forgetShared, sharedQuery } from './query';

/** A load whose answers the test gives, one call at a time. */
function loads<T>() {
  const calls: { resolve: (v: T) => void; reject: (e: unknown) => void }[] = [];
  const load = vi.fn(() => new Promise<T>((resolve, reject) => calls.push({ resolve, reject })));
  return { load, calls };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => forgetShared());

describe('Query', () => {
  it('loads when the first reader subscribes and tells it', async () => {
    const { load, calls } = loads<number>();
    const q = new Query(load);
    const seen = vi.fn();
    q.subscribe(seen);
    expect(load).toHaveBeenCalledTimes(1);
    calls[0].resolve(7);
    await flush();
    expect(q.get()).toMatchObject({ data: 7, error: null, loading: false });
    expect(seen).toHaveBeenCalled();
  });

  it('keeps the last value through a failed reload when asked to, with the error beside it', async () => {
    const { load, calls } = loads<number>();
    const q = new Query(load, () => ({ keepOnError: true }));
    q.subscribe(() => {});
    calls[0].resolve(1);
    await flush();
    q.fetch();
    calls[1].reject(new Error('arenad is down'));
    await flush();
    expect(q.get()).toMatchObject({ data: 1, error: 'arenad is down' });
    q.fetch();
    calls[2].resolve(2);
    await flush();
    expect(q.get()).toMatchObject({ data: 2, error: null });
  });

  it('drops the value on a failed load unless asked to keep it', async () => {
    const { load, calls } = loads<number>();
    const q = new Query(load);
    q.subscribe(() => {});
    calls[0].resolve(1);
    await flush();
    q.fetch();
    calls[1].reject('nope');
    await flush();
    expect(q.get()).toMatchObject({ data: null, error: 'nope' });
  });

  it('a stale response never overwrites a newer one', async () => {
    const { load, calls } = loads<string>();
    const q = new Query(load);
    q.subscribe(() => {});
    q.fetch();
    calls[1].resolve('new');
    await flush();
    calls[0].resolve('old');
    await flush();
    expect(q.get().data).toBe('new');
  });

  it('ignores what lands after its last reader left', async () => {
    const { load, calls } = loads<string>();
    const q = new Query(load);
    const stop = q.subscribe(() => {});
    stop();
    calls[0].resolve('late');
    await flush();
    expect(q.get().data).toBeNull();
  });

  it('starts from a seed (the previous key’s value) until its own answer lands', async () => {
    const { load, calls } = loads<string>();
    const q = new Query(load, () => ({}), 'previous');
    q.subscribe(() => {});
    expect(q.get().data).toBe('previous');
    calls[0].resolve('own');
    await flush();
    expect(q.get().data).toBe('own');
  });
});

describe('changing the answer on screen', () => {
  it('update changes the value in place (a streamed event), and the next answer replaces it', async () => {
    const { load, calls } = loads<number[]>();
    const q = new Query(load);
    q.subscribe(() => {});
    calls[0].resolve([1]);
    await flush();
    q.update((d) => (d ? [...d, 2] : d));
    expect(q.get().data).toEqual([1, 2]);
    q.fetch();
    calls[1].resolve([1, 2, 3]);
    await flush();
    expect(q.get().data).toEqual([1, 2, 3]);
  });
  it('a retry from scratch clears the value and the error while it asks', async () => {
    const { load, calls } = loads<number>();
    const q = new Query(load);
    q.subscribe(() => {});
    calls[0].reject(new Error('down'));
    await flush();
    q.fetch({ clear: true });
    expect(q.get()).toMatchObject({ data: null, error: null, loading: true });
  });
});

describe('polling', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('reads again every `ms` while the value says so, and stops when it no longer does', async () => {
    const values = ['running', 'running', 'finished'];
    const load = vi.fn(async () => values.shift()!);
    const q = new Query(load, () => ({ poll: { ms: 2000, while: (v: string) => v === 'running' } }));
    q.subscribe(() => {});
    await vi.advanceTimersByTimeAsync(0);
    expect(load).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(2000);
    expect(load).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(2000);
    expect(load).toHaveBeenCalledTimes(3);
    expect(q.get().data).toBe('finished');
    await vi.advanceTimersByTimeAsync(10_000);
    expect(load).toHaveBeenCalledTimes(3);
  });

  it('a failed poll keeps the last value and waits for a reload', async () => {
    let fail = false;
    const load = vi.fn(async () => { if (fail) throw new Error('down'); return 'running'; });
    const q = new Query(load, () => ({ keepOnError: true, poll: { ms: 1000, while: () => true } }));
    q.subscribe(() => {});
    await vi.advanceTimersByTimeAsync(0);
    fail = true;
    await vi.advanceTimersByTimeAsync(1000);
    expect(q.get()).toMatchObject({ data: 'running', error: 'down' });
    await vi.advanceTimersByTimeAsync(5000);
    expect(load).toHaveBeenCalledTimes(2);
    fail = false;
    q.fetch();
    await vi.advanceTimersByTimeAsync(0);
    expect(q.get()).toMatchObject({ data: 'running', error: null });
    expect(load).toHaveBeenCalledTimes(3);
  });

  it('stops polling when its last reader leaves', async () => {
    const load = vi.fn(async () => 'running');
    const q = new Query(load, () => ({ poll: { ms: 1000, while: () => true } }));
    const stop = q.subscribe(() => {});
    await vi.advanceTimersByTimeAsync(0);
    stop();
    await vi.advanceTimersByTimeAsync(5000);
    expect(load).toHaveBeenCalledTimes(1);
  });
});

describe('shared queries', () => {
  it('readers of one key share one load and its answer', async () => {
    const { load, calls } = loads<string[]>();
    const a = sharedQuery('tasks', load);
    const b = sharedQuery('tasks', load);
    expect(a).toBe(b);
    a.subscribe(() => {});
    b.subscribe(() => {});
    expect(load).toHaveBeenCalledTimes(1);
    calls[0].resolve(['t1']);
    await flush();
    expect(b.get().data).toEqual(['t1']);
  });

  it('a later reader gets the answer without asking again', async () => {
    const { load, calls } = loads<string>();
    const stop = sharedQuery('health', load).subscribe(() => {});
    calls[0].resolve('ok');
    await flush();
    stop();
    sharedQuery('health', load).subscribe(() => {});
    expect(load).toHaveBeenCalledTimes(1);
    expect(sharedQuery('health', load).get().data).toBe('ok');
  });

  it('takes options from whoever made it (a health read that keeps its last value)', async () => {
    const { load, calls } = loads<string>();
    const q = sharedQuery('h', load, { keepOnError: true });
    q.subscribe(() => {});
    calls[0].resolve('ok');
    await flush();
    q.fetch();
    calls[1].reject(new Error('down'));
    await flush();
    expect(q.get().data).toBe('ok');
  });

  it('a failed load is not kept: the next reader asks again', async () => {
    const { load, calls } = loads<string>();
    const stop = sharedQuery('settings', load).subscribe(() => {});
    calls[0].reject(new Error('down'));
    await flush();
    stop();
    sharedQuery('settings', load).subscribe(() => {});
    expect(load).toHaveBeenCalledTimes(2);
  });

  it('a reload reaches every reader', async () => {
    const { load, calls } = loads<number>();
    const q = sharedQuery('disk', load);
    const one = vi.fn();
    const two = vi.fn();
    q.subscribe(one);
    q.subscribe(two);
    calls[0].resolve(1);
    await flush();
    one.mockClear();
    two.mockClear();
    q.fetch();
    calls[1].resolve(2);
    await flush();
    expect(q.get().data).toBe(2);
    expect([one.mock.calls.length, two.mock.calls.length]).toEqual([2, 2]);
  });
});
