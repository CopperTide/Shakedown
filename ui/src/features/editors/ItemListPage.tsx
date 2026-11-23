// A kind's list: every item with its problems, each row a link to its editor with Delete beside
// it, and New. Worlds are listed under their groups, each opening onto its scenarios and experiments.
import { useNavigate } from 'react-router-dom';
import { Card } from '@/components/ui/card';
import { PageBody, PageHeader } from '@/components/ui/page';
import { EmptyState, ErrorState } from '@/components/ui/state';
import { useProjectItems } from '@/hooks/useProjectItems';
import type { ItemKind, ItemSummary } from '@/model/project';
import DeleteItemDialog from './DeleteItemDialog';
import ItemRow from './ItemRow';
import { KINDS } from './kinds';
import NewItemDialog from './NewItemDialog';
import WorldTree from './WorldTree';

function Rows({ kind, items }: { kind: ItemKind; items: ItemSummary[] }) {
  const route = KINDS[kind].route;
  return (
    <ul className="divide-y divide-line">
      {items.map((i) => (
        <ItemRow key={i.id} item={i} to={`${route}/${i.id}`}
          actions={<DeleteItemDialog compact kind={kind} id={i.id} label={i.label} onDeleted={() => {}} />} />
      ))}
    </ul>
  );
}

function Worlds({ worlds }: { worlds: ItemSummary[] }) {
  const scenarios = useProjectItems('scenarios').data ?? [];
  const experiments = useProjectItems('experiments').data ?? [];
  return <WorldTree worlds={worlds} scenarios={scenarios} experiments={experiments} />;
}

export default function ItemListPage({ kind }: { kind: ItemKind }) {
  const info = KINDS[kind];
  const q = useProjectItems(kind);
  const navigate = useNavigate();
  const items = q.data ?? [];
  const create = <NewItemDialog kind={kind} taken={items.map((i) => i.id)} onCreated={(id) => navigate(`${info.route}/${id}`)} />;
  const meta = kind === 'worlds' ? `${info.about} Drag a world onto a group to move it there.` : info.about;
  return (
    <>
      <PageHeader title={info.many} meta={meta} actions={create} width="wide" />
      <PageBody width="wide">
        {q.error && <ErrorState error={q.error} onRetry={q.reload} />}
        <Card className="overflow-hidden">
          {q.data && items.length === 0
            ? <EmptyState title={`No ${info.many.toLowerCase()} yet.`} action={create} />
            : kind === 'worlds' ? <Worlds worlds={items} /> : <Rows kind={kind} items={items} />}
        </Card>
      </PageBody>
    </>
  );
}
