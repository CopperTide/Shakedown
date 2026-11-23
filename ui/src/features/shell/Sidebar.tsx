// The sidebar: the mark, the project, the sections, and the footer links.
import { Link, useLocation } from 'react-router-dom';
import { Mark } from '@/components/brand/Mark';
import { useProject } from '@/hooks/useProject';
import { cn } from '@/lib/cn';
import { NavIcon } from './icons';
import WorldNav from './WorldNav';
import { FOOTER, NAV, type NavItem, isActive } from './nav';

function NavLink({ item, pathname }: { item: NavItem; pathname: string }) {
  const on = isActive(item, pathname);
  return (
    <Link
      to={item.to}
      aria-current={on ? 'page' : undefined}
      className={cn(
        'relative flex h-[30px] items-center gap-2.5 rounded-md px-2 text-sm font-medium text-muted hover:bg-hover hover:text-fg',
        on && 'bg-surface text-fg shadow-panel before:absolute before:-left-3 before:inset-y-1.5 before:w-0.5 before:rounded before:bg-copper',
      )}
    >
      <NavIcon name={item.icon} />
      <span>{item.label}</span>
    </Link>
  );
}

function ProjectName() {
  const { data } = useProject();
  const name = data?.open ? data.status.name : 'No project open';
  return (
    <div className="mx-0.5 mb-3 flex items-center gap-2 rounded-lg border border-line bg-surface px-2.5 py-2">
      <span className="grid size-6 place-items-center rounded-md bg-sunken font-mono text-2xs font-medium text-muted">{name.slice(0, 1).toUpperCase()}</span>
      <span className="min-w-0 flex-1 truncate text-sm font-medium text-fg">{name}</span>
    </div>
  );
}

export default function Sidebar() {
  const { pathname } = useLocation();
  return (
    <aside className="sticky top-0 flex h-screen w-60 shrink-0 flex-col overflow-y-auto border-r border-line bg-sunken p-3" aria-label="Sidebar">
      <Link to="/" className="mb-3 flex items-center gap-2 px-1.5 py-1 font-display text-lg text-fg"><Mark />Shakedown</Link>
      <ProjectName />
      <nav className="flex flex-col gap-0.5" aria-label="Main">
        {NAV.map((group, i) => (
          <div key={group.label ?? i} className="flex flex-col gap-0.5">
            {group.label && <div className="mt-4 px-2 pb-1 text-2xs font-medium text-faint">{group.label}</div>}
            {group.items.map((item) => (
              <div key={item.to} className="flex flex-col gap-0.5">
                <NavLink item={item} pathname={pathname} />
                {item.sub === 'worlds' && <WorldNav pathname={pathname} />}
              </div>
            ))}
          </div>
        ))}
      </nav>
      <span className="flex-1" />
      <nav className="flex flex-col gap-0.5 border-t border-line pt-3" aria-label="More">
        {FOOTER.map((item) => <NavLink key={item.to} item={item} pathname={pathname} />)}
      </nav>
      <p className="mt-3 px-2 text-2xs text-faint">Shakedown by CopperTide</p>
    </aside>
  );
}
