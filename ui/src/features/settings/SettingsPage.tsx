// Settings: Models, Observability and API tokens, side by side as pages under one header.
import type { ReactNode } from 'react';
import { Link, useLocation } from 'react-router-dom';
import { PageBody, PageHeader } from '@/components/ui/page';
import { cn } from '@/lib/cn';

const TABS = [
  { to: '/settings/models', label: 'Models' },
  { to: '/settings/observability', label: 'Observability' },
  { to: '/settings/tokens', label: 'API tokens' },
];

export default function SettingsPage({ children }: { children: ReactNode }) {
  const { pathname } = useLocation();
  return (
    <>
      <PageHeader title="Settings" width="default">
        <nav className="flex gap-1" aria-label="Settings">
          {TABS.map((t) => (
            <Link key={t.to} to={t.to} aria-current={pathname === t.to ? 'page' : undefined}
              className={cn('rounded-md px-2.5 py-1 text-sm', pathname === t.to ? 'bg-sunken font-medium text-fg' : 'text-muted hover:text-fg')}>
              {t.label}
            </Link>
          ))}
        </nav>
      </PageHeader>
      <PageBody width="default" className="flex flex-col gap-6">{children}</PageBody>
    </>
  );
}
