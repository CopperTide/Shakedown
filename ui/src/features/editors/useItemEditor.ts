// An editor's state: the item as saved, the draft being edited, and saving it back to its file.
import { useCallback, useEffect, useState } from 'react';
import { getItem, putItem } from '@/api/project';
import { projectChanged } from '@/hooks/useProjectItems';
import { errorMessage } from '@/lib/errors';
import type { Item, ItemKind, Problem } from '@/model/project';

export interface SaveError {
  message: string;
  /** The JSON pointer the project named, when it named one. */
  pointer?: string;
}

/** The pointer in a file error ("<file> at /a/0/b: message"), if any. */
export function pointerOf(message: string): string | undefined {
  return / at (\/[^:\s]*):/.exec(message)?.[1];
}

export function useItemEditor<B>(kind: ItemKind, id: string) {
  const [item, setItem] = useState<Item<B> | null>(null);
  const [draft, setDraft] = useState<B | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<SaveError | null>(null);
  useEffect(() => {
    let live = true;
    getItem<B>(kind, id).then((i) => { if (live) { setItem(i); setDraft(i.body); } }, (e) => live && setError({ message: errorMessage(e) }));
    return () => { live = false; };
  }, [kind, id]);
  const save = useCallback(async () => {
    if (draft == null) return;
    setSaving(true);
    try {
      const saved = await putItem<B>(kind, id, draft, true, item?.revision);
      if ('body' in saved) { setItem(saved); setDraft(saved.body); projectChanged(); }
      setError(null);
    } catch (e) {
      const message = errorMessage(e);
      setError({ message, pointer: pointerOf(message) });
    } finally {
      setSaving(false);
    }
  }, [kind, id, draft, item?.revision]);
  const discard = useCallback(() => { setDraft(item?.body ?? null); setError(null); }, [item]);
  const dirty = item != null && JSON.stringify(draft) !== JSON.stringify(item.body);
  const problems: Problem[] = item?.problems ?? [];
  return { item, draft, setDraft, dirty, save, discard, saving, error, problems };
}
