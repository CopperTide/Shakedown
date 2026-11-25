import { useCallback, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/cn';
import { type Facet, FACETS, type LogFilter, type LogRow, chipsOf, isActive, matching, NO_FILTER, parseFilter, toggled, writeFilter } from './logFilter';

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

/** Search over the detail, and (folded) each facet's values as chips with their counts. */
export function LogFilters({ rows, filter, setFilter, shown }: {
  rows: LogRow[]; filter: LogFilter; setFilter: (f: LogFilter) => void; shown: number | null;
}) {
  const active = isActive(filter);
  const picked = FACETS.reduce((n, k) => n + filter[k].length, 0);
  const [open, setOpen] = useState(picked > 0);
  const chips = useMemo(() => chipsOf(rows, filter), [rows, filter]);
  return (
    <div className="flex flex-col gap-1.5 border-b border-line px-3 py-1.5">
      <div className="flex flex-wrap items-center gap-2">
        <Input aria-label="Search the steps' detail" placeholder="Search detail…" value={filter.q} className="h-7 w-48 text-xs"
          onChange={(e) => setFilter({ ...filter, q: e.target.value })} />
        <Button size="sm" variant="ghost" aria-expanded={open} onClick={() => setOpen(!open)}>
          Filter{picked > 0 && ` (${picked})`} {open ? '▴' : '▾'}
        </Button>
        {active && <Button size="sm" variant="ghost" onClick={() => setFilter(NO_FILTER)}>Clear all</Button>}
        {shown != null && <span className="text-2xs text-muted">{shown} of {rows.length} steps</span>}
      </div>
      {open && FACETS.map((k) => chips[k].length > 1 || filter[k].length ? (
        <div key={k} role="group" aria-label={TITLE[k]} className="flex flex-wrap items-center gap-1">
          <span className="w-16 shrink-0 text-2xs font-medium text-muted">{TITLE[k]}</span>
          {chips[k].map((c) => (
            <button key={c.value} type="button" aria-pressed={c.on} onClick={() => setFilter(toggled(filter, k, c.value))}
              className={cn('rounded-full border px-2 py-0.5 text-2xs', c.on ? 'border-accent bg-accent-soft text-fg' : 'border-line text-muted hover:bg-hover hover:text-fg', c.count === 0 && !c.on && 'opacity-50')}>
              {c.value} <span className="font-mono text-faint">{c.count}</span>
            </button>
          ))}
        </div>
      ) : null)}
    </div>
  );
}
