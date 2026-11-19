// A value read from arenad, outside React: who reads it, the last answer, and when to ask again.
// `useQuery` puts one on screen; the rules live here so they can be tested without a DOM.
import { errorMessage } from '@/lib/errors';

export interface QueryState<T> {
  /** The last answer; null before the first, and after a failure unless `keepOnError`. */
  data: T | null;
  error: string | null;
  loading: boolean;
}

export interface QueryOptions<T> {
  /** Ask again every `ms` while `while(value)` holds (something still runs); a failure stops it until a reload. */
  poll?: { ms: number; while: (v: T) => boolean };
  /** Keep the last answer through a failed read, the error beside it. */
  keepOnError?: boolean;
  /** Every new reader asks again (one ask for readers that arrive together), the last answer shown meanwhile: what can change while the tab is open (the health, the benchmark). */
  fresh?: boolean;
}

/**
 * One value and its readers. A reader that finds no answer (none yet, or a failure) and nothing
 * asking starts a load; the last one to leave stops polling; only the newest request's answer lands.
 */
export class Query<T> {
  private state: QueryState<T>;
  private readonly readers = new Set<() => void>();
  private request = 0;
  private loaded = false;
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(
    private readonly load: () => Promise<T>,
    private readonly options: () => QueryOptions<T> = () => ({}),
    seed: T | null = null,
  ) {
    this.state = { data: seed, error: null, loading: false };
  }

  get = (): QueryState<T> => this.state;

  subscribe = (reader: () => void): (() => void) => {
    this.readers.add(reader);
    if ((!this.loaded || this.options().fresh) && !this.state.loading) this.fetch();
    return () => {
      this.readers.delete(reader);
      if (this.readers.size === 0) this.stop();
    };
  };

  /** Ask now; an answer to an earlier request no longer lands. `clear`: start over, no value or error meanwhile. */
  fetch = ({ clear = false }: { clear?: boolean } = {}): void => {
    const id = ++this.request;
    clearTimeout(this.timer);
    this.set(clear ? { data: null, error: null, loading: true } : { ...this.state, loading: true });
    this.load().then(
      (data) => {
        if (id !== this.request) return;
        this.loaded = true;
        this.set({ data, error: null, loading: false });
        const poll = this.options().poll;
        if (poll?.while(data) && this.readers.size) this.timer = setTimeout(() => this.fetch(), poll.ms);
      },
      (e) => {
        if (id !== this.request) return;
        this.set({ data: this.options().keepOnError ? this.state.data : null, error: errorMessage(e), loading: false });
      },
    );
  };

  /** Change the value on screen (a streamed event); the next answer replaces it. */
  update = (fn: (data: T | null) => T | null): void => {
    this.set({ ...this.state, data: fn(this.state.data) });
  };

  private stop() {
    clearTimeout(this.timer);
    if (this.state.loading) {
      this.request++;
      this.set({ ...this.state, loading: false });
    }
  }

  private set(next: QueryState<T>) {
    this.state = next;
    this.readers.forEach((r) => r());
  }
}

const shared = new Map<string, Query<unknown>>();

/** The page-wide query for `key` (the tasks, the health, the review settings): one answer, every reader. */
export function sharedQuery<T>(key: string, load: () => Promise<T>, options: QueryOptions<T> = {}): Query<T> {
  let q = shared.get(key) as Query<T> | undefined;
  if (!q) {
    q = new Query(load, () => options);
    shared.set(key, q as Query<unknown>);
  }
  return q;
}

/** For tests: forget every shared answer. */
export const forgetShared = () => shared.clear();
