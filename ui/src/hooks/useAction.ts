import { useCallback, useRef, useState } from 'react';
import { type ActionOptions, type ActionState, runAction } from './action';

const READY: ActionState = { busy: false, error: null };

/** `fn` as something a button runs: `run(...args)`, `busy` meanwhile, `error` when it failed, `reset` to clear it. */
export function useAction<A extends unknown[], R>(fn: (...args: A) => Promise<R>, options: ActionOptions = {}) {
  const [state, setState] = useState<ActionState>(READY);
  const latest = useRef({ fn, options });
  latest.current = { fn, options };
  const run = useCallback((...args: A) => runAction(() => latest.current.fn(...args), setState, latest.current.options), []);
  const reset = useCallback(() => setState(READY), []);
  return { ...state, run, reset };
}
