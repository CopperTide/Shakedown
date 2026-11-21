// A kind's list: every item with its problems, each a link to its editor, and New.
import { AlertTriangle } from 'lucide-react';
import { Link, useNavigate } from 'react-router-dom';
import { listItems } from '@/api/project';
import { Card } from '@/components/ui/card';
import { PageBody, PageHeader } from '@/components/ui/page';
import { EmptyState, ErrorState } from '@/components/ui/state';
import { useQuery } from '@/hooks/useQuery';
import type { ItemKind, ItemSummary } from '@/model/project';
import { KINDS } from './kinds';
import NewItemDialog from './NewItemDialog';

function Row({ item, route }: { item: ItemSummary; route: string }) {
  const n = item.problems.length;
  return (
    <li className="flex items-center gap-3 px-[18px] py-3 text-sm">
      <Link to={`${route}/${item.id}`} className="font-medium hover:text-accent">{item.label}</Link>
      <span className="font-mono text-2xs text-faint">{item.id}</span>
      {n > 0 && (
        <span title={item.problems[0].message} className="inline-flex items-center gap-1 rounded-full bg-warn-soft px-2 py-0.5 text-2xs font-medium text-warn">
          <AlertTriangle className="size-3" aria-hidden="true" />{n} problem{n > 1 ? 's' : ''}
        </span>
      )}
    </li>
  );
}

export default function ItemListPage({ kind }: { kind: ItemKind }) {
  const info = KINDS[kind];
  const q = useQuery(`items:${kind}`, () => listItems(kind));
  const navigate = useNavigate();
  const items = q.data?.items ?? [];
  const create = <NewItemDialog kind={kind} taken={items.map((i) => i.id)} onCreated={(id) => navigate(`${info.route}/${id}`)} />;
  return (
    <>
      <PageHeader title={info.many} meta={info.about} actions={create} width="wide" />
      <PageBody width="wide">
        {q.error && <ErrorState error={q.error} onRetry={q.reload} />}
        <Card>
          {q.data && items.length === 0
            ? <EmptyState title={`No ${info.many.toLowerCase()} yet.`} action={create} />
            : <ul className="divide-y divide-line">{items.map((i) => <Row key={i.id} item={i} route={info.route} />)}</ul>}
        </Card>
      </PageBody>
    </>
  );
}
