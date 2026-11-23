// The sidebar's worlds under their groups (groups A to Z, the ungrouped last), each a link to its
// page. Only the group holding the world on screen is open; a group's name opens or closes it.
import { ChevronRight } from 'lucide-react';
import { useState } from 'react';
import { Link } from 'react-router-dom';
import { type WorldGroup, groupWorlds } from '@/features/editors/tree';
import { useProjectItems } from '@/hooks/useProjectItems';
import { cn } from '@/lib/cn';

function Worlds({ group, pathname }: { group: WorldGroup; pathname: string }) {
  return (
    <ul>
      {group.worlds.map((w) => {
        const to = `/worlds/${w.id}`;
        const on = pathname === to;
        return (
          <li key={w.id}>
            <Link to={to} aria-current={on ? 'page' : undefined}
              className={cn('block truncate rounded-md px-2 py-0.5 text-xs text-muted hover:bg-hover hover:text-fg', on && 'bg-surface font-medium text-fg')}>{w.label}</Link>
          </li>
        );
      })}
    </ul>
  );
}

export default function WorldNav({ pathname }: { pathname: string }) {
  const groups = groupWorlds(useProjectItems('worlds').data ?? []);
  const [toggled, setToggled] = useState<Record<string, boolean>>({});
  if (groups.length === 0) return null;
  const here = groups.find((g) => g.worlds.some((w) => pathname === `/worlds/${w.id}`));
  const isOpen = (key: string, g: WorldGroup) => toggled[key] ?? (g === here || groups.length === 1);
  return (
    <ul aria-label="Worlds by group" className="ml-[17px] flex flex-col gap-0.5 border-l border-line pb-1 pl-2">
      {groups.map((g) => {
        const key = g.name ?? '';
        const open = isOpen(key, g);
        return (
          <li key={key}>
            <button type="button" aria-expanded={open} onClick={() => setToggled({ ...toggled, [key]: !open })}
              className="flex w-full items-center gap-1 truncate rounded-md px-2 pt-1 text-left text-2xs font-medium text-faint hover:text-fg">
              <ChevronRight className={cn('size-3 shrink-0 transition-transform', open && 'rotate-90')} aria-hidden="true" />{g.name ?? 'Ungrouped'}
            </button>
            {open && <Worlds group={g} pathname={pathname} />}
          </li>
        );
      })}
    </ul>
  );
}
