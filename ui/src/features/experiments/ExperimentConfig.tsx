// An experiment's configuration, folded under its results: what every run started with (seed,
// world time window, spend cap) and each setup's runner and agents.
import { Stat } from '@/components/ui/page';
import { RUNNER_WORDS } from '@/features/start/runners';
import { usd } from '@/lib/format';
import type { Experiment } from '@/model/experiment';
import type { SetupReport } from '@/model/report';

const when = (iso: string) => new Date(iso).toLocaleString();

function SetupLine({ s }: { s: SetupReport }) {
  const agents = s.team.agents.map((a) => `${a.name} (${a.model ?? a.kind})`).join(', ');
  return (
    <li className="flex flex-col px-3 py-2">
      <span className="text-sm font-medium">{s.name}</span>
      <span className="text-xs text-muted">{RUNNER_WORDS[s.runner] ?? s.runner} · {agents}</span>
    </li>
  );
}

function One({ e, titled }: { e: Experiment; titled: boolean }) {
  return (
    <section className="flex flex-col gap-3">
      {titled && <h3 className="text-xs font-semibold">{e.name}</h3>}
      <div className="flex flex-wrap gap-x-8 gap-y-2">
        <Stat label="Benchmark" value={e.benchmark} />
        <Stat label="Runs per setup" value={e.runs} />
        <Stat label="Seed" value={e.seed ?? '–'} />
        <Stat label="World time window" value={e.world_minutes != null ? `${e.world_minutes} min` : 'None'} />
        <Stat label="Spend cap per run" value={e.max_cost_usd != null ? usd(e.max_cost_usd) : 'No cap'} />
        <Stat label="Started" value={when(e.created_at)} />
        {e.ended_at && <Stat label="Ended" value={when(e.ended_at)} />}
      </div>
      <ul className="divide-y divide-line rounded-md border border-line">{e.setups.map((s) => <SetupLine key={s.id} s={s} />)}</ul>
    </section>
  );
}

export default function ExperimentConfig({ list }: { list: Experiment[] }) {
  return (
    <details className="rounded-lg border border-line bg-surface">
      <summary className="cursor-pointer px-4 py-3 text-sm">
        <span className="font-semibold">Configuration</span>
        <span className="ml-2 text-xs text-muted">Setups' agents, seed, world time window, spend cap</span>
      </summary>
      <div className="flex flex-col gap-5 border-t border-line p-4">
        {list.map((e) => <One key={e.id} e={e} titled={list.length > 1} />)}
      </div>
    </details>
  );
}
