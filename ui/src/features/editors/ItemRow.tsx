// One item in a list: a link to its page with its id and problems, what comes before it (a toggle
// in a tree) and its actions after it.
import { AlertTriangle } from 'lucide-react';
import type { ComponentProps, ReactNode } from 'react';
import { Link } from 'react-router-dom';
import { cn } from '@/lib/cn';
import type { ItemSummary } from '@/model/project';

function ProblemBadge({ item }: { item: ItemSummary }) {
  const n = item.problems.length;
  if (!n) return null;
  return (
    <span title={item.problems[0].message} className="inline-flex items-center gap-1 rounded-full bg-warn-soft px-2 py-0.5 text-2xs font-medium text-warn">
      <AlertTriangle className="size-3" aria-hidden="true" />{n} problem{n > 1 ? 's' : ''}
    </span>
  );
}

export default function ItemRow({ item, to, depth = 0, lead, actions, className, ...li }: {
  item: ItemSummary;
  to: string;
  /** How deep in a tree: each level indents the row. */
  depth?: number;
  lead?: ReactNode;
  actions?: ReactNode;
} & ComponentProps<'li'>) {
  return (
    <li className={cn('flex items-center gap-1.5 pr-3 transition-colors hover:bg-hover', className)} style={{ paddingLeft: 18 + depth * 24 }} {...li}>
      {lead}
      <Link to={to} className="flex min-w-0 flex-1 items-center gap-3 py-3 text-sm focus-visible:underline focus-visible:outline-none">
        <span className="truncate font-medium">{item.label}</span>
        <span className="font-mono text-2xs text-faint">{item.id}</span>
        <ProblemBadge item={item} />
      </Link>
      {actions}
    </li>
  );
}
