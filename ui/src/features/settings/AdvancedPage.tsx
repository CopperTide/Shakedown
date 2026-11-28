// Settings → Advanced: what a first demo doesn't need, one link each.
import { ChevronRight } from 'lucide-react';
import { Link } from 'react-router-dom';
import { Card } from '@/components/ui/card';
import SettingsPage from './SettingsPage';

export const ADVANCED = [
  { to: '/runtimes', label: 'Runtimes', about: 'The packages that run worlds. A world picks its runtime in its Runtime section.' },
  { to: '/settings/observability', label: 'Observability', about: 'Send every run as a trace to Langfuse, Arize Phoenix or any OpenTelemetry collector.' },
  { to: '/settings/tokens', label: 'API tokens', about: 'Tokens for the SDK and scripts.' },
  { to: '/connect', label: 'Connect an agent', about: 'Test an agent you already run (HTTP, Bedrock or code) and use it in a setup.' },
];

export default function AdvancedPage() {
  return (
    <SettingsPage>
      <Card>
        <ul className="divide-y divide-line">
          {ADVANCED.map((a) => (
            <li key={a.to}>
              <Link to={a.to} className="flex items-center gap-3 px-[18px] py-3 hover:bg-hover">
                <span className="min-w-0 flex-1">
                  <span className="block text-sm font-medium text-fg">{a.label}</span>
                  <span className="block text-xs text-muted">{a.about}</span>
                </span>
                <ChevronRight className="size-4 text-faint" aria-hidden="true" />
              </Link>
            </li>
          ))}
        </ul>
      </Card>
    </SettingsPage>
  );
}
