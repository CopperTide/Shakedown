import { memo } from 'react';
import Markdown from '@/components/Markdown';
import { cn } from '@/lib/cn';
import { reasoningWord } from '@/model/lanes';
import type { ReasoningRow } from './agentStats';
import { ShowMore, useShown } from './paging';

const PAGE = 20;

/** A link-styled button that selects a step. */
export function StepLink({ seq, label, onSelect }: { seq: number; label: string; onSelect: (seq: number) => void }) {
  return <button type="button" onClick={() => onSelect(seq)} className="font-mono text-2xs text-accent underline-offset-2 hover:underline">{label}</button>;
}

/**
 * The agent's reasoning, in order, a page at a time (a long run has hundreds, some of 30k
 * characters). Each item opens its model call, where the context the model read is, or itself.
 */
export default memo(function Trail({ rows, agent, selected, onSelect }: { rows: ReasoningRow[]; agent: string; selected: number | null; onSelect: (seq: number) => void }) {
  const [shown, more] = useShown(PAGE, agent);
  if (!rows.length) return <p className="px-3 py-4 text-xs text-muted">This agent recorded no reasoning.</p>;
  return (
    <>
      <ol className="flex flex-col">
        {rows.slice(0, shown).map((r, i) => (
          <li key={r.seq} data-active={r.seq === selected || r.modelCallSeq === selected ? '' : undefined}
            className={cn('flex flex-col gap-1 border-b border-line px-3 py-2 data-[active]:bg-accent-soft', r.inherited && 'opacity-60')}>
            <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-2xs text-muted">
              <span className="font-medium text-fg">{i + 1}. {reasoningWord(r.kind)}</span>
              {r.modelCallSeq != null && <StepLink seq={r.modelCallSeq} label={`context · model call ${r.modelCallSeq}`} onSelect={onSelect} />}
              <StepLink seq={r.seq} label={`step ${r.seq}`} onSelect={onSelect} />
              {r.inherited && <span>before the fork</span>}
              {r.truncated && <span>cut at 32,768 characters</span>}
            </div>
            <Markdown text={r.text} className="max-h-64 overflow-y-auto rounded bg-sunken px-2 py-1.5" />
          </li>
        ))}
      </ol>
      <ShowMore shown={shown} total={rows.length} page={PAGE} onMore={more} />
    </>
  );
});
