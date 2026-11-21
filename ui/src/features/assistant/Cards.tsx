// The cards a person decides on: a diff to apply, or an action to confirm.
import { useState } from 'react';
import type { Card } from '@/api/assistant';
import { Button } from '@/components/ui/button';
import { errorMessage } from '@/lib/errors';
import { applyCard, split } from './applyCard';

function useApply(card: Card) {
  const [state, setState] = useState<'open' | 'busy' | 'done' | 'dismissed'>('open');
  const [error, setError] = useState<string | null>(null);
  const apply = async () => {
    setState('busy');
    try { await applyCard(card); setState('done'); } catch (e) { setError(errorMessage(e)); setState('open'); }
  };
  return { state, error, apply: () => { void apply(); }, dismiss: () => setState('dismissed') };
}

function Footer({ go, label, onDismiss, dismissLabel, state }: { go: () => void; label: string; onDismiss: () => void; dismissLabel: string; state: string }) {
  if (state === 'done') return <p className="border-t border-line bg-sunken px-3 py-2 text-xs text-ok">Done.</p>;
  if (state === 'dismissed') return <p className="border-t border-line bg-sunken px-3 py-2 text-xs text-faint">Dismissed.</p>;
  return (
    <div className="flex gap-2 border-t border-line bg-sunken px-3 py-2">
      <Button variant="primary" size="sm" disabled={state === 'busy'} onClick={go}>{label}</Button>
      <span className="flex-1" />
      <Button variant="ghost" size="sm" onClick={onDismiss}>{dismissLabel}</Button>
    </div>
  );
}

export function CardView({ card }: { card: Card }) {
  const a = useApply(card);
  const [area, verb] = split(card.tool);
  const title = card.kind === 'diff' ? `Edit ${area.replace(/s$/, '')} ${card.arguments.id}` : `${verb.replace(/_/g, ' ')} ${area}`;
  return (
    <div className="overflow-hidden rounded-lg border border-line-strong bg-surface shadow-panel">
      <div className="flex items-center gap-2 border-b border-line px-3 py-2 text-sm font-medium">
        <span className="flex-1 capitalize">{title}</span>
        {card.kind === 'confirm' && <span className="rounded-full bg-warn-soft px-2 py-0.5 text-2xs text-warn">Needs your OK</span>}
      </div>
      {card.kind === 'diff'
        ? <pre className="max-h-48 overflow-auto bg-sunken px-3 py-2 font-mono text-2xs leading-relaxed">{card.diff}</pre>
        : <pre className="px-3 py-2 font-mono text-2xs text-muted">{JSON.stringify(card.arguments, null, 2)}</pre>}
      {a.error && <p role="alert" className="px-3 py-1 text-xs text-alarm">{a.error}</p>}
      <Footer go={a.apply} label={card.kind === 'diff' ? 'Apply' : 'Confirm'} onDismiss={a.dismiss} dismissLabel={card.kind === 'diff' ? 'Discard' : 'Cancel'} state={a.state} />
    </div>
  );
}
