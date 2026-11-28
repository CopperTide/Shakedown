// The small form pieces every editor uses: a labelled text box, a labelled choice, a list of items
// with Remove (and Edit, where items can be edited in place). Explanations are hints on the label.
import { Pencil, Trash2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { Hint } from '@/components/ui/hint';
import { Input, NativeSelect, Textarea } from '@/components/ui/input';
import { cn } from '@/lib/cn';

export function TextField({ label, hint, value, onChange, placeholder, mono = false, multiline = false }: {
  label: string;
  /** What the field is for, on hovering its label. */
  hint?: ReactNode;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  mono?: boolean;
  multiline?: boolean;
}) {
  const cls = mono ? 'font-mono' : undefined;
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-muted"><Hint hint={hint}>{label}</Hint></span>
      {multiline
        ? <Textarea aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} className={cls} rows={4} />
        : <Input aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} className={cls} />}
    </label>
  );
}

export function ChoiceField<V extends string>({ label, hint, value, options, onChange }: {
  label: string;
  hint?: ReactNode;
  value: V;
  options: { value: V; label: string }[];
  onChange: (v: V) => void;
}) {
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-muted"><Hint hint={hint}>{label}</Hint></span>
      <NativeSelect aria-label={label} value={value} onChange={(e) => onChange(e.target.value as V)}>
        {options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
      </NativeSelect>
    </label>
  );
}

/** Items as rows, each with Remove (and Edit with `onEdit`; the row `editing` is marked); `empty` (and on hovering it, `emptyHint`) when there are none. */
export function ItemRows<T>({ items, render, onRemove, onEdit, editing, empty, emptyHint }: {
  items: T[];
  render: (item: T) => ReactNode;
  onRemove: (i: number) => void;
  onEdit?: (i: number) => void;
  editing?: number | null;
  empty: string;
  emptyHint?: ReactNode;
}) {
  if (items.length === 0) return <p className="px-[18px] py-4 text-sm text-faint"><Hint hint={emptyHint}>{empty}</Hint></p>;
  return (
    <ul className="divide-y divide-line">
      {items.map((item, i) => (
        <li key={i} aria-current={editing === i || undefined} className={cn('flex items-center gap-3 px-[18px] py-2.5 text-sm', editing === i && 'bg-hover')}>
          <div className="min-w-0 flex-1">{render(item)}</div>
          {onEdit && <Button variant="ghost" size="sm" aria-label="Edit" onClick={() => onEdit(i)}><Pencil /></Button>}
          <Button variant="ghost" size="sm" aria-label="Remove" onClick={() => onRemove(i)}><Trash2 /></Button>
        </li>
      ))}
    </ul>
  );
}

/** A section's heading and its actions; `about`, what the section is for, shows on hovering the heading. */
export function SectionHead({ title, about, actions }: { title: string; about?: ReactNode; actions?: ReactNode }) {
  return (
    <header className="mb-1 flex flex-wrap items-center justify-between gap-3 border-b border-line pb-3">
      <h2 className="text-lg font-semibold"><Hint hint={about}>{title}</Hint></h2>
      {actions}
    </header>
  );
}
