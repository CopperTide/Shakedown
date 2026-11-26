// The pieces the Analytics overview's cards share: a card with a serif title, a legend, a hover
// tooltip, and the percentage and money words.
import type { ReactNode } from 'react';
import { Panel } from '@/components/ui/panel';
import { cn } from '@/lib/cn';
import { fmt } from '@/lib/format';

/** A card: its serif title and a line on what it shows, actions on the right, then its body. */
export function Card({ title, hint, actions, children, className }: { title: string; hint?: ReactNode; actions?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <Panel className={cn('flex flex-col', className)}>
      <header className="flex flex-wrap items-start gap-x-3 gap-y-2 px-4 pt-3">
        <div className="min-w-0 flex-1">
          <h2 className="font-display text-xl leading-tight">{title}</h2>
          {hint && <p className="mt-0.5 text-xs text-muted">{hint}</p>}
        </div>
        {actions}
      </header>
      <div className="flex min-w-0 flex-1 flex-col px-4 pt-2 pb-3">{children}</div>
    </Panel>
  );
}

/** Each series' name beside its colour: identity never by colour alone. */
export function Legend({ items }: { items: { name: string; color: string }[] }) {
  return (
    <ul aria-label="Legend" className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted">
      {items.map((i) => (
        <li key={i.name} className="flex min-w-0 items-center gap-1.5">
          <span aria-hidden="true" className="size-2.5 shrink-0 rounded-full" style={{ background: i.color }} />
          <span className="max-w-64 truncate" title={i.name}>{i.name}</span>
        </li>
      ))}
    </ul>
  );
}

/** A tooltip over a chart, at `left` (and `top`) in the chart box's pixels, kept inside the box. */
export function Tip({ left, top = 8, children }: { left: number; top?: number; children: ReactNode }) {
  return (
    <div role="tooltip" className="pointer-events-none absolute z-10 w-max max-w-64 rounded-md border border-line bg-raised px-2.5 py-2 text-xs shadow-pop"
      style={{ left, top, transform: `translateX(${left > 160 ? 'calc(-100% - 12px)' : '12px'})` }}>
      {children}
    </div>
  );
}

/** "67%", "–" for none. */
export const pct = (x: number | null) => (x == null ? '–' : `${Math.round(x * 100)}%`);

/** "$0.56", "–" for none. */
export const money = (x: number | null) => fmt(x, 'usd');
