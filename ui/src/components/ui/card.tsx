import type { ReactNode } from 'react';
import { cn } from '@/lib/cn';
import { Hint } from './hint';

/**
 * The one surface for panels: an optional header (title, a short data line, actions), a body and a
 * footer. `description` is data shown under the title ("2 scenarios"); `hint` is explanation, shown
 * only on hovering (or tapping) the title.
 */
export function Card({ title, description, hint, actions, footer, children, className }: {
  title?: ReactNode;
  description?: ReactNode;
  hint?: ReactNode;
  actions?: ReactNode;
  footer?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn('min-w-0 rounded-lg border border-line bg-surface shadow-panel', className)}>
      {(title || actions) && (
        <header className="flex items-start gap-4 border-b border-line px-[18px] py-3.5">
          <div className="min-w-0 flex-1">
            {title && <h3 className="text-sm font-semibold text-fg"><Hint hint={hint}>{title}</Hint></h3>}
            {description && <p className="mt-0.5 text-xs text-faint">{description}</p>}
          </div>
          {actions}
        </header>
      )}
      {children}
      {footer && <footer className="flex items-center gap-2 rounded-b-lg border-t border-line bg-sunken px-[18px] py-2.5 text-xs text-muted">{footer}</footer>}
    </section>
  );
}
