import { memo, useMemo, useState } from 'react';
import { cn } from '@/lib/cn';
import { type BoardColumn, type Sort, boardColumns, boardRows, cellText, columnWidths, defaultSort, filterCounts, nextSort } from './board';
import type { Zone } from './format';
import { PanelEmpty, PanelFrame } from './PanelFrame';
import { type Cols, cols } from './rows';
import { Chip, TONE_RULE } from './tones';
import { type BoardData, type BoardDef, asTone } from './types';
import { useScroll, useSize } from './useSize';
import { windowOf } from './virtual';
import { useZone } from './zone';

const ROW = 22;
/** IBM Plex Mono's advance at the board's 11.5 px, and a cell's side padding. */
const CH = 6.9, PAD = 18;

/** Each column as wide as its widest text, then a filler that takes what is left of the panel. */
const gridColumns = (chars: number[]) => [...chars.map((w) => `${Math.ceil(w * CH + PAD)}px`), 'minmax(0, 1fr)'].join(' ');

function Filters({ def, counts, on, toggle }: { def: BoardDef; counts: number[]; on: number[]; toggle: (i: number) => void }) {
  return (
    <div className="flex min-w-0 items-center gap-1 overflow-x-auto">
      {(def.filters ?? []).map((f, i) => (
        <button key={f.id} type="button" aria-pressed={on.includes(i)} onClick={() => toggle(i)}
          className={cn('flex h-6 shrink-0 items-center gap-1.5 rounded-[3px] border px-1.5 text-[11px] whitespace-nowrap transition-colors',
            on.includes(i) ? 'border-fg bg-fg text-bg' : 'border-line text-muted hover:border-line-strong hover:text-fg')}>
          {f.label}<span className="font-mono tabular-nums opacity-70">{counts[i]}</span>
        </button>
      ))}
    </div>
  );
}

function Header({ columns, template, sort, onSort }: { columns: BoardColumn[]; template: string; sort: Sort | null; onSort: (i: number) => void }) {
  return (
    <div className="sticky top-0 z-10 grid border-b border-line-strong bg-surface pl-[3px]" style={{ gridTemplateColumns: template }}>
      {columns.map((col, i) => (
        <button key={`${i}-${col.label}`} type="button" onClick={() => onSort(i)} disabled={col.name == null}
          className={cn('flex h-6 min-w-0 items-center gap-1 px-2 text-[11px] font-medium whitespace-nowrap text-muted hover:text-fg', col.numeric && 'justify-end')}>
          <span className="truncate">{col.label}</span>
          {sort?.column === i && <span aria-hidden="true" className="font-mono text-[9px] text-accent">{sort.desc ? '▼' : '▲'}</span>}
        </button>
      ))}
    </div>
  );
}

/** A row; the same row isn't drawn again as the board scrolls. */
const Row = memo(function Row({ c, row, columns, template, zone }: { c: Cols; row: unknown[]; columns: BoardColumn[]; template: string; zone: Zone }) {
  const tone = asTone(c.get(row, 'tone'));
  return (
    <div className={cn('grid border-b border-l-[3px] border-b-line/70 hover:bg-hover', tone === 'neutral' ? 'border-l-transparent' : TONE_RULE[tone])}
      style={{ gridTemplateColumns: template, height: ROW }}>
      {columns.map((col, i) => {
        const text = cellText(c, row, col, zone);
        return (
          <div key={i} className={cn('flex min-w-0 items-center px-2 font-mono text-[11.5px] leading-none whitespace-nowrap tabular-nums', col.numeric && 'justify-end', i === 0 && 'font-medium')}>
            {col.format === 'chip' && col.name != null ? <Chip tone={tone}>{text}</Chip> : <span className="truncate" title={text}>{text}</span>}
          </div>
        );
      })}
    </div>
  );
});

function useBoard(def: BoardDef, data: BoardData, zone: Zone) {
  const c = useMemo(() => cols(data.columns), [data.columns]);
  const columns = useMemo(() => boardColumns(def, c), [def, c]);
  const [sort, setSort] = useState<Sort | null>(null);
  const [on, setOn] = useState<number[]>([]);
  const [search, setSearch] = useState('');
  const sorting = sort ?? defaultSort(def, columns, c);
  const order = useMemo(() => boardRows(data, columns, { sort: sorting, filters: on, search, zone }), [data, columns, sorting, on, search, zone]);
  const counts = useMemo(() => filterCounts(data, def.filters?.length ?? 0), [data, def.filters]);
  const template = useMemo(() => gridColumns(columnWidths(data, columns)), [data, columns]);
  const toggle = (i: number) => setOn((xs) => (xs.includes(i) ? xs.filter((x) => x !== i) : [...xs, i]));
  const onSort = (i: number) => setSort(nextSort(sorting, i, columns[i].numeric));
  return { c, columns, sorting, on, search, setSearch, order, counts, template, toggle, onSort };
}

/** A dense table: a sticky header that sorts, quick filters, a search over its text, rows drawn only where they show. */
export default function BoardPanel({ id, def, data }: { id: string; def: BoardDef; data: BoardData }) {
  const zone = useZone();
  const b = useBoard(def, data, zone);
  const size = useSize<HTMLDivElement>();
  const scroll = useScroll();
  const win = windowOf(scroll.top, size.height || 600, ROW, b.order.length);
  const total = data.total ?? data.rows.length;
  const meta = b.order.length === total ? total.toLocaleString('en-US') : `${b.order.length.toLocaleString('en-US')} of ${total.toLocaleString('en-US')}`;
  const tools = (
    <>
      <Filters def={def} counts={b.counts} on={b.on} toggle={b.toggle} />
      <input type="search" value={b.search} onChange={(e) => b.setSearch(e.target.value)} placeholder="Search" aria-label={`Search ${def.title ?? id}`}
        className="h-6 w-32 shrink-0 rounded-[3px] border border-line bg-raised px-1.5 font-mono text-[11px] text-fg placeholder:font-sans placeholder:text-faint" />
    </>
  );
  return (
    <PanelFrame id={id} title={def.title} meta={meta} tools={tools}>
      <div ref={size.ref} onScroll={scroll.onScroll} className="absolute inset-0 overflow-auto">
        <div className="w-max min-w-full">
          <Header columns={b.columns} template={b.template} sort={b.sorting} onSort={b.onSort} />
          <div style={{ paddingTop: win.before, paddingBottom: win.after }}>
            {b.order.slice(win.start, win.end).map((ri) => <Row key={ri} c={b.c} row={data.rows[ri]} columns={b.columns} template={b.template} zone={zone} />)}
          </div>
          {!b.order.length && <PanelEmpty>{data.rows.length ? 'No rows match.' : 'No rows.'}</PanelEmpty>}
        </div>
      </div>
    </PanelFrame>
  );
}
