// Settings: Connections & models, Project settings and Advanced (runtimes, telemetry, API tokens,
// connecting an agent), as pages under one header.
import type { ReactNode } from 'react';
import { Link, useLocation } from 'react-router-dom';
import { PageBody, PageHeader } from '@/components/ui/page';
import { cn } from '@/lib/cn';

/** The tabs; Advanced also covers the pages it lists that live under /settings. */
const TABS = [
  { to: '/settings/models', label: 'Connections & models', match: /^\/settings\/models$/ },
  { to: '/settings/project', label: 'Project settings', match: /^\/settings\/project$/ },
  { to: '/settings/advanced', label: 'Advanced', match: /^\/settings\/(advanced|observability|tokens)$/ },
];

/** The Settings tabs, the current one marked. */
export function SettingsTabs() {
  const { pathname } = useLocation();
  return (
    <nav className="flex flex-wrap gap-1" aria-label="Settings">
      {TABS.map((t) => {
        const on = t.match.test(pathname);
        return (
          <Link key={t.to} to={t.to} aria-current={on ? 'page' : undefined}
            className={cn('rounded-md px-2.5 py-1 text-sm', on ? 'bg-sunken font-medium text-fg' : 'text-muted hover:text-fg')}>
            {t.label}
          </Link>
        );
      })}
    </nav>
  );
}

export default function SettingsPage({ children }: { children: ReactNode }) {
  return (
    <>
      <PageHeader title="Settings" width="default"><SettingsTabs /></PageHeader>
      <PageBody width="default" className="flex flex-col gap-6">{children}</PageBody>
    </>
  );
}
