import { memo } from 'react';
import type { CallRow } from './agentStats';

const compact = (n: number) => new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 }).format(n);

/**
 * Context size over the agent's model calls: one bar per call, its input tokens, the cached part
 * shaded. Plain SVG; the labels sit outside it so a stretched plot
 * keeps them crisp. A click picks that call's step.
 */
export default memo(function ContextChart({ calls, selected, onSelect }: { calls: CallRow[]; selected: number | null; onSelect: (seq: number) => void }) {
  const known = calls.filter((c) => c.input != null);
  if (!known.length) return <p className="px-3 py-4 text-xs text-muted">No model call recorded its input tokens.</p>;
  const max = Math.max(1, ...known.map((c) => c.input!));
  const peak = known.reduce((a, c) => (c.input! > a.input! ? c : a));
  const n = calls.length;
  const pick = (ev: React.MouseEvent<SVGSVGElement>) => {
    const box = ev.currentTarget.getBoundingClientRect();
    const i = Math.min(n - 1, Math.max(0, Math.floor(((ev.clientX - box.left) / box.width) * n)));
    onSelect(calls[i].seq);
  };
  return (
    <div className="px-3 py-2">
      <div className="flex items-baseline justify-between text-2xs text-muted">
        <span>{compact(max)} tokens</span>
        <span>peak at step {peak.seq} · {calls.length.toLocaleString()} calls</span>
      </div>
      <svg viewBox={`0 0 ${n} 100`} preserveAspectRatio="none" className="block h-24 w-full cursor-pointer border-b border-line" onClick={pick}
        role="img" aria-label={`Input tokens per model call, up to ${max.toLocaleString()}`}>
        {calls.map((c, i) => {
          if (c.input == null) return null;
          const h = (c.input / max) * 100, ch = ((c.cached ?? 0) / max) * 100;
          const on = c.seq === selected;
          return (
            <g key={c.seq} opacity={c.inherited ? 0.45 : 1}>
              <rect x={i + 0.1} width={0.8} y={100 - h} height={h} fill={on ? 'var(--fg)' : 'var(--accent)'} />
              {ch > 0 && <rect x={i + 0.1} width={0.8} y={100 - ch} height={ch} fill={on ? 'var(--muted)' : 'var(--accent-soft)'} />}
              <title>{`Step ${c.seq}: ${c.input.toLocaleString()} in${c.cached ? ` (${c.cached.toLocaleString()} cached)` : ''}, ${(c.output ?? 0).toLocaleString()} out`}</title>
            </g>
          );
        })}
      </svg>
      <div className="mt-1 flex flex-wrap gap-x-4 text-2xs text-muted">
        <span className="flex items-center gap-1"><span aria-hidden className="size-2 rounded-sm bg-accent" />input tokens</span>
        <span className="flex items-center gap-1"><span aria-hidden className="size-2 rounded-sm bg-accent-soft" />of which cached</span>
        <span className="ml-auto">first call → last · click a bar to open its step</span>
      </div>
    </div>
  );
});
