import { useLayoutEffect, useMemo, useRef } from 'react';
import { cn } from '@/lib/cn';
import { PanelEmpty } from './PanelFrame';
import { cols } from './rows';
import { Chip, TONE_RULE } from './tones';
import { type ChatData, type ChatDef, asTone } from './types';

/** A message's text: a string as it is, nothing as empty, anything else as JSON. */
export function chatText(v: unknown): string {
  if (v == null) return '';
  return typeof v === 'string' ? v : JSON.stringify(v);
}

/** The panel's count: the messages shown, and of how many when `top` cut some. */
export function chatMeta(data: ChatData): string {
  const shown = data.rows.length, total = data.total ?? shown;
  return total > shown ? `${shown} of ${total}` : String(shown);
}

/** A conversation, oldest first: each message a bubble under its sender, whitespace kept, on the right when
 * its sender is one of `right`, its tone as a rule; scrolled to the newest. */
export default function ChatPanel({ def, data }: { def: ChatDef; data: ChatData }) {
  const c = useMemo(() => cols(data.columns), [data.columns]);
  const right = useMemo(() => new Set((data.right ?? def.right ?? []).map(String)), [data.right, def.right]);
  const box = useRef<HTMLOListElement>(null);
  useLayoutEffect(() => {
    if (box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [data.rows]);
  if (!data.rows.length) return <PanelEmpty>No messages yet.</PanelEmpty>;
  return (
    <ol ref={box} className="absolute inset-0 flex flex-col gap-2 overflow-y-auto px-3 py-2.5">
      {data.rows.map((row, i) => {
        const from = c.str(row, 'from');
        const side = right.has(from) ? 'right' : 'left';
        const tone = asTone(c.get(row, 'tone'));
        const chip = c.str(row, 'chip');
        return (
          <li key={`${c.str(row, 'key')}-${i}`} data-side={side}
            className={cn('flex max-w-[85%] flex-col gap-0.5', side === 'right' ? 'items-end self-end' : 'items-start self-start')}>
            <span className="flex items-center gap-1.5 font-mono text-[10.5px] leading-[15px] text-muted">
              {from || '—'}
              {chip && <Chip tone={tone}>{chip}</Chip>}
            </span>
            <div className={cn('max-w-full rounded-[6px] border border-line px-2.5 py-1.5 text-xs leading-[17px] break-words whitespace-pre-wrap text-fg',
              side === 'right' ? 'bg-sunken' : 'bg-raised', tone !== 'neutral' && cn('border-l-2', TONE_RULE[tone]))}>
              {chatText(c.get(row, 'text'))}
            </div>
          </li>
        );
      })}
    </ol>
  );
}
