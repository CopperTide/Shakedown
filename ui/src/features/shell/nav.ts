// The sidebar's sections as data: what the app offers and where each lives.

export interface NavItem {
  to: string;
  label: string;
  icon: 'home' | 'flask' | 'globe' | 'film' | 'users' | 'plug' | 'gear' | 'rocket' | 'code' | 'book';
  /** Which addresses belong to it (default: the address and everything under it). */
  match?: RegExp;
  /** The project's items listed under it: its worlds, under their groups. */
  sub?: 'worlds';
}

export interface NavGroup {
  label?: string;
  items: NavItem[];
}

export const NAV: NavGroup[] = [
  {
    items: [
      { to: '/', label: 'Home', icon: 'home', match: /^\/$/ },
      { to: '/experiments', label: 'Experiments', icon: 'flask', match: /^\/(experiments|runs?|compare|control|world)(\/|$)/ },
    ],
  },
  {
    label: 'Build',
    items: [
      { to: '/worlds', label: 'Worlds', icon: 'globe', sub: 'worlds' },
      { to: '/scenarios', label: 'Scenarios', icon: 'film' },
      { to: '/setups', label: 'Agent setups', icon: 'users', match: /^\/(setups|team)(\/|$)/ },
    ],
  },
  {
    label: 'Settings',
    items: [
      { to: '/settings/models', label: 'Connections & models', icon: 'plug', match: /^\/settings\/(models|tokens)/ },
      { to: '/settings/project', label: 'Project settings', icon: 'gear' },
    ],
  },
];

export const FOOTER: NavItem[] = [
  { to: '/get-started', label: 'Get started', icon: 'rocket' },
  { to: '/connect', label: 'Connect an agent', icon: 'code' },
];

/** Whether `item` is the section `pathname` belongs to. */
export function isActive(item: NavItem, pathname: string): boolean {
  if (item.match) return item.match.test(pathname);
  return pathname === item.to || pathname.startsWith(`${item.to}/`);
}

/** The breadcrumb trail for `pathname`: the section, then the page when it is deeper. */
export function crumbsFor(pathname: string): string[] {
  for (const item of [...NAV.flatMap((g) => g.items), ...FOOTER]) {
    if (isActive(item, pathname)) return [item.label];
  }
  return [];
}
