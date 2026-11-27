// Which installed runtime runs this world: picked from the installed benchmarks and built worlds,
// or installed here (from a git URL or an archive) and then picked.
import { Link } from 'react-router-dom';
import { worlds } from '@/api/worlds';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { useQuery } from '@/hooks/useQuery';
import type { WorldBody } from '@/model/project';
import InstallRuntime from '../../runtimes/InstallRuntime';
import { setIn } from '../edit';
import { ChoiceField, SectionHead } from '../fields';

/** The runtimes a world can name: `None yet`, then each installed one by title. */
export function runtimeOptions(installed: { name: string; title: string }[]) {
  return [{ value: '', label: 'None yet' }, ...installed.map((w) => ({ value: w.name, label: w.title && w.title !== w.name ? `${w.title} (${w.name})` : w.name }))];
}

export default function RuntimeSection({ world, onChange }: { world: WorldBody; onChange: (w: WorldBody) => void }) {
  const installed = useQuery('installed-worlds', worlds);
  return (
    <div className="grid gap-4">
      <SectionHead title="Runtime" about="The installed runtime (a benchmark package or a built world) that runs this world: its parts, its tasks and its scoring. Deploying an experiment needs one." />
      <Card footer={<><span className="flex-1">Not listed? Install it, then pick it here.</span><InstallRuntime onInstalled={installed.reload} trigger={<Button size="sm">Install a runtime</Button>} /><Link to="/runtimes" className="text-primary">Manage runtimes</Link></>}>
        <div className="p-[18px]">
          <ChoiceField label="Runtime package" value={world.package ?? ''} options={runtimeOptions(installed.data ?? [])} onChange={(v) => onChange(setIn(world, ['package'], v || undefined))} />
        </div>
      </Card>
    </div>
  );
}
