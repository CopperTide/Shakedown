// Pure updates of an editor's draft: each returns a changed copy and never touches the original.

type Path = (string | number)[];

/** `obj` with the value at `path` replaced by `value`; `undefined` removes an object key. */
export function setIn<T>(obj: T, path: Path, value: unknown): T {
  if (path.length === 0) return value as T;
  const [head, ...rest] = path;
  if (Array.isArray(obj)) {
    const copy = [...obj];
    copy[head as number] = setIn(copy[head as number], rest, value);
    return copy as T;
  }
  const record = { ...(obj as Record<string, unknown>) };
  const next = setIn(record[head as string], rest, value);
  if (next === undefined) delete record[head as string];
  else record[head as string] = next;
  return record as T;
}

/** `list` with `item` at the end. */
export const addTo = <T>(list: T[] | undefined, item: T): T[] => [...(list ?? []), item];

/** `list` with its `i`th item replaced by `item`. */
export const replaceAt = <T>(list: T[] | undefined, i: number, item: T): T[] => (list ?? []).map((x, j) => (j === i ? item : x));

/** `list` without its `i`th item. */
export const removeAt = <T>(list: T[] | undefined, i: number): T[] => (list ?? []).filter((_, j) => j !== i);
