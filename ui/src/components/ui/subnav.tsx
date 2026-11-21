import type { ReactNode } from 'react';
import { cn } from '@/lib/cn';

export interface SubNavItem {
  id: string;
  label: string;
  icon?: ReactNode;
  /** A dot after the label: ok, needs attention, or new. */
  status?: 'ok' | 'warn' | 'new';
}

const DOT = { ok: 'bg-ok', warn: 'bg-warn', new: 'bg-copper' } as const;

/** A settings page's section list: one section on screen at a time, the active one marked in copper. */
export function SubNav({ items, active, onSelect, title }: {
  items: SubNavItem[];
  active: string;
  onSelect: (id: string) => void;
  title?: string;
}) {
  return (
    <nav className="sticky top-[72px] flex flex-col gap-0.5" aria-label={title ?? 'Sections'}>
      {title && <div className="px-2.5 pb-2 font-mono text-2xs uppercase tracking-wider text-faint">{title}</div>}
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          aria-current={item.id === active ? 'page' : undefined}
          onClick={() => onSelect(item.id)}
          className={cn(
            'relative flex h-[30px] items-center gap-2.5 rounded-md px-2.5 text-left text-sm font-medium text-muted hover:bg-hover hover:text-fg',
            item.id === active && 'bg-hover text-fg before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded before:bg-copper',
          )}
        >
          {item.icon}
          <span className="flex-1">{item.label}</span>
          {item.status && <span className={cn('size-1.5 rounded-full', DOT[item.status])} />}
        </button>
      ))}
    </nav>
  );
}
