// The cards a person decides on: a diff to apply, an action to confirm, or a delete, which shows
// what uses the item and lets the person delete it alone or with what uses it.
import { useState } from 'react';
import type { Card } from '@/api/assistant';
import { Button } from '@/components/ui/button';
import DeletePanel from '@/features/editors/DeletePanel';
import { projectChanged } from '@/hooks/useProjectItems';
import { errorMessage } from '@/lib/errors';
import { applyCard, asKind, cardTitle, split } from './applyCard';

type State = 'open' | 'busy' | 'done' | 'dismissed';

/** A card's state, starting from how it was settled before; settling it tells `onSettle`. */
function useApply(card: Card, onSettle: (outcome: 'done' | 'dismissed') => void) {
  const [state, setState] = useState<State>(card.outcome ?? 'open');
  const [error, setError] = useState<string | null>(null);
  const settle = (outcome: 'done' | 'dismissed') => { setState(outcome); onSettle(outcome); };
  const apply = async () => {
    setState('busy');
    try { await applyCard(card); settle('done'); projectChanged(); } catch (e) { setError(errorMessage(e)); setState('open'); }
  };
  return { state, error, apply: () => { void apply(); }, done: () => settle('done'), dismiss: () => settle('dismissed') };
}

function Ended({ state }: { state: State }) {
  return state === 'done'
    ? <p className="border-t border-line bg-sunken px-3 py-2 text-xs text-ok">Done.</p>
    : <p className="border-t border-line bg-sunken px-3 py-2 text-xs text-faint">Dismissed.</p>;
}

function Footer({ go, label, onDismiss, dismissLabel, state }: { go: () => void; label: string; onDismiss: () => void; dismissLabel: string; state: State }) {
  if (state === 'done' || state === 'dismissed') return <Ended state={state} />;
  return (
    <div className="flex gap-2 border-t border-line bg-sunken px-3 py-2">
      <Button variant="primary" size="sm" disabled={state === 'busy'} onClick={go}>{label}</Button>
      <span className="flex-1" />
      <Button variant="ghost" size="sm" onClick={onDismiss}>{dismissLabel}</Button>
    </div>
  );
}

function Body({ card, a }: { card: Card; a: ReturnType<typeof useApply> }) {
  const [area, verb] = split(card.tool);
  const kind = asKind(area);
  if (card.kind === 'confirm' && kind && verb === 'delete') {
    const id = String(card.arguments.id ?? '');
    if (a.state === 'done' || a.state === 'dismissed') return <Ended state={a.state} />;
    return <DeletePanel kind={kind} id={id} label={id} onDeleted={a.done} onCancel={a.dismiss} />;
  }
  return (
    <>
      {card.kind === 'diff'
        ? <pre className="max-h-48 overflow-auto bg-sunken px-3 py-2 font-mono text-2xs leading-relaxed">{card.diff}</pre>
        : <pre className="px-3 py-2 font-mono text-2xs text-muted">{JSON.stringify(card.arguments, null, 2)}</pre>}
      {a.error && <p role="alert" className="px-3 py-1 text-xs text-alarm">{a.error}</p>}
      <Footer go={a.apply} label={card.kind === 'diff' ? 'Apply' : 'Confirm'} onDismiss={a.dismiss} dismissLabel={card.kind === 'diff' ? 'Discard' : 'Cancel'} state={a.state} />
    </>
  );
}

export function CardView({ card, onSettle }: { card: Card; onSettle: (outcome: 'done' | 'dismissed') => void }) {
  const a = useApply(card, onSettle);
  return (
    <div className="overflow-hidden rounded-lg border border-line-strong bg-surface shadow-panel">
      <div className="flex items-center gap-2 border-b border-line px-3 py-2 text-sm font-medium">
        <span className="flex-1">{cardTitle(card)}</span>
        {card.kind === 'confirm' && <span className="rounded-full bg-warn-soft px-2 py-0.5 text-2xs text-warn">Needs your OK</span>}
      </div>
      <Body card={card} a={a} />
    </div>
  );
}
