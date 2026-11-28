// The Coordination tab: the run's agents and what passed between them.
import { Hint } from '@/components/ui/hint';
import type { Event } from '@/model/types';
import CoordinationMessages from './CoordinationMessages';
import { coordination } from './coordination';

export default function Coordination({ events, onSelect }: { events: Event[]; onSelect: (seq: number) => void }) {
  const { agents, links } = coordination(events);
  return (
    <div className="grid gap-3 p-4 text-sm">
      <p className="text-muted"><Hint hint="A team described in MADL also shows its declared connections on its setup page.">{agents.length} agent{agents.length === 1 ? '' : 's'}</Hint>: {agents.join(', ') || 'none recorded'}</p>
      {links.length === 0 ? <p className="text-faint">No connections</p> : (
        <table className="w-full text-left">
          <thead className="font-mono text-2xs uppercase text-faint"><tr><th className="py-1">From</th><th>To</th><th className="text-right">Messages</th><th className="text-right">Handoffs</th><th className="text-right">Delegations</th></tr></thead>
          <tbody className="divide-y divide-line">
            {links.map((l) => (
              <tr key={`${l.from}-${l.to}`}><td className="py-1.5 font-medium">{l.from}</td><td>{l.to}</td><td className="text-right tabular-nums">{l.messages}</td><td className="text-right tabular-nums">{l.handoffs}</td><td className="text-right tabular-nums">{l.delegations}</td></tr>
            ))}
          </tbody>
        </table>
      )}
      <CoordinationMessages events={events} onSelect={onSelect} />
    </div>
  );
}
