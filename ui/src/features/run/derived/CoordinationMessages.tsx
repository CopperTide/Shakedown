import { useMemo, useState } from 'react';
import { Search } from 'lucide-react';
import Markdown from '@/components/Markdown';
import { Button } from '@/components/ui/button';
import { Input, NativeSelect } from '@/components/ui/input';
import type { Event } from '@/model/types';

import { exchangesOf } from './coordination';

export default function CoordinationMessages({ events, onSelect }: { events: Event[]; onSelect: (seq: number) => void }) {
  const [query, setQuery] = useState('');
  const [agent, setAgent] = useState('');
  const exchanges = useMemo(() => exchangesOf(events), [events]);
  const agents = [...new Set(exchanges.flatMap((e) => [e.from, e.to]))].sort();
  const shown = exchanges.filter((e) => (!agent || e.from === agent || e.to === agent)
    && `${e.from} ${e.to} ${e.kind} ${e.text}`.toLowerCase().includes(query.toLowerCase()));
  return (
    <section aria-label="Recorded messages" className="min-w-0 rounded-lg border border-line">
      <div className="flex flex-wrap items-center gap-2 border-b border-line p-3">
        <h3 className="mr-auto font-semibold">Messages <span className="ml-1 text-xs font-normal tabular-nums text-muted">{shown.length} of {exchanges.length}</span></h3>
        <div className="relative min-w-40 flex-1 sm:max-w-64">
          <Search className="absolute left-2 top-2 size-3.5 text-muted" aria-hidden="true" />
          <Input aria-label="Search messages" placeholder="Search messages…" value={query} onChange={(e) => setQuery(e.target.value)} className="h-8 pl-7 text-xs" />
        </div>
        <NativeSelect aria-label="Messages involving agent" value={agent} onChange={(e) => setAgent(e.target.value)} className="h-8 w-auto max-w-full text-xs">
          <option value="">All participants</option>{agents.map((a) => <option key={a} value={a}>{a}</option>)}
        </NativeSelect>
      </div>
      <div className="max-h-[36rem] divide-y divide-line overflow-y-auto">
        {shown.map((e) => (
          <article key={e.seq} className="p-3">
            <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
              <span className="font-semibold">{e.from}</span><span className="text-muted">→</span><span className="font-medium">{e.to}</span>
              <span className="rounded border border-line px-1.5 py-0.5 text-2xs text-muted">{e.kind}</span>
              <Button size="sm" variant="ghost" className="ml-auto h-6 text-2xs" onClick={() => onSelect(e.seq)} aria-label={`View message at step ${e.seq}`}>Step {e.seq} ↗</Button>
            </div>
            {e.text ? <Markdown text={e.text} /> : <p className="text-sm text-muted">No text</p>}
          </article>
        ))}
        {!shown.length && <p className="p-5 text-sm text-muted">{exchanges.length ? 'No matches' : 'No messages'}</p>}
      </div>
    </section>
  );
}
