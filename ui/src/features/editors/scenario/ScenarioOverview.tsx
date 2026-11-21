// The scenario at a glance: the ticket, then its events in the order they fire.
import { Card } from '@/components/ui/card';
import type { ScenarioBody } from '@/model/project';
import { SectionHead } from '../fields';
import { timeline } from './events';

export default function ScenarioOverview({ scenario }: { scenario: ScenarioBody }) {
  const { timed, byCalls } = timeline(scenario.events ?? []);
  return (
    <div className="grid gap-4">
      <SectionHead title="Overview" about="What goes in, what must hold during the run, and what must be true at the end." />
      <Card title="Ticket"><p className="whitespace-pre-wrap px-[18px] py-3 text-sm">{scenario.task || 'No ticket yet.'}</p></Card>
      <Card title="Timeline" description={`${timed.length + byCalls.length} events · ${scenario.gates?.length ?? 0} gates · ${scenario.checks?.length ?? 0} checks`}>
        <ol className="divide-y divide-line">
          {timed.map((e, i) => <li key={`t${i}`} className="flex gap-3 px-[18px] py-2 text-sm"><span className="w-14 font-mono text-faint">{e.at}</span>{e.name}</li>)}
          {byCalls.map((e, i) => <li key={`c${i}`} className="flex gap-3 px-[18px] py-2 text-sm"><span className="w-14 font-mono text-faint">#{e.after_calls}</span>{e.name}</li>)}
        </ol>
      </Card>
    </div>
  );
}
