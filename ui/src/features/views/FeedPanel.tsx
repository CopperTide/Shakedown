import { useMemo } from 'react';
import { cn } from '@/lib/cn';
import { UTC, type Zone, minutes, timeIn } from './format';
import { PanelEmpty } from './PanelFrame';
import { cols } from './rows';
import { Chip, TONE_RULE } from './tones';
import { type At, type FeedData, asTone } from './types';
import { useZone } from './zone';

/** An event's world time in the world's zone ("10:05", "14:05Z" in UTC) from its epoch, else the clock as "+H:MM". */
export function feedTime(clockMs: number | null, at: Pick<At, 'world_ms' | 'clock_ms'> | null, zone: Zone = UTC): string {
  if (clockMs == null) return '';
  if (at?.world_ms != null && at.clock_ms != null) return timeIn(at.world_ms - at.clock_ms + clockMs, zone);
  return `+${minutes(clockMs / 60_000)}`;
}

/** The world's events, newest first: the world time, the kind, the text, the tone as a left rule. */
export default function FeedPanel({ data, at }: { data: FeedData; at: At | null }) {
  const c = useMemo(() => cols(data.columns), [data.columns]);
  const zone = useZone();
  if (!data.rows.length) return <PanelEmpty>No events yet.</PanelEmpty>;
  return (
    <ol className="absolute inset-0 overflow-y-auto">
      {data.rows.map((row, i) => {
        const tone = asTone(c.get(row, 'tone'));
        const chip = c.str(row, 'chip');
        return (
          <li key={`${c.str(row, 'seq')}-${i}`} className={cn('flex gap-2 border-b border-l-2 border-b-line/70 py-[3px] pr-3 pl-2.5', TONE_RULE[tone])}>
            <span className="w-[42px] shrink-0 font-mono text-[11px] leading-[17px] text-faint tabular-nums">{feedTime(c.num(row, 'clock_ms'), at, zone)}</span>
            <span className="min-w-0 flex-1 text-xs leading-[17px]">
              <span className="mr-1.5 font-mono text-[10.5px] text-muted">{c.str(row, 'kind')}</span>
              {chip && <Chip tone={tone} className="mr-1.5">{chip}</Chip>}
              <span className={cn(tone === 'critical' && 'text-tone-critical')}>{c.str(row, 'text')}</span>
            </span>
          </li>
        );
      })}
    </ol>
  );
}
