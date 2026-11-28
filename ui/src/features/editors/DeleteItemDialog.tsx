// The Delete button and its dialog; `compact` makes the button an icon, for a list row.
import { Trash2 } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import type { ItemKind } from '@/model/project';
import DeletePanel from './DeletePanel';
import { KINDS } from './kinds';

export default function DeleteItemDialog({ kind, id, label, onDeleted, compact = false }: {
  kind: ItemKind;
  id: string;
  label: string;
  onDeleted: () => void;
  compact?: boolean;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      {compact
        ? <Button variant="ghost" size="sm" aria-label={`Delete ${label}`} title="Delete" onClick={() => setOpen(true)}><Trash2 /></Button>
        : <Button onClick={() => setOpen(true)}><Trash2 />Delete</Button>}
      <DialogContent title={`Delete ${label}?`} hint={`Removes this ${KINDS[kind].one}'s file from the project.`}>
        <DeletePanel kind={kind} id={id} label={label} onDeleted={() => { setOpen(false); onDeleted(); }} onCancel={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  );
}
