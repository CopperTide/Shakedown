import { Sparkles } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { askStep, type StoredQuestion, stepQuestions } from '@/api/questions';
import { Button } from '@/components/ui/button';
import { Hint } from '@/components/ui/hint';
import { Input, Textarea } from '@/components/ui/input';
import { ErrorState } from '@/components/ui/state';
import { POLL_MS } from '@/features/review/useReview';
import { useServerReview } from '@/features/review/setting';
import { errorMessage } from '@/lib/errors';
import type { Evidence, StepQuestion } from '@/model/review';
import { formatCost } from './timeline';

/** A step's questions, followed while one is being answered. */
function useQuestions(branchId: string, seq: number) {
  const [list, setList] = useState<StoredQuestion[]>([]);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    stepQuestions(branchId, seq).then((l) => { setList(l); setError(null); }, (e) => setError(errorMessage(e)));
  }, [branchId, seq]);
  useEffect(() => { setList([]); load(); }, [load]);
  const running = list.some((q) => q.status === 'running');
  useEffect(() => {
    if (!running) return;
    const t = setInterval(load, POLL_MS);
    return () => clearInterval(t);
  }, [running, load]);
  return { list, error, load, add: (q: StoredQuestion) => setList((l) => [...l, q]) };
}

/** Who asked, when, with which model, and what the answer cost. */
export function askedLine(q: StoredQuestion): string {
  const who = q.review?.mine ? 'You' : 'A teammate';
  const when = new Date(q.created_at).toLocaleString();
  const cost = q.status === 'done' ? ` · ${formatCost(q.cost_usd, q.cost_source)}${q.tokens != null ? ` · ${q.tokens.toLocaleString()} tokens` : ''}` : '';
  return `${who} · ${when} · ${q.model}${cost}`;
}

function Quote({ e, onStep }: { e: Evidence; onStep: (seq: number) => void }) {
  return (
    <li className="flex flex-col gap-0.5">
      <blockquote className="border-l-2 border-accent pl-2 font-mono text-2xs leading-4 break-words whitespace-pre-wrap">{e.quote}</blockquote>
      <span className="text-2xs text-muted">
        {e.step != null ? <button type="button" className="underline hover:text-fg" onClick={() => onStep(e.step as number)}>step {e.step}</button> : e.source}
        {' · '}{e.field}{e.why && ` · ${e.why}`}
      </span>
    </li>
  );
}

/** A finished answer: the answer, the evidence quoted from the record, and the speculation labelled as such. */
export function Answer({ a, dropped, onStep }: { a: StepQuestion; dropped: number; onStep: (seq: number) => void }) {
  return (
    <div className="flex flex-col gap-2 text-xs">
      <p className="leading-5">{a.answer}</p>
      <div className="flex flex-col gap-1">
        <h4 className="text-2xs font-semibold text-muted"><Hint hint="What the agent had, quoted from the record.">Evidence</Hint></h4>
        {a.evidence?.length ? <ul className="flex flex-col gap-1.5">{a.evidence.map((e, i) => <Quote key={i} e={e} onStep={onStep} />)}</ul>
          : <p className="text-2xs text-muted"><Hint hint="No quote of the record supports this answer.">None</Hint></p>}
        {dropped > 0 && <p className="text-2xs text-faint"><Hint hint="Not found in the record.">{dropped} citation{dropped === 1 ? '' : 's'} dropped</Hint></p>}
      </div>
      {!!a.speculation?.length && (
        <div className="flex flex-col gap-1">
          <h4 className="text-2xs font-semibold text-muted"><Hint hint="Inference, not in the record.">Speculation</Hint></h4>
          <ul className="flex list-disc flex-col gap-0.5 pl-4 italic">{a.speculation.map((g, i) => <li key={i}>{g.text}</li>)}</ul>
        </div>
      )}
    </div>
  );
}

function Asked({ q, onStep }: { q: StoredQuestion; onStep: (seq: number) => void }) {
  const r = q.review;
  return (
    <li className="flex flex-col gap-1.5 rounded-md bg-surface px-2 py-1.5">
      <p className="text-xs font-medium">{r?.question}</p>
      {q.status === 'running' && <p className="flex items-center gap-1.5 text-xs text-muted" aria-busy="true"><Sparkles className="size-3 animate-pulse" aria-hidden="true" />Looking into it…</p>}
      {q.status === 'error' && <p className="text-xs text-alarm">{q.error ?? 'The explainer failed.'}</p>}
      {q.status === 'done' && r && <Answer a={r} dropped={q.issues.filter((i) => i.startsWith('dropped evidence')).length} onStep={onStep} />}
      <p className="text-2xs text-faint">{askedLine(q)}</p>
    </li>
  );
}

/** "Ask about this step": a question to a model on your own key, and the questions asked before. */
export default function AskStep({ branchId, seq, onStep }: { branchId: string; seq: number; onStep: (seq: number) => void }) {
  const settings = useServerReview();
  const { list, error, add } = useQuestions(branchId, seq);
  const [question, setQuestion] = useState('');
  const [model, setModel] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [askError, setAskError] = useState<string | null>(null);
  if (!settings?.ask) return null;
  const submit = async () => {
    setBusy(true); setAskError(null);
    try { add(await askStep(branchId, seq, question, model ?? settings.model)); setQuestion(''); } catch (e) { setAskError(errorMessage(e)); }
    setBusy(false);
  };
  return (
    <section aria-label="Ask about this step" className="flex flex-col gap-2 border-t border-line px-3 py-2">
      <h3 className="text-2xs font-medium text-muted"><Hint hint="A model on your own key answers, quoting the record as evidence. ⌘/Ctrl + Enter asks.">Ask about this step</Hint></h3>
      {list.length > 0 && <ul className="flex flex-col gap-2">{list.map((q) => <Asked key={q.key} q={q} onStep={onStep} />)}</ul>}
      {error && <ErrorState error={error} />}
      <Textarea aria-label="Your question" rows={2} placeholder="Why did the agent do this?" value={question} className="font-sans"
        onChange={(e) => setQuestion(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && question.trim()) void submit(); }} />
      <div className="flex items-center gap-2">
        <Input aria-label="Explainer model" title="The model that answers, on your own key: <provider>:<model>" className="h-7 flex-1 font-mono text-2xs"
          value={model ?? settings.model} onChange={(e) => setModel(e.target.value)} />
        <Button size="sm" variant="primary" disabled={busy || !question.trim()} onClick={() => void submit()}>Ask</Button>
      </div>
      {askError && <p className="text-xs text-alarm">{askError}</p>}
    </section>
  );
}
