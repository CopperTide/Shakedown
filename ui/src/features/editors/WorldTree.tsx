// The worlds list: worlds under their group headers (drag a world onto a header to move it there),
// each world opening onto its scenarios and each scenario onto its experiments.
import { ChevronRight } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { toast } from 'sonner';
import { getItem, putItem } from '@/api/project';
import { Button } from '@/components/ui/button';
import { projectChanged } from '@/hooks/useProjectItems';
import { cn } from '@/lib/cn';
import { errorMessage } from '@/lib/errors';
import type { ItemKind, ItemSummary, WorldBody } from '@/model/project';
import DeleteItemDialog from './DeleteItemDialog';
import ItemRow from './ItemRow';
import RenameGroupDialog from './RenameGroupDialog';
import { type WorldGroup, experimentsUsing, groupNames, groupWorlds, scenariosIn, withGroup } from './tree';

/** Moves world `id` into `group` (null: out of any group). */
async function moveWorld(id: string, group: string | null) {
  const { body } = await getItem<WorldBody>('worlds', id);
  await putItem('worlds', id, withGroup(body, group), true);
}

function Expander({ open, label, onToggle }: { open: boolean; label: string; onToggle: () => void }) {
  return (
    <Button variant="ghost" size="sm" className="size-6 px-0" aria-expanded={open} aria-label={label} onClick={onToggle}>
      <ChevronRight className={cn('transition-transform', open && 'rotate-90')} />
    </Button>
  );
}

const Spacer = () => <span className="size-6 shrink-0" aria-hidden="true" />;

const remove = (kind: ItemKind, item: ItemSummary) => <DeleteItemDialog compact kind={kind} id={item.id} label={item.label} onDeleted={() => {}} />;

function Empty({ depth, children }: { depth: number; children: ReactNode }) {
  return <li className="py-2 pr-3 text-xs text-faint" style={{ paddingLeft: 18 + depth * 24 + 30 }}>{children}</li>;
}

function ScenarioRows({ scenario, experiments }: { scenario: ItemSummary; experiments: ItemSummary[] }) {
  const [open, setOpen] = useState(false);
  const mine = experimentsUsing(experiments, scenario.id);
  return (
    <>
      <ItemRow item={scenario} to={`/scenarios/${scenario.id}`} depth={1} actions={remove('scenarios', scenario)}
        lead={<Expander open={open} label={`Show experiments using ${scenario.label}`} onToggle={() => setOpen(!open)} />} />
      {open && mine.map((e) => <ItemRow key={e.id} item={e} to={`/experiments/file/${e.id}`} depth={2} lead={<Spacer />} actions={remove('experiments', e)} />)}
      {open && mine.length === 0 && <Empty depth={2}>No experiments yet.</Empty>}
    </>
  );
}

function WorldRows({ world, scenarios, experiments, onDrag }: { world: ItemSummary; scenarios: ItemSummary[]; experiments: ItemSummary[]; onDrag: (id: string | null) => void }) {
  const [open, setOpen] = useState(false);
  const mine = scenariosIn(scenarios, world.id);
  return (
    <>
      <ItemRow item={world} to={`/worlds/${world.id}`} actions={remove('worlds', world)} draggable className="cursor-grab"
        onDragStart={(e) => { e.dataTransfer?.setData('text/plain', world.id); onDrag(world.id); }} onDragEnd={() => onDrag(null)}
        lead={<Expander open={open} label={`Show scenarios in ${world.label}`} onToggle={() => setOpen(!open)} />} />
      {open && mine.map((s) => <ScenarioRows key={s.id} scenario={s} experiments={experiments} />)}
      {open && mine.length === 0 && <Empty depth={1}>No scenarios yet.</Empty>}
    </>
  );
}

/** A group's header, where a dragged world is dropped to move it into the group. */
function GroupHeader({ group, groups, onDrop }: { group: WorldGroup; groups: string[]; onDrop: () => void }) {
  const [over, setOver] = useState(false);
  const name = group.name ?? 'Ungrouped';
  return (
    <div data-group={name} className={cn('flex items-center gap-2 border-b border-line bg-sunken px-[18px] py-2 transition-colors', over && 'bg-accent-soft')}
      onDragOver={(e) => { e.preventDefault(); setOver(true); }} onDragLeave={() => setOver(false)}
      onDrop={(e) => { e.preventDefault(); setOver(false); onDrop(); }}>
      <h3 className="font-display text-lg leading-tight">{name}</h3>
      <span className="text-2xs text-faint">{group.worlds.length} world{group.worlds.length === 1 ? '' : 's'}</span>
      {group.name && <RenameGroupDialog name={group.name} count={group.worlds.length} groups={groups} />}
    </div>
  );
}

export default function WorldTree({ worlds, scenarios, experiments }: { worlds: ItemSummary[]; scenarios: ItemSummary[]; experiments: ItemSummary[] }) {
  const [dragging, setDragging] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const drop = (group: string | null) => {
    const world = worlds.find((w) => w.id === dragging);
    setDragging(null);
    if (!world || (world.group ?? null) === group) return;
    const back = world.group ?? null;
    const undo = () => { moveWorld(world.id, back).then(projectChanged, (e: unknown) => toast.error(errorMessage(e))); };
    moveWorld(world.id, group).then(() => {
      setError(null);
      projectChanged();
      toast(`Moved ${world.label} to ${group ?? 'Ungrouped'}`, { action: { label: 'Undo', onClick: undo } });
    }, (e: unknown) => setError(errorMessage(e)));
  };
  const groups = groupNames(worlds);
  return (
    <div className="divide-y divide-line">
      {error && <p role="alert" className="px-[18px] py-2 text-sm text-alarm">{error}</p>}
      {groupWorlds(worlds).map((g) => (
        <section key={g.name ?? ''}>
          <GroupHeader group={g} groups={groups} onDrop={() => drop(g.name)} />
          <ul className="divide-y divide-line">
            {g.worlds.map((w) => <WorldRows key={w.id} world={w} scenarios={scenarios} experiments={experiments} onDrag={setDragging} />)}
          </ul>
        </section>
      ))}
    </div>
  );
}
