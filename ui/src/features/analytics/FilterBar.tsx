import { ChevronDown, X } from 'lucide-react';
import { useState } from 'react';
import type { Options } from '@/api/analytics';
import { Button } from '@/components/ui/button';
import { Checkbox, Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Segmented } from '@/components/ui/segmented';
import { cn } from '@/lib/cn';
import { type AnalyticsFilter, DIMENSION_LABEL, DIMENSIONS, type Dimension, isNarrowed, RANGES } from './filters';

/** A searchable multi-pick of one dimension's values; a value picked but absent from the range stays listed. */
function Picker({ dim, options, picked, onChange }: { dim: Dimension; options: string[]; picked: string[]; onChange: (v: string[]) => void }) {
  const [query, setQuery] = useState('');
  const label = DIMENSION_LABEL[dim];
  const all = [...new Set([...picked, ...options])];
  const shown = all.filter((v) => v.toLowerCase().includes(query.toLowerCase()));
  const toggle = (v: string) => onChange(picked.includes(v) ? picked.filter((p) => p !== v) : [...picked, v]);
  return (
    <Popover onOpenChange={() => setQuery('')}>
      <PopoverTrigger asChild>
        <Button size="md" className={cn('h-8 text-xs', picked.length > 0 && 'border-accent/40 bg-accent-soft')}>
          {label}
          {picked.length > 0 && <span className="rounded bg-accent/10 px-1.5 text-[10px] tabular-nums">{picked.length}</span>}
          <ChevronDown className="text-faint" />
        </Button>
      </PopoverTrigger>
      <PopoverContent aria-label={`Filter by ${label.toLowerCase()}`} className="w-72 max-w-[calc(100vw-2rem)] p-0">
        <div className="border-b border-line p-3">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs font-semibold">Filter by {label.toLowerCase()}</span>
            {picked.length > 0 && <button type="button" onClick={() => onChange([])} className="text-xs text-muted hover:text-fg">Reset</button>}
          </div>
          {all.length > 6 && <Input aria-label={`Find a ${label.toLowerCase()}`} placeholder="Find…" value={query} onChange={(e) => setQuery(e.target.value)} className="h-8 text-xs" />}
        </div>
        <div className="max-h-60 overflow-y-auto p-1.5">
          {shown.map((v) => (
            <label key={v} className={cn('flex cursor-pointer items-center gap-2.5 rounded-md px-2 py-2 text-xs hover:bg-hover', picked.includes(v) && 'bg-accent-soft')}>
              <Checkbox checked={picked.includes(v)} onChange={() => toggle(v)} />
              <span className="min-w-0 flex-1 break-words">{v}</span>
            </label>
          ))}
          {!shown.length && <p className="px-2 py-4 text-xs text-muted">{all.length ? 'Nothing matches.' : 'No runs in this range.'}</p>}
        </div>
      </PopoverContent>
    </Popover>
  );
}

/** The time range and the pickers in one row, then the picks as removable chips. */
export default function FilterBar({ filter, options, onChange }: { filter: AnalyticsFilter; options: Options | null; onChange: (f: AnalyticsFilter) => void }) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <Segmented label="Time range" value={filter.range} options={RANGES} onChange={(range) => onChange({ ...filter, range })} />
        {DIMENSIONS.map((k) => (
          <Picker key={k} dim={k} options={options?.[k] ?? []} picked={filter[k]} onChange={(v) => onChange({ ...filter, [k]: v })} />
        ))}
      </div>
      {isNarrowed(filter) && (
        <div className="flex flex-wrap items-center gap-1.5">
          {DIMENSIONS.flatMap((k) => filter[k].map((v) => (
            <button key={`${k}:${v}`} type="button" aria-label={`Remove ${DIMENSION_LABEL[k].toLowerCase()} filter: ${v}`}
              onClick={() => onChange({ ...filter, [k]: filter[k].filter((x) => x !== v) })}
              className="inline-flex max-w-full items-center gap-1.5 rounded-md border border-line bg-raised px-2 py-1 text-2xs hover:border-accent hover:bg-hover">
              <span className="text-muted">{DIMENSION_LABEL[k]}:</span><span className="truncate">{v}</span><X className="size-3 shrink-0 text-muted" />
            </button>
          )))}
          <Button size="sm" variant="ghost" className="h-6 text-2xs" onClick={() => onChange({ ...filter, world: [], scenario: [], setup: [], model: [] })}>Clear all</Button>
        </div>
      )}
    </div>
  );
}
