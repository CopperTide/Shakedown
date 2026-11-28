import { FlaskConical, Film, Globe, Home, Rocket, Settings, Users } from 'lucide-react';
import type { NavItem } from './nav';

const ICONS = { home: Home, flask: FlaskConical, globe: Globe, film: Film, users: Users, gear: Settings, rocket: Rocket };

export function NavIcon({ name }: { name: NavItem['icon'] }) {
  const Icon = ICONS[name];
  return <Icon className="size-4 shrink-0" aria-hidden="true" />;
}
