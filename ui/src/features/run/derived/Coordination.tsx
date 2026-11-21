// The Coordination tab: the run's agents and what passed between them.
import type { Event } from '@/model/types';
import { coordination } from './coordination';

export default function Coordination({ events }: { events: Event[] }) {
  const { agents, links } = coordination(events);
  return (
    <div className="grid gap-3 p-4 text-sm">
      <p className="text-muted">{agents.length} agent{agents.length === 1 ? '' : 's'}: {agents.join(', ') || 'none recorded'}. A team described in MADL also shows its declared connections on its setup page.</p>
      {links.length === 0 ? <p className="text-faint">Agents didn't message each other in this run.</p> : (
        <table className="w-full text-left">
          <thead className="font-mono text-2xs uppercase text-faint"><tr><th className="py-1">From</th><th>To</th><th className="text-right">Messages</th><th className="text-right">Handoffs</th><th className="text-right">Delegations</th></tr></thead>
          <tbody className="divide-y divide-line">
            {links.map((l) => (
              <tr key={`${l.from}-${l.to}`}><td className="py-1.5 font-medium">{l.from}</td><td>{l.to}</td><td className="text-right tabular-nums">{l.messages}</td><td className="text-right tabular-nums">{l.handoffs}</td><td className="text-right tabular-nums">{l.delegations}</td></tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
