import { useCallback, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Search, SlidersHorizontal, ChevronDown, X } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Hint } from '@/components/ui/hint';
import { Checkbox, Input } from '@/components/ui/input';
import { cn } from '@/lib/cn';
import { type Chip, type Facet, FACETS, type LogFilter, type LogRow, chipsOf, isActive, matching, NO_FILTER, parseFilter, toggled, writeFilter } from './logFilter';

/** The run page's step filter, kept in the address; `matches` is null when nothing is filtered. */
export function useLogFilter(rows: LogRow[]) {
  const [sp, setSp] = useSearchParams();
  const key = ['action', 'agent', 'outcome', 'q'].map((k) => sp.get(k) ?? '').join('\n');
  const filter = useMemo(() => parseFilter(sp), [key]); // eslint-disable-line react-hooks/exhaustive-deps
  const setFilter = useCallback((f: LogFilter) => setSp((prev) => writeFilter(prev, f), { replace: true }), [setSp]);
  const matches = useMemo(() => matching(rows, filter), [rows, filter]);
  return { filter, setFilter, matches };
}

const TITLE: Record<Facet, string> = { action: 'Action', agent: 'Agent', outcome: 'Outcome' };

/** Searchable multi-select with counts conditioned on the other active filters. */
function FacetFilter({ facet, chips, onToggle, onClear }: {
  facet: Facet; chips: Chip[]; onToggle: (value: string) => void; onClear: () => void;
}) {
  const [query, setQuery] = useState('');
  const count = chips.filter((c) => c.on).length;
  const options = chips.filter((c) => c.value.toLowerCase().includes(query.toLowerCase()));
  return (
    <Popover onOpenChange={() => setQuery('')}>
      <PopoverTrigger asChild>
        <Button size="md" className={cn('h-8 text-xs', count > 0 && 'border-accent/40 bg-accent-soft')}>
          {TITLE[facet]}
          {count > 0 && <span className="rounded bg-accent/10 px-1.5 text-[10px] tabular-nums">{count}</span>}
          <ChevronDown className="text-faint" />
        </Button>
      </PopoverTrigger>
      <PopoverContent aria-label={`Filter by ${TITLE[facet].toLowerCase()}`} className="w-72 max-w-[calc(100vw-2rem)] p-0">
        <div className="border-b border-line p-3">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs font-semibold"><Hint hint="Select multiple values to match any of them.">Filter by {TITLE[facet].toLowerCase()}</Hint></span>
            {count > 0 && <button type="button" onClick={onClear} className="text-xs text-muted hover:text-fg">Reset</button>}
          </div>
          <Input aria-label={`Find ${TITLE[facet].toLowerCase()} options`} placeholder="Find an option…" value={query} onChange={(e) => setQuery(e.target.value)} className="h-8 text-xs" />
        </div>
        <div className="max-h-60 overflow-y-auto p-1.5">
          {options.map((c) => (
            <label key={c.value} className={cn('flex cursor-pointer items-center gap-2.5 rounded-md px-2 py-2 text-xs hover:bg-hover', c.on && 'bg-accent-soft')}>
              <Checkbox checked={c.on} onChange={() => onToggle(c.value)} />
              <span className="min-w-0 flex-1 break-words">{c.value}</span>
              <span className="text-2xs tabular-nums text-muted">{c.count.toLocaleString()}</span>
            </label>
          ))}
          {!options.length && <p className="px-2 py-4 text-xs text-muted">No options</p>}
        </div>
      </PopoverContent>
    </Popover>
  );
}

/** A persistent filter toolbar with removable selections and shareable URL state. */
export function LogFilters({ rows, filter, setFilter, shown }: {
  rows: LogRow[]; filter: LogFilter; setFilter: (f: LogFilter) => void; shown: number | null;
}) {
  const active = isActive(filter);
  const chips = useMemo(() => chipsOf(rows, filter), [rows, filter]);
  return (
    <div className="border-b border-line bg-raised/40">
      <div className="flex flex-wrap items-center gap-2 px-3 py-3">
        <div className="relative min-w-40 flex-1 sm:max-w-72">
          <Search aria-hidden="true" className="pointer-events-none absolute left-2.5 top-2 size-4 text-faint" />
          <Input aria-label="Search the steps' detail" placeholder="Search event details…" value={filter.q} className="h-8 pl-8 pr-8 text-xs"
            onChange={(e) => setFilter({ ...filter, q: e.target.value })} />
          {filter.q && <button type="button" aria-label="Clear search" onClick={() => setFilter({ ...filter, q: '' })} className="absolute right-1 top-1 rounded p-1 text-muted hover:bg-hover"><X className="size-4" /></button>}
        </div>
        <SlidersHorizontal aria-hidden="true" className="ml-1 hidden size-3.5 text-faint sm:block" />
        {FACETS.map((k) => <FacetFilter key={k} facet={k} chips={chips[k]} onToggle={(value) => setFilter(toggled(filter, k, value))} onClear={() => setFilter({ ...filter, [k]: [] })} />)}
        <span role="status" aria-label="Matching steps" className="ml-auto whitespace-nowrap pl-2 text-xs tabular-nums text-muted">
          <span className="font-medium text-fg">{(shown ?? rows.length).toLocaleString()}</span>{active && ` of ${rows.length.toLocaleString()}`} steps
        </span>
      </div>
      {active && <div className="flex flex-wrap items-center gap-1.5 border-t border-line/60 px-3 py-2">
        <span className="mr-1 text-2xs text-muted">Active filters</span>
        {FACETS.flatMap((k) => filter[k].map((value) => (
          <button key={`${k}:${value}`} type="button" aria-label={`Remove ${TITLE[k].toLowerCase()} filter: ${value}`} onClick={() => setFilter(toggled(filter, k, value))}
            className="inline-flex max-w-full items-center gap-1.5 rounded-md border border-line bg-raised px-2 py-1 text-2xs hover:border-accent hover:bg-hover">
            <span className="text-muted">{TITLE[k]}:</span><span className="truncate">{value}</span><X className="size-3 shrink-0 text-muted" />
          </button>
        )))}
        {filter.q.trim() && <span className="text-2xs text-muted">Text search</span>}
        <Button size="sm" variant="ghost" className="ml-auto h-6 text-2xs" onClick={() => setFilter(NO_FILTER)}>Clear all</Button>
      </div>}
    </div>
  );
}
