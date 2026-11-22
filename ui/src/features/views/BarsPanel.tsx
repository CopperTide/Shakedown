import { useMemo } from 'react';
import { cn } from '@/lib/cn';
import { DASH, formatValue } from './format';
import { PanelEmpty } from './PanelFrame';
import { cols } from './rows';
import { Chip, TONE_BG, TONE_TEXT } from './tones';
import { type BarsData, type BarsDef, asTone } from './types';

/** Where the limit sits on every bar's track: a value at its limit fills this much. */
export const LIMIT_AT = 0.8;

/** How much of the track a ratio fills, 0–1. */
export const fillOf = (ratio: number | null) => (ratio == null || ratio < 0 ? 0 : Math.min(1, ratio * LIMIT_AT));

/** A bar per row: its value against its limit (the limit marked at the same place on every track), the ratio in numerals. */
export default function BarsPanel({ def, data }: { def: BarsDef; data: BarsData }) {
  const c = useMemo(() => cols(data.columns), [data.columns]);
  const fmt = (v: unknown) => {
    const s = formatValue(v, def.format ?? 'number', def.decimals);
    return s === DASH || !def.unit ? s : `${s} ${def.unit}`;
  };
  if (!data.rows.length) return <PanelEmpty>Nothing to show.</PanelEmpty>;
  return (
    <div className="absolute inset-0 overflow-y-auto py-1">
      {data.rows.map((row, i) => {
        const tone = asTone(c.get(row, 'tone'));
        const ratio = c.num(row, 'ratio');
        const chip = c.str(row, 'chip');
        return (
          <div key={`${c.str(row, 'key')}-${i}`} className="grid h-[22px] grid-cols-[minmax(72px,1fr)_minmax(80px,1.6fr)_auto_44px] items-center gap-2.5 px-3 hover:bg-hover">
            <span className="flex min-w-0 items-center gap-1.5 font-mono text-[11.5px]" title={c.str(row, 'label')}>
              <span className="truncate">{c.str(row, 'label') || c.str(row, 'key')}</span>
              {chip && <Chip tone={tone}>{chip}</Chip>}
            </span>
            <span className="relative h-[7px] bg-sunken">
              <span className={cn('absolute inset-y-0 left-0', tone === 'neutral' ? 'bg-tone-neutral/60' : TONE_BG[tone])} style={{ width: `${fillOf(ratio) * 100}%` }} />
              <span className="absolute -top-[3px] -bottom-[3px] w-px bg-fg" style={{ left: `${LIMIT_AT * 100}%` }} />
            </span>
            <span className="font-mono text-[11px] whitespace-nowrap text-muted tabular-nums">{fmt(c.get(row, 'value'))}<span className="text-faint"> / {fmt(c.get(row, 'limit'))}</span></span>
            <span className={cn('text-right font-mono text-[11.5px] font-medium tabular-nums', TONE_TEXT[tone])}>{formatValue(ratio, 'ratio_pct')}</span>
          </div>
        );
      })}
    </div>
  );
}
