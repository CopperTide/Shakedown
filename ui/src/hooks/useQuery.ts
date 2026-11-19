import { useMemo, useRef, useSyncExternalStore } from 'react';
import { Query, type QueryOptions, type QueryState, sharedQuery } from './query';

export interface UseQueryOptions<T> extends QueryOptions<T> {
  /** One answer for the whole page, read by every component that asks for this key. */
  shared?: boolean;
  /** Show the previous key's answer until the new key's lands (a refresh, not a different thing). */
  keepPrevious?: boolean;
}

/** A list read again every `ms` while `still(value)` holds; a failed read keeps the last value beside its error. */
export const pollWhile = <T>(ms: number, still: (v: T) => boolean): UseQueryOptions<T> =>
  ({ poll: { ms, while: still }, keepOnError: true, keepPrevious: true });

const IDLE: QueryState<never> = { data: null, error: null, loading: false };
const idle = () => IDLE;
const none = () => () => {};

/**
 * `load()` for `key`, again when the key changes or on `reload`; null `key` reads nothing.
 * A response for an earlier key or an earlier request never lands.
 */
export function useQuery<T>(key: string | null, load: () => Promise<T>, options: UseQueryOptions<T> = {}) {
  const latest = useRef({ load, options });
  latest.current = { load, options };
  const previous = useRef<T | null>(null);
  const { shared, keepPrevious } = options;
  const query = useMemo(() => {
    if (key == null) return null;
    const run = () => latest.current.load();
    if (shared) return sharedQuery(key, run);
    return new Query(run, () => latest.current.options, keepPrevious ? previous.current : null);
  }, [key, shared, keepPrevious]);
  const state = useSyncExternalStore(query?.subscribe ?? none, query?.get ?? idle, query?.get ?? idle);
  previous.current = state.data;
  return { ...state, reload: query?.fetch ?? noop };
}

function noop() {}
