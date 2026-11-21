import { Button } from './button';

/** The bar that appears after a change on a settings page: what changed, Discard and Save. */
export function SaveBar({ dirty, summary, onSave, onDiscard, saving = false }: {
  dirty: boolean;
  summary: string;
  onSave: () => void;
  onDiscard: () => void;
  saving?: boolean;
}) {
  if (!dirty) return null;
  return (
    <div role="region" aria-label="Unsaved changes" className="fixed bottom-6 left-1/2 z-40 flex w-[min(560px,calc(100vw-48px))] -translate-x-1/2 items-center gap-3 rounded-lg bg-primary py-2 pl-4 pr-2 text-sm text-primary-fg shadow-pop">
      <span className="flex-1">{summary}</span>
      <Button variant="ghost" size="sm" className="text-primary-fg hover:bg-transparent hover:opacity-80" onClick={onDiscard}>Discard</Button>
      <Button size="sm" className="border-copper bg-copper text-[var(--bg)] hover:bg-copper" disabled={saving} onClick={onSave}>Save</Button>
    </div>
  );
}
