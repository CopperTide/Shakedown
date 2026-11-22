import { cn } from '@/lib/cn';
import { formatValue } from './format';
import { Chip, TONE_TEXT } from './tones';
import { type KpisData, type KpisDef, asTone } from './types';

/** A strip of figures in the file's order: the label in small caps, the value large and toned. */
export default function KpisPanel({ def, data }: { def: KpisDef; data: KpisData }) {
  const items = def.items ?? [];
  return (
    <div className="flex h-full min-w-0 flex-wrap items-stretch">
      {items.map((item, i) => {
        const got = data.items?.[i];
        const tone = asTone(got?.tone);
        const text = formatValue(got?.value, item.format, item.decimals, got?.chip);
        return (
          <div key={`${i}-${item.label}`} className="flex min-w-[124px] flex-1 flex-col justify-center gap-0.5 border-r border-line px-3 py-1.5 last:border-r-0">
            <span className="truncate text-[11px] leading-4 font-medium tracking-[0.04em] text-muted [font-variant-caps:all-small-caps]" title={item.label}>{item.label}</span>
            {item.format === 'chip'
              ? <span className="py-0.5"><Chip tone={tone} className="text-xs leading-5">{text}</Chip></span>
              : <span className={cn('font-mono text-[22px] leading-7 font-medium tracking-tight whitespace-nowrap tabular-nums', TONE_TEXT[tone])}>{text}</span>}
          </div>
        );
      })}
    </div>
  );
}
