// Which installed runtime runs this world: picked from the installed benchmarks and built worlds,
// or installed here (from a git URL or an archive) and then picked.
import { Link } from 'react-router-dom';
import { type Runtimes, runtimes } from '@/api/runtimes';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { useQuery } from '@/hooks/useQuery';
import type { WorldBody } from '@/model/project';
import InstallRuntime from '../../runtimes/InstallRuntime';
import { setIn } from '../edit';
import { ChoiceField, SectionHead } from '../fields';

/** The runtimes a world can name: `None yet`, then every installed benchmark (each install's, then those installed with Shakedown). */
export function runtimeOptions(listed: Runtimes | null) {
  const from = (listed?.installs ?? []).flatMap((i) => i.packages.filter((p) => p.kind === 'benchmark').map((p) => ({ value: p.name, label: `${p.name} · ${i.id}` })));
  const builtin = (listed?.builtin ?? []).filter((p) => p.kind === 'benchmark').map((p) => ({ value: p.name, label: p.name }));
  return [{ value: '', label: 'None yet' }, ...from, ...builtin];
}

/** The installed runtimes, read once per page. */
export const useRuntimes = () => useQuery('runtimes', runtimes);

export default function RuntimeSection({ world, onChange }: { world: WorldBody; onChange: (w: WorldBody) => void }) {
  const installed = useRuntimes();
  return (
    <div className="grid gap-4">
      <SectionHead title="Runtime" about="The installed runtime (a benchmark package or a built world) that runs this world: its parts, its tasks and its scoring. Deploying an experiment needs one." />
      <Card footer={<><span className="flex-1">Not listed? Install it, then pick it here.</span><InstallRuntime onInstalled={installed.reload} trigger={<Button size="sm">Install a runtime</Button>} /><Link to="/runtimes" className="text-primary">Manage runtimes</Link></>}>
        <div className="p-[18px]">
          <ChoiceField label="Runtime package" value={world.package ?? ''} options={runtimeOptions(installed.data)} onChange={(v) => onChange(setIn(world, ['package'], v || undefined))} />
        </div>
      </Card>
    </div>
  );
}
