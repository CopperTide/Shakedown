// Project settings → Deleted files: the most recent delete, and undoing it after its toast is gone.
import { RotateCcw } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { ApiError } from '@/api/client';
import { lastDelete, restoreDeleted } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { ErrorState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';
import { projectChanged } from '@/hooks/useProjectItems';
import { errorMessage } from '@/lib/errors';

/** The most recent delete, or null when there is nothing to undo. */
const load = () => lastDelete().catch((e: unknown) => {
  if (e instanceof ApiError && e.status === 404) return null;
  throw e;
});

export default function LastDelete() {
  const q = useQuery('last-delete', load);
  const [error, setError] = useState<string | null>(null);
  const undo = (trash: string, label: string) => {
    restoreDeleted(trash).then(() => { setError(null); projectChanged(); toast(`Restored ${label}`); q.reload(); }, (e: unknown) => setError(errorMessage(e)));
  };
  const last = q.data;
  return (
    <>
      {q.error && <ErrorState error={q.error} onRetry={q.reload} />}
      <Card title="Last delete" hint="Deletes move files to the project's trash, .shakedown/trash/. The last 20 deletes, and every delete of the last 30 days, are kept.">
        {last
          ? (
            <div className="grid gap-2 px-[18px] py-3 text-sm">
              <p><span className="font-medium">{last.label}</span> <span className="text-muted">· {last.files.length} file{last.files.length === 1 ? '' : 's'}</span></p>
              <ul className="font-mono text-2xs text-faint">{last.files.map((f) => <li key={f}>{f}</li>)}</ul>
              {error && <p role="alert" className="text-alarm">{error}</p>}
              <div><Button onClick={() => undo(last.trash, last.label)}><RotateCcw />Undo last delete</Button></div>
            </div>
          )
          : q.data === null && <p className="px-[18px] py-3 text-sm text-faint">Nothing to undo.</p>}
      </Card>
    </>
  );
}
