// A remembered view choice: this browser only, the default when storage is unavailable.
import { useState } from 'react';

export function usePref(key: string, initial: boolean): [boolean, (v: boolean) => void] {
  const [v, setV] = useState(() => { try { const s = localStorage.getItem(key); return s === null ? initial : s === '1'; } catch { return initial; } });
  return [v, (next) => { setV(next); try { localStorage.setItem(key, next ? '1' : '0'); } catch { /* storage unavailable */ } }];
}
