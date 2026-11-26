import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';

/** How many of a long list to draw: `page` at first, `page` more on each ask; back to `page` when `reset` changes. */
export function useShown(page: number, reset: unknown): [number, () => void] {
  const [shown, setShown] = useState(page);
  useEffect(() => setShown(page), [page, reset]);
  return [shown, () => setShown((n) => n + page)];
}

/** "Showing 25 of 310 · Show 25 more", or nothing once all are drawn. */
export function ShowMore({ shown, total, page, onMore }: { shown: number; total: number; page: number; onMore: () => void }) {
  if (shown >= total) return null;
  return (
    <div className="flex items-center gap-2 px-3 py-2 text-xs text-muted">
      <span>Showing {shown.toLocaleString()} of {total.toLocaleString()}</span>
      <Button size="sm" onClick={onMore}>Show {Math.min(page, total - shown)} more</Button>
    </div>
  );
}
