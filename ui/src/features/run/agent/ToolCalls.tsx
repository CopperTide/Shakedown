import { memo } from 'react';
import { StatusMark } from '@/components/ui/status';
import { alarmMark } from '@/lib/mark';
import { cn } from '@/lib/cn';
import { formatDuration } from '@/model/lanes';
import type { ToolRow } from './agentStats';
import { ShowMore, useShown } from './paging';
import { StepLink } from './Trail';

const PAGE = 50;

/** The agent's tool calls, in order: the tool, how it went, and a line each of its arguments and result. */
export default memo(function ToolCalls({ rows, agent, selected, onSelect }: { rows: ToolRow[]; agent: string; selected: number | null; onSelect: (seq: number) => void }) {
  const [shown, more] = useShown(PAGE, agent);
  if (!rows.length) return <p className="px-3 py-4 text-xs text-muted">No tool calls</p>;
  return (
    <>
      <ol className="flex flex-col">
        {rows.slice(0, shown).map((t) => (
          <li key={t.seq} data-active={t.seq === selected ? '' : undefined}
            className={cn('flex min-w-0 flex-col gap-0.5 border-b border-line px-3 py-1.5 text-xs data-[active]:bg-accent-soft', t.inherited && 'opacity-60')}>
            <div className="flex flex-wrap items-center gap-x-2">
              <StepLink seq={t.seq} label={`step ${t.seq}`} onSelect={onSelect} />
              <span className="font-mono font-medium">{t.tool}</span>
              {t.outcome !== 'ok' && <StatusMark mark={alarmMark(t.outcome)} />}
              {t.durationMs != null && <span className="text-2xs text-muted">{formatDuration(t.durationMs)}</span>}
            </div>
            {t.args && <p className="truncate font-mono text-2xs text-muted" title={t.args}><span className="text-faint">args </span>{t.args}</p>}
            {t.result && <p className={cn('truncate font-mono text-2xs', t.error ? 'text-alarm' : 'text-muted')} title={t.result}><span className="text-faint">{t.error ? 'error ' : 'result '}</span>{t.result}</p>}
          </li>
        ))}
      </ol>
      <ShowMore shown={shown} total={rows.length} page={PAGE} onMore={more} />
    </>
  );
});
