// The moments a run needed a person: shown above the timeline when there are any.
import { UserRound } from 'lucide-react';
import type { Event } from '@/model/types';
import { needsAHuman } from './humans';

export default function NeedsAHuman({ events, onSelect }: { events: Event[]; onSelect: (seq: number) => void }) {
  const moments = needsAHuman(events);
  if (moments.length === 0) return null;
  return (
    <section aria-label="Needs a human" className="rounded-lg border border-warn bg-warn-soft">
      <header className="flex items-center gap-2 px-4 py-2 text-sm font-semibold text-warn"><UserRound className="size-4" />Needs a human · {moments.length}</header>
      <ul className="divide-y divide-line border-t border-line bg-surface">
        {moments.map((m) => (
          <li key={m.seq}>
            <button type="button" onClick={() => onSelect(m.seq)} className="flex w-full items-center gap-3 px-4 py-2 text-left text-sm hover:bg-hover">
              <span className="font-mono text-2xs text-faint">#{m.seq}</span>
              <span className="text-xs text-muted">{m.kind === 'approval' ? 'Approval' : 'Message'}</span>
              <span className="min-w-0 flex-1 truncate">{m.text}</span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
