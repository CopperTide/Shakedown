// Which installed benchmark or built world runs this world, until worlds run from their files.
import { worlds } from '@/api/worlds';
import { Card } from '@/components/ui/card';
import { useQuery } from '@/hooks/useQuery';
import type { WorldBody } from '@/model/project';
import { setIn } from '../edit';
import { ChoiceField, SectionHead } from '../fields';

export default function RuntimeSection({ world, onChange }: { world: WorldBody; onChange: (w: WorldBody) => void }) {
  const installed = useQuery('installed-worlds', worlds);
  const options = [{ value: '', label: 'None yet' }, ...(installed.data ?? []).map((w) => ({ value: w.name, label: w.title || w.name }))];
  return (
    <div className="grid gap-4">
      <SectionHead title="Runtime" about="The installed benchmark or built world that runs this world. Deploying an experiment needs one." />
      <Card>
        <div className="p-[18px]">
          <ChoiceField label="Runtime package" value={world.package ?? ''} options={options} onChange={(v) => onChange(setIn(world, ['package'], v || undefined))} />
        </div>
      </Card>
    </div>
  );
}
