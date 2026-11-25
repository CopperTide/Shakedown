// Renaming a world group from its header: every world in it moves to the new name, each file checked first.
import { Pencil } from 'lucide-react';
import { useState } from 'react';
import { renameGroup } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Dialog, DialogActions, DialogContent } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { projectChanged } from '@/hooks/useProjectItems';
import { errorMessage } from '@/lib/errors';

/** `count` worlds are in group `name`; `groups` are every group's name. */
export default function RenameGroupDialog({ name, count, groups }: { name: string; count: number; groups: string[] }) {
  const [open, setOpen] = useState(false);
  const [to, setTo] = useState(name);
  const [error, setError] = useState<string | null>(null);
  const show = (o: boolean) => { setOpen(o); setTo(name); setError(null); };
  const rename = async () => {
    try {
      await renameGroup(name, to.trim() || null);
      setOpen(false);
      projectChanged();
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  const into = to.trim() !== name && groups.includes(to.trim()) ? to.trim() : null;
  return (
    <Dialog open={open} onOpenChange={show}>
      <Button variant="ghost" size="sm" aria-label={`Rename group ${name}`} title="Rename group" onClick={() => show(true)}><Pencil /></Button>
      <DialogContent title={`Rename ${name}`} description="Every world in the group moves to the new name. An empty name takes them out of any group.">
        <div className="grid gap-3 px-4 py-3 text-sm">
          <label className="grid gap-1"><span>Group name</span><Input aria-label="Group name" value={to} onChange={(e) => setTo(e.target.value)} /></label>
          {into && <p className="text-xs text-muted">Moves {count} world{count === 1 ? '' : 's'} into {into}, which already exists.</p>}
          {error && <p role="alert" className="text-alarm">{error}</p>}
        </div>
        <DialogActions onCancel={() => show(false)} onGo={() => { void rename(); }} disabled={to.trim() === name} label="Rename" />
      </DialogContent>
    </Dialog>
  );
}
