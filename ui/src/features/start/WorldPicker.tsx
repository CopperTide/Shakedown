import { useNavigate } from 'react-router-dom';
import { chooseWorld, worlds as fetchWorlds } from '@/api/worlds';
import { NativeSelect } from '@/components/ui/input';
import { useAction } from '@/hooks/useAction';
import { useQuery } from '@/hooks/useQuery';
import type { BuiltWorld } from '@/model/world';

const BUILD = '__build__';

/** The worlds to run in, by title, the current one chosen; the last option builds a new one. */
export default function WorldPicker({ value, worlds, onChoose, busy }: { value: string; worlds: BuiltWorld[]; onChoose: (name: string) => void; busy: boolean }) {
  const nav = useNavigate();
  const pick = (v: string) => (v === BUILD ? nav('/worlds/new') : onChoose(v));
  return (
    <NativeSelect value={value} disabled={busy} aria-label="World" onChange={(e) => pick(e.target.value)} className="w-auto max-w-full self-start">
      {worlds.map((w) => <option key={w.name} value={w.name} title={w.template?.name}>{w.title}</option>)}
      <option value={BUILD}>Build a world…</option>
    </NativeSelect>
  );
}

/** The picker over shaked's worlds: choosing one makes it current and reloads the page, whose every read is of that world. */
export function CurrentWorldPicker({ busy }: { busy: boolean }) {
  const list = useQuery('worlds', fetchWorlds, { shared: true, fresh: true });
  const choose = useAction(async (name: string) => { await chooseWorld(name); window.location.reload(); }, { stayBusy: true });
  const all = list.data ?? [];
  const current = all.find((w) => w.current)?.name ?? '';
  if (!all.length) return null;
  return (
    <div className="flex flex-col gap-1">
      <WorldPicker value={current} worlds={all} onChoose={(n) => void choose.run(n)} busy={busy || choose.busy} />
      {choose.error && <span role="status" className="text-sm text-alarm">{`⚠ ${choose.error}`}</span>}
    </div>
  );
}
