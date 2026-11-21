import { XCircle } from 'lucide-react';
import { Link } from 'react-router-dom';
import { Card } from '@/components/ui/card';
import { EmptyState } from '@/components/ui/state';
import type { Branch } from '@/model/types';

/** Failed runs, newest first, each a link to its run. */
export default function NeedsAttention({ branches }: { branches: Branch[] }) {
  return (
    <Card title="Needs attention" description="Failed runs, newest first">
      {branches.length === 0 ? (
        <EmptyState title="Nothing needs you right now." />
      ) : (
        <ul className="divide-y divide-line">
          {branches.map((b) => (
            <li key={b.id}>
              <Link to={`/runs/${b.run_id}/${b.id}`} className="flex items-center gap-3 px-[18px] py-3 hover:bg-hover">
                <XCircle className="size-4 text-alarm" aria-hidden="true" />
                <span className="min-w-0 flex-1 truncate text-sm font-medium">{b.error ?? 'Run failed'}</span>
                <span className="font-mono text-2xs text-faint">{new Date(b.created_at).toLocaleString()}</span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
