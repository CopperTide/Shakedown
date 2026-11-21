import { ChevronRight } from 'lucide-react';
import { Link } from 'react-router-dom';
import { Card } from '@/components/ui/card';
import type { ProjectStatus } from '@/model/project';

const ROWS = [
  { to: '/worlds', label: 'Worlds', key: 'worlds' },
  { to: '/scenarios', label: 'Scenarios', key: 'scenarios' },
  { to: '/setups', label: 'Agent setups', key: 'setups' },
  { to: '/experiments', label: 'Experiments', key: 'experiments' },
] as const;

/** The project's contents, each a link to its section, and how many files have problems. */
export default function ProjectCard({ status }: { status: ProjectStatus }) {
  const problems = status.problems.length;
  return (
    <Card title={status.name} description={problems ? `${problems} problem${problems > 1 ? 's' : ''} in the project's files` : 'Personal project'}>
      <ul className="divide-y divide-line">
        {ROWS.map((r) => (
          <li key={r.key}>
            <Link to={r.to} className="flex items-center gap-3 px-[18px] py-3 text-sm hover:bg-hover">
              <span className="flex-1 font-medium">{r.label}</span>
              <span className="tabular-nums text-faint">{status.counts[r.key] ?? 0}</span>
              <ChevronRight className="size-4 text-faint" aria-hidden="true" />
            </Link>
          </li>
        ))}
      </ul>
    </Card>
  );
}
