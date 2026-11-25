// A kind's items as the project lists them: one answer per page, read again after any change.
import { useEffect } from 'react';
import { listItems } from '@/api/project';
import type { ItemKind } from '@/model/project';
import { useQuery } from './useQuery';

/** Pages dispatch this after they change project files: a save, a delete, a world moved to a group. */
export const PROJECT_CHANGED = 'shakedown:project-changed';

export const projectChanged = () => window.dispatchEvent(new Event(PROJECT_CHANGED));

/** The items of `kind`, shared by every reader on the page; a failed read keeps the last list. */
export function useProjectItems(kind: ItemKind) {
  const q = useQuery(`items:${kind}`, () => listItems(kind).then((r) => r.items), { shared: true, fresh: true, keepOnError: true });
  const { reload } = q;
  useEffect(() => {
    window.addEventListener(PROJECT_CHANGED, reload);
    return () => window.removeEventListener(PROJECT_CHANGED, reload);
  }, [reload]);
  return q;
}
