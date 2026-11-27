// The saved views: each opens again in the builder or the SQL tab, and can be deleted.
import { Trash2 } from 'lucide-react';
import { useState } from 'react';
import { deleteView, listViews, type SavedView } from '@/api/explore';
import { Button } from '@/components/ui/button';
import { Dialog, DialogActions, DialogContent } from '@/components/ui/dialog';
import { Panel } from '@/components/ui/panel';
import { Tag } from '@/components/ui/status';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';
import { errorMessage } from '@/lib/errors';
import { ago, formatTime } from '@/lib/time';

function Row({ v, onOpen, onDelete }: { v: SavedView; onOpen: (v: SavedView) => void; onDelete: (v: SavedView) => void }) {
  return (
    <li className="flex items-center gap-3 px-4 py-2">
      <button type="button" onClick={() => onOpen(v)} className="flex min-w-0 flex-1 items-center gap-2 text-left hover:text-accent">
        <span className="min-w-0 truncate text-sm">{v.name}</span>
        <Tag>{v.kind === 'sql' ? 'SQL' : 'builder'}</Tag>
      </button>
      <time dateTime={v.updated_at} title={formatTime(v.updated_at)} className="text-2xs text-faint">{ago(v.updated_at)}</time>
      <Button size="icon" variant="ghost" aria-label={`Delete ${v.name}`} onClick={() => onDelete(v)}><Trash2 /></Button>
    </li>
  );
}

function ConfirmDelete({ v, onClose, onDeleted }: { v: SavedView | null; onClose: () => void; onDeleted: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const go = async () => {
    try {
      await deleteView(v!.id);
      onDeleted();
      onClose();
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  return (
    <Dialog open={v != null} onOpenChange={(o) => { if (!o) { setError(null); onClose(); } }}>
      {v && (
        <DialogContent title={`Delete ${v.name}?`} description="The view is gone for everyone; the runs it shows stay.">
          {error && <p role="alert" className="px-4 pt-3 text-xs text-alarm">{error}</p>}
          <DialogActions danger label="Delete" onCancel={onClose} onGo={() => void go()} />
        </DialogContent>
      )}
    </Dialog>
  );
}

export default function SavedViews({ onOpen }: { onOpen: (v: SavedView) => void }) {
  const q = useQuery('explore:views', listViews);
  const [doomed, setDoomed] = useState<SavedView | null>(null);
  if (!q.data) return q.error ? <ErrorState error={q.error} onRetry={q.retry} /> : <Panel><LoadingState label="Loading saved views…" rows={3} /></Panel>;
  return (
    <Panel>
      {q.data.items.length === 0
        ? <EmptyState title="No saved views yet" hint="Build a query or write SQL, then press Save view to keep it here." />
        : <ul aria-label="Saved views" className="divide-y divide-line">{q.data.items.map((v) => <Row key={v.id} v={v} onOpen={onOpen} onDelete={setDoomed} />)}</ul>}
      <ConfirmDelete v={doomed} onClose={() => setDoomed(null)} onDeleted={() => q.reload()} />
    </Panel>
  );
}
