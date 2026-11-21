// One turn of the conversation: the person's question, or the assistant's answer with the tools
// it used and any cards.
import type { Card, ToolUse } from '@/api/assistant';
import { CardView } from './Cards';

export interface Turn { role: 'user' | 'assistant'; text: string; tools?: ToolUse[]; cards?: Card[] }

export default function Message({ turn }: { turn: Turn }) {
  if (turn.role === 'user') return <div className="max-w-[88%] self-end rounded-xl rounded-br-sm bg-sunken px-3 py-2 text-sm">{turn.text}</div>;
  const n = turn.tools?.length ?? 0;
  return (
    <div className="grid gap-2 text-sm">
      {n > 0 && (
        <details className="rounded-md border border-line bg-sunken text-xs">
          <summary className="cursor-pointer px-2.5 py-1.5 text-muted">Used {n} tool{n > 1 ? 's' : ''}</summary>
          <ol className="grid gap-0.5 border-t border-line px-6 py-1.5 font-mono">{turn.tools?.map((t, i) => <li key={i}>{t.name}</li>)}</ol>
        </details>
      )}
      <p className="whitespace-pre-wrap">{turn.text}</p>
      {turn.cards?.map((c, i) => <CardView key={i} card={c} />)}
    </div>
  );
}
