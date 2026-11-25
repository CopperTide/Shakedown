// A world at a glance: its group, each section's state a click away, and the scenarios in it.
import { Card } from '@/components/ui/card';
import type { ItemSummary, WorldBody } from '@/model/project';
import { setIn } from '../edit';
import { SectionHead } from '../fields';
import RelatedItems from '../RelatedItems';
import GroupField from './GroupField';

function Summary({ title, text, onOpen }: { title: string; text: string; onOpen: () => void }) {
  return (
    <button type="button" onClick={onOpen} className="text-left">
      <Card className="h-full p-[18px] hover:border-line-strong">
        <h3 className="text-sm font-semibold">{title}</h3>
        <p className="mt-1 text-sm text-muted">{text}</p>
      </Card>
    </button>
  );
}

const count = (n: number | undefined, one: string, many: string) => (n ? `${n} ${n === 1 ? one : many}` : `No ${many} yet`);

export default function WorldOverview({ world, onOpen, onChange, groups, scenarios }: {
  world: WorldBody;
  onOpen: (section: string) => void;
  onChange: (w: WorldBody) => void;
  /** The groups the project's worlds are in. */
  groups: string[];
  /** The scenarios in this world. */
  scenarios: ItemSummary[];
}) {
  return (
    <div className="grid gap-4">
      <SectionHead title="Overview" about="The production systems as they are before any agent. Open a section to change it." />
      <GroupField value={world.group} groups={groups} onChange={(g) => onChange(setIn(world, ['group'], g))} />
      <div className="grid gap-3 md:grid-cols-2">
        <Summary title="Connectors" text={count(world.connectors?.length, 'connector', 'connectors')} onOpen={() => onOpen('connectors')} />
        <Summary title="Software flows" text={count(world.flows?.length, 'flow', 'flows')} onOpen={() => onOpen('flows')} />
        <Summary title="Data" text={count(world.data?.length, 'source', 'sources')} onOpen={() => onOpen('data')} />
        <Summary title="People" text={count(world.people?.length, 'person', 'people')} onOpen={() => onOpen('people')} />
        <Summary title="Runtime" text={world.package ? `Runs on ${world.package}` : 'No runtime yet'} onOpen={() => onOpen('runtime')} />
      </div>
      <RelatedItems title="Scenarios in this world" noun="scenario" items={scenarios} to={(id) => `/scenarios/${id}`} empty="No scenarios in this world yet." />
    </div>
  );
}
