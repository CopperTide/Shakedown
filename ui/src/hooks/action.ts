// A request a person starts (start a run, fork, delete): busy while it runs, its failure in words.
import { errorMessage } from '@/lib/errors';

export interface ActionState {
  busy: boolean;
  error: string | null;
}

export interface ActionOptions {
  /** Stay busy after success: the page moves on (navigates) and the button should not come back. */
  stayBusy?: boolean;
}

/** Runs `fn`, telling `set` when it starts and how it ended; its answer, or undefined when it failed. */
export async function runAction<R>(fn: () => Promise<R>, set: (s: ActionState) => void, { stayBusy = false }: ActionOptions = {}): Promise<R | undefined> {
  set({ busy: true, error: null });
  try {
    const r = await fn();
    if (!stayBusy) set({ busy: false, error: null });
    return r;
  } catch (e) {
    set({ busy: false, error: errorMessage(e) });
    return undefined;
  }
}
