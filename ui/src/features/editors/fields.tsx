// The small form pieces every editor uses: a labelled text box, a labelled choice, a list of items
// with Remove.
import { Trash2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { Input, NativeSelect, Textarea } from '@/components/ui/input';

export function TextField({ label, value, onChange, placeholder, mono = false, multiline = false }: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  mono?: boolean;
  multiline?: boolean;
}) {
  const cls = mono ? 'font-mono' : undefined;
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-muted">{label}</span>
      {multiline
        ? <Textarea aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} className={cls} rows={4} />
        : <Input aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} placeholder={placeholder} className={cls} />}
    </label>
  );
}

export function ChoiceField<V extends string>({ label, value, options, onChange }: {
  label: string;
  value: V;
  options: { value: V; label: string }[];
  onChange: (v: V) => void;
}) {
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-muted">{label}</span>
      <NativeSelect aria-label={label} value={value} onChange={(e) => onChange(e.target.value as V)}>
        {options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
      </NativeSelect>
    </label>
  );
}

/** Items as rows, each with Remove; `empty` when there are none. */
export function ItemRows<T>({ items, render, onRemove, empty }: {
  items: T[];
  render: (item: T) => ReactNode;
  onRemove: (i: number) => void;
  empty: string;
}) {
  if (items.length === 0) return <p className="px-[18px] py-4 text-sm text-faint">{empty}</p>;
  return (
    <ul className="divide-y divide-line">
      {items.map((item, i) => (
        <li key={i} className="flex items-center gap-3 px-[18px] py-2.5 text-sm">
          <div className="min-w-0 flex-1">{render(item)}</div>
          <Button variant="ghost" size="sm" aria-label="Remove" onClick={() => onRemove(i)}><Trash2 /></Button>
        </li>
      ))}
    </ul>
  );
}

/** A section's heading and one-line description. */
export function SectionHead({ title, about }: { title: string; about: string }) {
  return (
    <header className="mb-5 border-b border-line pb-4">
      <h2 className="text-lg font-semibold">{title}</h2>
      <p className="mt-1 max-w-[68ch] text-sm text-muted">{about}</p>
    </header>
  );
}
