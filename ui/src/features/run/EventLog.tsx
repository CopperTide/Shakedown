import { useEffect, useRef, useState } from 'react';
import { StatusMark } from '@/components/ui/status';
import { Table, Td, Th } from '@/components/ui/table';
import { cn } from '@/lib/cn';
import { alarmMark } from '@/lib/mark';
import { type Span, formatDuration } from '@/model/lanes';
import type { LogRow } from './logFilter';

/** A remembered view choice (this browser only; the default when storage is unavailable). */
function usePref(key: string, initial: boolean): [boolean, (v: boolean) => void] {
  const [v, setV] = useState(() => { try { const s = localStorage.getItem(key); return s === null ? initial : s === '1'; } catch { return initial; } });
  return [v, (next) => { setV(next); try { localStorage.setItem(key, next ? '1' : '0'); } catch { /* storage unavailable */ } }];
}

const offset = (at: number, span: Span | null) => (span && !Number.isNaN(at) ? `+${((at - span.start) / 1000).toFixed(1)} s` : '–');

/**
 * Every step as a row (only those in `matches` when filtered): the record behind the timeline, in step order.
 * The detail comes last and takes the rest of the width, so the short columns always show and the text wraps in full.
 */
export default function EventLog({ rows: all, matches, span, selected, onSelect }: {
  rows: LogRow[]; matches: ReadonlySet<number> | null; span: Span | null; selected: number | null; onSelect: (seq: number) => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  // Full text by default: each step's whole detail, wrapped. Compact keeps one line per step.
  const [full, setFull] = usePref('shakedown.eventlog.full', true);
  const [tall, setTall] = usePref('shakedown.eventlog.tall', false);
  const rows = matches ? all.filter((r) => matches.has(r.m.seq)) : all;
  // The short cells: one line each, level with the detail's first line when it wraps.
  const cell = full ? 'py-1.5' : 'h-7';
  useEffect(() => { box.current?.querySelector('[data-active]')?.scrollIntoView({ block: 'nearest' }); }, [selected]);
  return (
    <div>
    <div className="flex items-center justify-end gap-3 px-3 pb-1 text-2xs text-muted">
      <button type="button" className="hover:text-fg" aria-pressed={full} onClick={() => setFull(!full)}>{full ? 'Compact rows' : 'Full text'}</button>
      <button type="button" className="hover:text-fg" aria-pressed={tall} onClick={() => setTall(!tall)}>{tall ? 'Shorter' : 'Taller'}</button>
    </div>
    <div ref={box} className={cn('overflow-auto', tall ? 'max-h-[80vh]' : 'max-h-[420px]')}>
      <Table className="text-xs" wrapperClassName="overflow-visible">
        <thead className="sticky top-0 z-10"><tr><Th>Step</Th><Th>Time</Th><Th>Agent</Th><Th>Action</Th><Th className="text-right">Took</Th><Th title="The world was saved after this step">Saved</Th><Th>Outcome</Th><Th className="w-full">Detail</Th></tr></thead>
        <tbody>{rows.map(({ lane, m }) => (
          <tr key={m.seq} data-active={selected === m.seq || undefined} onClick={() => onSelect(m.seq)}
            className={cn('cursor-pointer hover:bg-hover data-[active]:bg-accent-soft', full && '[&>td]:align-top', m.inherited && 'opacity-50')}>
            <Td className={cn(cell, 'font-mono')}>{m.seq}</Td><Td className={cn(cell, 'font-mono text-muted')}>{offset(m.at, span)}</Td>
            <Td className={cn(cell, full && 'whitespace-normal')}>{lane}</Td>
            <Td className={cn(cell, full && 'whitespace-normal', m.effect === 'write' && 'font-semibold')}>{m.label}</Td>
            <Td className={cn(cell, 'text-right font-mono text-muted')}>{formatDuration(m.durationMs)}</Td>
            <Td className={cn(cell, 'text-center text-muted')}>{m.saved && <span title="saved after this step">◉</span>}</Td>
            <Td className={cell}>{m.outcome === 'ok' ? <span className="text-faint">ok</span> : <StatusMark mark={alarmMark(m.outcome)} />}</Td>
            <Td className={cn('min-w-[24ch] font-mono text-2xs text-muted', full ? 'py-1.5 whitespace-pre-wrap wrap-anywhere' : 'h-7 max-w-0 truncate')} title={full ? undefined : m.detail}>{m.detail}</Td>
          </tr>
        ))}</tbody>
      </Table>
      {rows.length === 0 && <p className="px-3 py-4 text-xs text-muted">No step matches the filter.</p>}
    </div>
    </div>
  );
}
