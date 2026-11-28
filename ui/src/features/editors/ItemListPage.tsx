// A kind's list: every item with its problems, each row a link to its editor with Delete beside
// it, and New (worlds also Import from git, setups also Connect an agent). Worlds are listed under
// their groups, each opening onto its scenarios and experiments.
import { PlugZap } from 'lucide-react';
import type { ReactNode } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { PageBody, PageHeader } from '@/components/ui/page';
import { Hint } from '@/components/ui/hint';
import { EmptyState, ErrorState } from '@/components/ui/state';
import { useProjectItems } from '@/hooks/useProjectItems';
import type { ItemKind, ItemSummary } from '@/model/project';
import DeleteItemDialog from './DeleteItemDialog';
import ImportFromGit from './ImportFromGit';
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

/** Beside New: worlds import from git; setups can use an agent you already run. */
function Actions({ kind, create, onImported }: { kind: ItemKind; create: ReactNode; onImported: () => void }) {
  if (kind === 'worlds') return <div className="flex gap-2"><ImportFromGit onImported={onImported} />{create}</div>;
  if (kind === 'setups') return <div className="flex gap-2"><Button asChild><Link to="/connect"><PlugZap />Connect an agent</Link></Button>{create}</div>;
  return create;
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
  const hint = kind === 'worlds' ? `${info.about}\nDrag a world onto a group to move it there.` : info.about;
  return (
    <>
      <PageHeader title={<Hint hint={hint}>{info.many}</Hint>} actions={<Actions kind={kind} create={create} onImported={q.reload} />} width="wide" />
      <PageBody width="wide">
        {q.error && <ErrorState error={q.error} onRetry={q.reload} />}
        <Card className="overflow-hidden">
          {q.data && items.length === 0
            ? <EmptyState title={`No ${info.many.toLowerCase()} yet.`} hint={info.about} action={create} />
            : kind === 'worlds' ? <Worlds worlds={items} /> : <Rows kind={kind} items={items} />}
        </Card>
      </PageBody>
    </>
  );
}
