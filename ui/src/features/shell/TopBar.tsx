// The top bar: where you are, search (⌘K), Ask (the assistant), and the account and tools menus.
import { Search, Sparkles } from 'lucide-react';
import { useLocation } from 'react-router-dom';
import { AccountMenu, useMe } from '@/app/AccountMenu';
import type { Mode } from '@/app/palette';
import { Button } from '@/components/ui/button';
import { Kbd } from '@/components/ui/kbd';
import { crumbsFor } from './nav';
import { ToolsMenu } from './ToolsMenu';

/** The window event the assistant drawer (plan 4) listens for. */
export const ASK_EVENT = 'shakedown:ask';

export default function TopBar({ onPalette }: { onPalette: (start?: Mode) => void }) {
  const { pathname } = useLocation();
  const me = useMe();
  const crumbs = crumbsFor(pathname);
  return (
    <header className="sticky top-0 z-30 flex h-12 shrink-0 items-center gap-3 border-b border-line bg-bg/90 px-7 backdrop-blur">
      <nav aria-label="Breadcrumb" className="flex min-w-0 flex-1 items-center gap-2 text-sm">
        {crumbs.map((c, i) => (
          <span key={c} className={i === crumbs.length - 1 ? 'font-medium text-fg' : 'text-faint'}>{c}</span>
        ))}
      </nav>
      <button type="button" onClick={() => onPalette()} className="hidden h-8 w-60 items-center gap-2 rounded-md border border-line bg-surface px-2.5 text-sm text-faint hover:border-line-strong sm:flex">
        <Search className="size-3.5" aria-hidden="true" /><span className="flex-1 text-left">Search…</span><Kbd>⌘K</Kbd>
      </button>
      <Button onClick={() => window.dispatchEvent(new CustomEvent(ASK_EVENT))} title="Ask the assistant (⌘J)">
        <Sparkles className="text-copper" />Ask
      </Button>
      <AccountMenu me={me} />
      <ToolsMenu onPalette={onPalette} />
    </header>
  );
}
