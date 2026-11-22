import type { ReactNode } from 'react';
import { cn } from '@/lib/cn';

/** A ruled region of a screen: a title row (when there is a title or tools) over its body. */
export function PanelFrame({ id, title, meta, tools, children, className, bodyClass }: {
  id: string; title?: string; meta?: ReactNode; tools?: ReactNode; children: ReactNode; className?: string; bodyClass?: string;
}) {
  const head = title || meta != null || tools;
  return (
    <section data-panel={id} className={cn('flex min-h-0 min-w-0 flex-col bg-surface', className)} style={{ gridArea: id }}>
      {head && (
        <header className="flex h-8 shrink-0 items-center gap-3 border-b border-line px-3">
          {title && <h2 className="truncate font-display text-[17px] leading-none text-fg">{title}</h2>}
          {meta != null && <span className="shrink-0 font-mono text-[11px] text-faint tabular-nums">{meta}</span>}
          {tools && <div className="ml-auto flex min-w-0 items-center gap-1.5">{tools}</div>}
        </header>
      )}
      <div className={cn('relative min-h-0 flex-1', bodyClass)}>{children}</div>
    </section>
  );
}

/** A panel core couldn't evaluate: its error, in that panel only. */
export function PanelErrorBody({ error }: { error: string }) {
  return (
    <div className="flex h-full min-h-16 items-start gap-2 px-3 py-2.5 text-xs text-tone-critical">
      <span aria-hidden="true" className="font-mono">✕</span>
      <span className="min-w-0 font-mono break-words">{error}</span>
    </div>
  );
}

/** A panel with nothing to draw. */
export function PanelEmpty({ children }: { children: ReactNode }) {
  return <div className="px-3 py-2.5 text-xs text-faint">{children}</div>;
}
