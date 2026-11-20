// This browser's stored settings under their names until 2026-11-19 (`arena.*`) move once to
// `shakedown.*`: read the old, write the new (unless the new is already there), delete the old.
// main.tsx imports this first, so it runs before anything reads storage.

export const OLD_PREFIX = 'arena.';
export const PREFIX = 'shakedown.';

/** Moves every `arena.*` key of `storage` to `shakedown.*`; the keys moved. A blocked storage moves nothing. */
export function migrateStorage(storage: Pick<Storage, 'length' | 'key' | 'getItem' | 'setItem' | 'removeItem'> | null): string[] {
  if (!storage) return [];
  const moved: string[] = [];
  try {
    const old = Array.from({ length: storage.length }, (_, i) => storage.key(i)).filter((k): k is string => !!k?.startsWith(OLD_PREFIX));
    for (const key of old) {
      const value = storage.getItem(key);
      const next = PREFIX + key.slice(OLD_PREFIX.length);
      if (value != null && storage.getItem(next) == null) storage.setItem(next, value);
      storage.removeItem(key);
      moved.push(key);
    }
  } catch {
    /* storage blocked or full: the old keys stay, and the pages start from their defaults */
  }
  return moved;
}

function browserStorage(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage;
  } catch {
    return null;
  }
}

migrateStorage(browserStorage());
