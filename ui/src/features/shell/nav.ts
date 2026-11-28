// The sidebar's sections as data: what the app offers and where each lives.

export interface NavItem {
  to: string;
  label: string;
  icon: 'home' | 'flask' | 'globe' | 'film' | 'users' | 'gear' | 'rocket';
  /** Which addresses belong to it (default: the address and everything under it). */
  match?: RegExp;
}

export interface NavGroup {
  label?: string;
  items: NavItem[];
}

/** The core of the product. Everything else (runtimes, telemetry, tokens, connecting an agent) is under Settings. */
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
      { to: '/worlds', label: 'Worlds', icon: 'globe' },
      { to: '/scenarios', label: 'Scenarios', icon: 'film' },
      { to: '/setups', label: 'Agent setups', icon: 'users', match: /^\/(setups|team)(\/|$)/ },
    ],
  },
];

export const FOOTER: NavItem[] = [
  { to: '/get-started', label: 'Get started', icon: 'rocket' },
  { to: '/settings/models', label: 'Settings', icon: 'gear', match: /^\/(settings|runtimes|connect)(\/|$)/ },
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
