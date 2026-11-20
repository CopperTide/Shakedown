import { describe, expect, it } from 'vitest';
import { migrateStorage } from './storageNames';

/** A Storage over a Map, as a browser's localStorage behaves. */
function memory(entries: Record<string, string>) {
  const m = new Map(Object.entries(entries));
  return {
    get length() { return m.size; },
    key: (i: number) => [...m.keys()][i] ?? null,
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => { m.set(k, v); },
    removeItem: (k: string) => { m.delete(k); },
    entries: () => Object.fromEntries(m),
  };
}

describe('stored settings under their old names', () => {
  it('move once to shakedown.*: the old is read, the new written, the old deleted', () => {
    const s = memory({ 'arena.theme': 'dark', 'arena.team.v1': '{"id":"t"}', other: 'x' });
    expect(migrateStorage(s).sort()).toEqual(['arena.team.v1', 'arena.theme']);
    expect(s.entries()).toEqual({ other: 'x', 'shakedown.theme': 'dark', 'shakedown.team.v1': '{"id":"t"}' });
    expect(migrateStorage(s)).toEqual([]);
  });
  it('keep a value already stored under the new name', () => {
    const s = memory({ 'arena.theme': 'dark', 'shakedown.theme': 'light' });
    migrateStorage(s);
    expect(s.entries()).toEqual({ 'shakedown.theme': 'light' });
  });
  it('do nothing without storage, or when it refuses', () => {
    expect(migrateStorage(null)).toEqual([]);
    const blocked = { ...memory({ 'arena.theme': 'dark' }), setItem: () => { throw new Error('quota'); } };
    expect(migrateStorage(blocked)).toEqual([]);
  });
});
