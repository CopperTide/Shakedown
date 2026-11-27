// A searchable multi-pick in a popover: measures to show, or the experiments, setups and agents a
// query is narrowed to. A value picked but no longer offered stays listed.
import { ChevronDown } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox, Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { cn } from '@/lib/cn';

export interface Option { id: string; label: string; hint?: string }

/** One choice: its box, its name and what it means. */
function Row({ o, on, full, onToggle }: { o: Option; on: boolean; full: boolean; onToggle: (id: string) => void }) {
  return (
    <label title={o.hint} className={cn('flex cursor-pointer items-start gap-2.5 rounded-md px-2 py-2 text-xs hover:bg-hover', on && 'bg-accent-soft', !on && full && 'opacity-50')}>
      <Checkbox checked={on} disabled={!on && full} onChange={() => onToggle(o.id)} className="mt-0.5" />
      <span className="flex min-w-0 flex-1 flex-col"><span className="break-words">{o.label}</span>{o.hint && <span className="text-2xs text-muted">{o.hint}</span>}</span>
    </label>
  );
}

export default function MultiPicker({ label, options, picked, onChange, max, summary }: {
  label: string; options: Option[]; picked: string[]; onChange: (v: string[]) => void; max?: number; summary?: string;
}) {
  const [query, setQuery] = useState('');
  const all: Option[] = [...options, ...picked.filter((p) => !options.some((o) => o.id === p)).map((id) => ({ id, label: id }))];
  const shown = all.filter((o) => o.label.toLowerCase().includes(query.toLowerCase()));
  const full = max != null && picked.length >= max;
  const toggle = (id: string) => onChange(picked.includes(id) ? picked.filter((p) => p !== id) : [...picked, id]);
  return (
    <Popover onOpenChange={() => setQuery('')}>
      <PopoverTrigger asChild>
        <Button size="md" className={cn('h-8 max-w-72 text-xs', picked.length > 0 && 'border-accent/40 bg-accent-soft')}>
          <span className="truncate">{summary ?? label}</span>
          {picked.length > 0 && !summary && <span className="rounded bg-accent/10 px-1.5 text-[10px] tabular-nums">{picked.length}</span>}
          <ChevronDown className="text-faint" />
        </Button>
      </PopoverTrigger>
      <PopoverContent aria-label={label} className="w-72 max-w-[calc(100vw-2rem)] p-0">
        <div className="border-b border-line p-3">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs font-semibold">{label}</span>
            {picked.length > 0 && max == null && <button type="button" onClick={() => onChange([])} className="text-xs text-muted hover:text-fg">Reset</button>}
          </div>
          {all.length > 6 && <Input aria-label={`Find in ${label.toLowerCase()}`} placeholder="Find…" value={query} onChange={(e) => setQuery(e.target.value)} className="h-8 text-xs" />}
        </div>
        <div className="max-h-72 overflow-y-auto p-1.5">
          {shown.map((o) => <Row key={o.id} o={o} on={picked.includes(o.id)} full={full} onToggle={toggle} />)}
          {!shown.length && <p className="px-2 py-4 text-xs text-muted">{all.length ? 'Nothing matches.' : 'Nothing to pick yet.'}</p>}
        </div>
      </PopoverContent>
    </Popover>
  );
}
